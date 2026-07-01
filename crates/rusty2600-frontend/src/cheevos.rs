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
//! bytes, per-frame `do_frame`/`idle` pumping, hardcore mode, and surfacing
//! unlock/server events as status-bar text. What it does NOT (yet) provide: a
//! login dialog, an achievement-list panel, or rich-presence display — real
//! functionality either way (unlocks fire and hardcore mode gates play), just
//! without a dedicated UI surface yet.

use std::cell::Cell;
use std::rc::Rc;

use rusty2600_cheevos::{RaClient, RaEvent};

/// Persistent RetroAchievements state, owned by the app's `Active` struct.
pub struct CheevosState {
    client: RaClient,
    /// Whether a game has been successfully identified and loaded. Shared
    /// with the async `begin_load_game` completion closure via `Rc<Cell<_>>`
    /// since that closure is `'static` and runs later, detached from `self`.
    game_loaded: Rc<Cell<bool>>,
}

impl Default for CheevosState {
    fn default() -> Self {
        Self {
            client: RaClient::new(),
            game_loaded: Rc::new(Cell::new(false)),
        }
    }
}

impl CheevosState {
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
        self.client.do_frame(peek);
        self.client.poll_http_completions();
        self.client
            .take_events()
            .into_iter()
            .filter_map(Self::format_event)
            .collect()
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
