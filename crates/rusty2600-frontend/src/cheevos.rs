//! RetroAchievements integration (`retroachievements` feature): owns the
//! [`rusty2600_cheevos::RaClient`] on the winit/main thread.
//!
//! `RaClient` is deliberately `!Send`/`!Sync` (see `rusty2600-cheevos`'s own
//! docs), so it CANNOT live inside [`crate::emu_thread::EmuCore`] — `EmuCore`
//! must stay `Send` for the default-on `emu-thread` feature. This mirrors the
//! debugger's own architecture (`crate::debugger`): the client lives here, on
//! the main thread, and [`crate::cheevos::CheevosState::pump`] is called under the SAME brief
//! emu lock the present path and the debug snapshot already take, peeking the
//! bus rather than holding the lock any longer than that one call.
//!
//! What this wires up: client construction, achievement/game loading from ROM
//! bytes, per-frame `do_frame`/`idle` pumping, hardcore mode, surfacing
//! unlock/server events as status-bar text, login/logout, and the
//! achievement-list/leaderboard/rich-presence/unlock-toast panel
//! (`crate::debugger::cheevos_panel`, `T-0802-005`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rusty2600_cheevos::{RaClient, RaEvent};

/// Login progress, tracked across the async `begin_login_*` completion.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum LoginState {
    /// Not logged in, no attempt in flight.
    #[default]
    LoggedOut,
    /// A `begin_login_*` call is awaiting its server round-trip.
    LoggingIn,
    /// Logged in successfully.
    LoggedIn,
    /// The last login attempt failed, with a short reason.
    Error(String),
}

/// One transient unlock notification, shown in
/// [`crate::debugger::cheevos_panel`]'s "Recent unlocks" list.
#[derive(Debug, Clone)]
pub struct Toast {
    /// The achievement's title.
    pub title: String,
    /// A short detail string (currently the point value).
    pub detail: String,
    /// `true` for an error-class notification (a server error, a lost
    /// connection) rather than a genuine achievement unlock.
    pub is_error: bool,
    /// The RA media-server URL of the unlocked badge PNG (empty if
    /// unavailable) — the panel can fetch/display it, or fall back to text.
    pub badge_url: String,
}

/// Persistent RetroAchievements state, owned by the app's `Active` struct.
pub struct CheevosState {
    client: RaClient,
    /// Whether a game has been successfully identified and loaded. Shared
    /// with the async `begin_load_game` completion closure via `Rc<Cell<_>>`
    /// since that closure is `'static` and runs later, detached from `self`.
    game_loaded: Rc<Cell<bool>>,
    /// Login progress. Shared with the async `begin_login_*` completion
    /// closures the same way `game_loaded` is — `RefCell` since the error
    /// variant carries an owned `String`, not just a `Copy` flag.
    login_state: Rc<RefCell<LoginState>>,
    /// Recent unlock/error notifications, newest last. The panel is
    /// responsible for trimming this (see `cheevos_panel`'s cap).
    pub toasts: Vec<Toast>,
    /// The Login panel's username text-input buffer.
    pub username_input: String,
    /// The Login panel's password text-input buffer.
    pub password_input: String,
}

impl Default for CheevosState {
    fn default() -> Self {
        Self {
            client: RaClient::new(),
            game_loaded: Rc::new(Cell::new(false)),
            login_state: Rc::new(RefCell::new(LoginState::LoggedOut)),
            toasts: Vec::new(),
            username_input: String::new(),
            password_input: String::new(),
        }
    }
}

impl CheevosState {
    /// Begin logging in with the current [`Self::username_input`]/
    /// [`Self::password_input`] (cleared on success; the password is
    /// cleared either way, since it should never linger in memory or the
    /// UI longer than the one login attempt that needed it).
    pub fn begin_login(&mut self) {
        *self.login_state.borrow_mut() = LoginState::LoggingIn;
        let state = Rc::clone(&self.login_state);
        self.client.begin_login_password(
            &self.username_input,
            &self.password_input,
            move |result| {
                *state.borrow_mut() = match result {
                    Ok(()) => LoginState::LoggedIn,
                    Err(e) => LoginState::Error(e),
                };
            },
        );
        self.password_input.clear();
    }

    /// Log out, resetting [`Self::login_state`] to [`LoginState::LoggedOut`].
    pub fn logout(&mut self) {
        self.client.logout();
        *self.login_state.borrow_mut() = LoginState::LoggedOut;
    }

    /// The current login progress.
    #[must_use]
    pub fn login_state(&self) -> LoginState {
        self.login_state.borrow().clone()
    }

