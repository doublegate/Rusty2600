//! [`WritesLocked`] — the determinism gate every script-driven WRITE
//! (`emu.poke`, `emu.setJoystick`, `emu.setConsoleSwitch`) is checked
//! against before it's allowed to reach the emulator.

/// Whether script-driven writes are currently forbidden, folding every real
/// determinism-lock source that exists today.
///
/// Two real sources as of the frontend-wiring pass that also wired rollback
/// netplay into `rusty2600-frontend`: [`Self::ra_hardcore`]
/// (`RetroAchievements` hardcore mode,
/// `rusty2600_cheevos::client::RaClient::get_hardcore_enabled`, `v1.9.0`)
/// and [`Self::netplay_active`] (a rollback netplay session is connected —
/// a script's `poke`/`setJoystick`/`setConsoleSwitch` calls are purely
/// LOCAL side effects that would silently desync the two peers' otherwise
/// bit-identical timelines, so they're forbidden for the same reason RA
/// hardcore mode forbids them: an unreplicated write breaks the
/// determinism contract the whole feature depends on). A `.r26m` movie
/// record/replay lock (`rusty2600_core::movie`, shipped `v1.7.0`) still
/// does NOT get a field here — it has no lock concept of its own yet, and
/// adding an always-`false` stub field would be dead weight pretending to
/// be a feature. The right time to add `movie_locked` is the same change
/// that gives movies a real lock to fold in, exactly the precedent this
/// field followed for netplay.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WritesLocked {
    /// `RetroAchievements` hardcore mode is active for the current session.
    pub ra_hardcore: bool,
    /// A rollback netplay session is currently connected.
    pub netplay_active: bool,
}

impl WritesLocked {
    /// Whether ANY real lock source is currently active.
    #[must_use]
    pub const fn locked(self) -> bool {
        self.ra_hardcore || self.netplay_active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlocked_by_default() {
        assert!(!WritesLocked::default().locked());
    }

    #[test]
    fn hardcore_locks() {
        let locked = WritesLocked {
            ra_hardcore: true,
            ..Default::default()
        };
        assert!(locked.locked());
    }

    #[test]
    fn netplay_locks() {
        let locked = WritesLocked {
            netplay_active: true,
            ..Default::default()
        };
        assert!(locked.locked());
    }
}
