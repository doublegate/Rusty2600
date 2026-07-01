//! Save-states — a versioned binary snapshot of an entire [`System`].
//!
//! [`System`] (and everything it owns — `Cpu`, `Bus`, `Tia`, `Riot`,
//! `Cartridge`) already derives `serde::Serialize`/`Deserialize`, so this
//! module is a thin wrapper: a small header (magic, format version, a
//! caller-supplied ROM identity tag) plus the `System` itself, encoded with
//! `postcard` (a compact, `no_std`+`alloc`-friendly binary serde format — no
//! hand-rolled tagged-section encoder needed, unlike a codebase that has to
//! avoid `serde` in its core).
//!
//! `rusty2600-core` doesn't know how a ROM's identity should be computed
//! (that's a frontend/tooling concern — a full SHA-256, a fast FNV-1a, or
//! anything else); callers supply an opaque `rom_tag: u64` and
//! [`SaveState::restore`] simply checks it matches what was captured, so a
//! save file can't silently be loaded against the wrong cartridge.
//!
//! See `docs/adr/0007-save-state-versioning.md` for the version-compatibility
//! policy this header format implements.

use alloc::vec::Vec;

use crate::scheduler::System;

/// The save-state file magic (`"R26S"`).
const MAGIC: [u8; 4] = *b"R26S";

/// The current save-state format version. Bump this only per the migration
/// policy in ADR 0007 (additive changes within a MINOR release may keep this
/// the same, relying on `#[serde(default)]` on new fields; anything else
/// bumps it and must be handled explicitly by [`SaveState::restore`]).
const FORMAT_VERSION: u16 = 1;

/// The oldest format version this build can still read.
const MIN_SUPPORTED_FORMAT_VERSION: u16 = 1;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SaveStateHeader {
    magic: [u8; 4],
    format_version: u16,
    rom_tag: u64,
}

/// A captured snapshot of a [`System`], ready to be encoded to bytes or
/// restored back into a running system.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SaveState {
    header: SaveStateHeader,
    system: System,
}

/// Everything that can go wrong loading a save-state.
#[derive(Debug)]
pub enum SaveStateError {
    /// The byte stream didn't decode as a `SaveState` at all (truncated,
    /// corrupt, or not a save-state file).
    Malformed,
    /// The decoded header's magic didn't match — this isn't a Rusty2600
    /// save-state file.
    BadMagic,
    /// The save-state's `rom_tag` doesn't match the ROM currently loaded.
    RomMismatch,
    /// The save-state's format version is older than this build can read.
    UnsupportedFormat {
        /// The format version stored in the file.
        file_version: u16,
        /// The oldest format version this build supports.
        min_supported: u16,
    },
}

impl core::fmt::Display for SaveStateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Malformed => write!(f, "save-state data is malformed or truncated"),
            Self::BadMagic => write!(f, "not a Rusty2600 save-state file"),
            Self::RomMismatch => write!(f, "save-state was captured for a different ROM"),
            Self::UnsupportedFormat {
                file_version,
                min_supported,
            } => write!(
                f,
                "save-state format v{file_version} is older than the minimum \
                 supported format v{min_supported}"
            ),
        }
    }
}

impl SaveState {
    /// Captures `system` as a save-state tagged with `rom_tag` (an opaque
    /// caller-supplied ROM identity, e.g. a hash of the loaded ROM image).
    #[must_use]
    pub fn capture(system: &System, rom_tag: u64) -> Self {
        Self {
            header: SaveStateHeader {
                magic: MAGIC,
                format_version: FORMAT_VERSION,
                rom_tag,
            },
            system: system.clone(),
        }
    }

    /// Encodes this save-state to its binary wire format.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        // A `SaveState` built via `capture` always serializes successfully —
        // every field is a plain derive over data this crate itself owns, no
        // trait objects or external I/O to fail on.
        postcard::to_allocvec(self).unwrap_or_default()
    }

    /// Decodes a save-state from its binary wire format, without checking it
    /// against any particular ROM yet (see [`Self::restore`] for that).
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveStateError> {
        let state: Self = postcard::from_bytes(bytes).map_err(|_| SaveStateError::Malformed)?;
        if state.header.magic != MAGIC {
            return Err(SaveStateError::BadMagic);
        }
        if state.header.format_version < MIN_SUPPORTED_FORMAT_VERSION {
            return Err(SaveStateError::UnsupportedFormat {
                file_version: state.header.format_version,
                min_supported: MIN_SUPPORTED_FORMAT_VERSION,
            });
        }
        Ok(state)
    }

    /// Decodes and validates a save-state against `rom_tag`, returning the
    /// [`System`] it captured. Fails with [`SaveStateError::RomMismatch`] if
    /// the save-state was captured for a different ROM.
    pub fn restore(bytes: &[u8], rom_tag: u64) -> Result<System, SaveStateError> {
        let state = Self::decode(bytes)?;
        if state.header.rom_tag != rom_tag {
            return Err(SaveStateError::RomMismatch);
        }
        Ok(state.system)
    }

    /// The `System` this save-state captured, without any ROM-tag check —
    /// for callers (like the rewind ring) that already know the ROM can't
    /// have changed since capture.
    #[must_use]
    pub fn into_system(self) -> System {
        self.system
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_is_byte_identical() {
        let mut system = System::new(42);
        system.step_instruction();
        system.step_instruction();

        let saved = SaveState::capture(&system, 0xDEAD_BEEF);
        let bytes = saved.encode();
        let restored = SaveState::restore(&bytes, 0xDEAD_BEEF).expect("round-trip should succeed");

        assert_eq!(system.color_clocks(), restored.color_clocks());
        assert_eq!(
            (system.cpu.a, system.cpu.x, system.cpu.y, system.cpu.pc),
            (
                restored.cpu.a,
                restored.cpu.x,
                restored.cpu.y,
                restored.cpu.pc
            )
        );
        assert_eq!(system.bus.riot.ram, restored.bus.riot.ram);
    }

    #[test]
    fn rom_tag_mismatch_is_rejected() {
        let system = System::new(1);
        let saved = SaveState::capture(&system, 1);
        let bytes = saved.encode();
        let err = SaveState::restore(&bytes, 2).expect_err("mismatched rom_tag must fail");
        assert!(matches!(err, SaveStateError::RomMismatch));
    }

    #[test]
    fn bad_magic_is_rejected() {
        let system = System::new(1);
        let saved = SaveState::capture(&system, 1);
        let mut bytes = saved.encode();
        // Corrupt the header's magic bytes (first 4 bytes of the postcard
        // stream, since `SaveStateHeader` is the first field serialized).
        bytes[0] ^= 0xFF;
        let err = SaveState::decode(&bytes);
        assert!(err.is_err());
    }

    #[test]
    fn truncated_bytes_are_rejected() {
        let err = SaveState::decode(&[0u8; 2]);
        assert!(matches!(err, Err(SaveStateError::Malformed)));
    }
}