    /// The logged-in user's profile, if any.
    #[must_use]
    pub fn user_info(&self) -> Option<rusty2600_cheevos::RaUser> {
        self.client.user_info()
    }

    /// The current game's achievement-count/points summary.
    #[must_use]
    pub fn game_summary(&self) -> rusty2600_cheevos::RaGameSummary {
        self.client.user_game_summary()
    }

    /// The current game's rich-presence string.
    pub fn rich_presence(&mut self) -> String {
        self.client.rich_presence()
    }

    /// The current game's full achievement list.
    pub fn achievement_list(&mut self) -> Vec<rusty2600_cheevos::RaAchievement> {
        self.client.achievement_list()
    }

    /// The current game's full leaderboard list.
    pub fn leaderboard_list(&mut self) -> Vec<rusty2600_cheevos::RaLeaderboard> {
        self.client.leaderboard_list()
    }
    /// Begin identifying + loading a game from raw ROM bytes. Fires
    /// asynchronously; [`Self::game_loaded`] flips to `true` once
    /// [`RaClient::poll_http_completions`] completes it successfully.
    pub fn load_rom(&mut self, rom: &[u8]) {
        self.game_loaded.set(false);
        let loaded = Rc::clone(&self.game_loaded);
        self.client.begin_load_game(rom, move |result| {
            // Errors (unrecognized ROM, no network, ...) are non-fatal here:
            // the emulator keeps running without achievements either way.
            loaded.set(result.is_ok());
        });
    }

    /// Note that a ROM was closed — clears the loaded-game flag so a stale
    /// client doesn't keep reporting achievements for the wrong game.
    pub fn close_rom(&mut self) {
        self.game_loaded.set(false);
        self.client.unload_game();
    }

    /// Drive one frame of achievement logic and return any newly fired
    /// events, formatted as short status-bar strings. `peek` is a
    /// side-effect-free bus read, e.g. `|addr| bus.peek(addr)`.
    pub fn pump(&mut self, peek: &mut dyn FnMut(u16) -> u8) -> Vec<String> {
        /// Caps [`CheevosState::toasts`] so a long unlock-heavy session
        /// (or a disconnect/reconnect flurry) can't grow it unbounded.
        const MAX_TOASTS: usize = 20;

        self.client.do_frame(peek);
        self.client.poll_http_completions();
        let events = self.client.take_events();
        for ev in &events {
            if let Some(toast) = Self::toast_for(ev) {
                self.toasts.push(toast);
                if self.toasts.len() > MAX_TOASTS {
                    self.toasts.remove(0);
                }
            }
        }
        events.into_iter().filter_map(Self::format_event).collect()
    }

    /// Builds a [`Toast`] for the event kinds worth surfacing there
    /// (unlocks and connection-health issues), mirroring
    /// [`Self::format_event`]'s coverage.
    fn toast_for(ev: &RaEvent) -> Option<Toast> {
        match ev {
            RaEvent::AchievementTriggered {
                title,
                points,
                badge_url,
                ..
            } => Some(Toast {
                title: title.clone(),
                detail: format!("{points} pts"),
                is_error: false,
                badge_url: badge_url.clone(),
            }),
            RaEvent::ServerError { msg, .. } => Some(Toast {
                title: "RetroAchievements error".into(),
                detail: msg.clone(),
                is_error: true,
                badge_url: String::new(),
            }),
            _ => None,
        }
    }

    /// `true`/`false` toggle for hardcore mode (disables save-states/rewind
    /// while achievements are being tracked, per RA convention).
    #[must_use]
    pub fn hardcore_enabled(&self) -> bool {
        self.client.get_hardcore_enabled()
    }

    /// Set hardcore mode.
    pub fn set_hardcore_enabled(&mut self, enabled: bool) {
        self.client.set_hardcore_enabled(enabled);
    }

    /// Whether a game has been successfully identified and loaded.
    #[must_use]
    pub fn game_loaded(&self) -> bool {
        self.game_loaded.get()
    }

    fn format_event(ev: RaEvent) -> Option<String> {
        match ev {
            RaEvent::AchievementTriggered { title, points, .. } => {
                Some(format!("Achievement unlocked: {title} ({points} pts)"))
            }
            RaEvent::GameCompleted => Some("All achievements earned!".into()),
            RaEvent::Disconnected => Some("RetroAchievements: server connection lost".into()),
            RaEvent::Reconnected => Some("RetroAchievements: server connection restored".into()),
            RaEvent::ServerError { msg, .. } => Some(format!("RetroAchievements error: {msg}")),
            // Leaderboards / progress / challenge indicators / reset: no
            // status-bar surface yet (needs a HUD, not just text).
            _ => None,
        }
    }
}
