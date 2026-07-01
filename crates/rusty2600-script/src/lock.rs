//! [`WritesLocked`] — the determinism gate every script-driven WRITE
//! (`emu.poke`, `emu.setJoystick`, `emu.setConsoleSwitch`) is checked
//! against before it's allowed to reach the emulator.

/// Whether script-driven writes are currently forbidden, folding every real
/// determinism-lock source that exists today.
///
/// Only one real source exists as of `v1.9.0`:
/// [`Self::ra_hardcore`] (`RetroAchievements` hardcore mode,
/// `rusty2600_cheevos::client::RaClient::get_hardcore_enabled`). Two more
/// are staged by the project's roadmap but do NOT get a field here yet,
/// deliberately: a `.r26m` movie record/replay lock (`rusty2600_core::movie`,
/// shipped `v1.7.0`, has no lock concept of its own yet) and a rollback
/// netplay lock (staged `v1.10.0`, unbuilt). Adding an always-`false` stub
/// field for either now would be dead weight pretending to be a feature —
/// the right time to add `movie_locked`/`netplay_locked` is the same change
/// that gives those subsystems a real lock to fold in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WritesLocked {
    /// `RetroAchievements` hardcore mode is active for the current session.
    pub ra_hardcore: bool,
}

impl WritesLocked {
    /// Whether ANY real lock source is currently active.
    #[must_use]
    pub const fn locked(self) -> bool {
        self.ra_hardcore
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
        let locked = WritesLocked { ra_hardcore: true };
        assert!(locked.locked());
    }
}
