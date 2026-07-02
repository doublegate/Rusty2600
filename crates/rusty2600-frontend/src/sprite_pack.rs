//! The HD-pack analog for the 2600.
//!
//! A right-sized sprite-replacement data model + loader for player/missile/
//! ball graphics, behind the `hd-pack`
//! feature.
//!
//! Deliberately much smaller than a Mesen2-style HD-pack system: the TIA has
//! no tile/pattern-table concept at all — a player's entire visual data
//! *is* its 8-bit `GRPx` byte, and a missile/ball is just an on/off dot — so
//! there's no CHR bank to hash-match against, and no separate "background
//! image" concept distinct from playfield rendering. This module covers
//! exactly what the 2600 actually has: a replacement bitmap keyed by
//! `(GRPx value, NUSIZx copy mode)`.
//!
//! **Scope of this release**: the data model and manifest loader below are real and tested, and
//! (as of v2.7.0 "True Colors") so is the live splice. `rusty2600-tia`'s `hd-pack`-gated
//! `object_mask` (see `docs/tia.md`) now tags every pixel with which object rendered it plus,
//! for player pixels, the exact `(GRPx, NUSIZx)` live at that moment; `crate::emu_thread::
//! EmuCore::step_frame` consults `EmuCore::sprite_pack` against that mask and substitutes a
//! matching replacement bitmap's pixels for a player object's on-screen footprint, nearest-
//! neighbor scaled. Still deliberately player-only, matching this module's data model: missile/
//! ball/playfield/background pixels are tagged by the mask but have no replacement key here.

use std::collections::HashMap;
use std::path::Path;

/// The lookup key for one player/missile/ball sprite replacement.
///
/// Its `GRPx` bitmap byte plus the `NUSIZx` copy-mode bits that affect how
/// many times (and how far apart) it's drawn — matching
/// `crate::debugger::pmb_panel`'s own decode of the same register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpriteKey {
    /// The `GRPx` byte (the object's entire visual data).
    pub grp: u8,
    /// The `NUSIZx` byte, masked to the 3 copy-mode bits (`& 0x07`).
    pub nusiz_copies: u8,
}

impl SpriteKey {
    /// Builds a key from a raw `GRPx`/`NUSIZx` pair, masking `nusiz` down
    /// to its copy-mode bits so unrelated size bits don't fragment the
    /// lookup table.
    #[must_use]
    pub const fn new(grp: u8, nusiz: u8) -> Self {
        Self {
            grp,
            nusiz_copies: nusiz & 0x07,
        }
    }
}

/// One replacement bitmap: raw RGBA8 pixels, row-major, plus its dimensions.
#[derive(Debug, Clone)]
pub struct SpriteBitmap {
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// `width * height * 4` RGBA8 bytes.
    pub rgba: Vec<u8>,
}

/// A loaded sprite-replacement pack: a flat `SpriteKey -> SpriteBitmap` map.
#[derive(Debug, Clone, Default)]
pub struct SpritePack {
    sprites: HashMap<SpriteKey, SpriteBitmap>,
}

/// Errors loading a [`SpritePack`] manifest.
#[derive(Debug, thiserror::Error)]
pub enum SpritePackError {
    /// The manifest file couldn't be read.
    #[error("failed to read manifest: {0}")]
    ManifestRead(#[from] std::io::Error),
    /// The manifest's TOML was malformed.
    #[error("failed to parse manifest: {0}")]
    ManifestParse(#[from] toml::de::Error),
    /// One entry's `file` couldn't be read as raw RGBA8 bytes.
    #[error("sprite '{file}': failed to read: {source}")]
    SpriteRead {
        /// The manifest-relative file path that failed.
        file: String,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// One entry's `file` size didn't match `width * height * 4`.
    #[error("sprite '{file}': expected {expected} bytes ({width}x{height} RGBA8), found {actual}")]
    SizeMismatch {
        /// The manifest-relative file path.
        file: String,
        /// The manifest's declared width.
        width: u32,
        /// The manifest's declared height.
        height: u32,
        /// The byte count the declared dimensions imply.
        expected: usize,
        /// The file's actual byte count.
        actual: usize,
    },
}

#[derive(serde::Deserialize)]
struct ManifestEntry {
    grp: u8,
    nusiz: u8,
    width: u32,
    height: u32,
    file: String,
}

#[derive(serde::Deserialize)]
struct Manifest {
    #[serde(rename = "sprite", default)]
    sprites: Vec<ManifestEntry>,
}

impl SpritePack {
    /// Loads a pack from a `manifest.toml` in `dir`, e.g.:
    ///
    /// ```toml
    /// [[sprite]]
    /// grp = 0xFF
    /// nusiz = 0
    /// width = 8
    /// height = 8
    /// file = "player0_full.rgba"
    /// ```
    ///
    /// Each `file` is raw RGBA8 bytes (row-major, `width * height * 4`
    /// long) relative to `dir` — no image-format decoder dependency needed
    /// for this first cut.
    ///
    /// # Errors
    ///
    /// Returns [`SpritePackError`] if the manifest can't be read/parsed, or
    /// if any entry's sprite file can't be read or doesn't match its
    /// declared dimensions.
    pub fn load(dir: &Path) -> Result<Self, SpritePackError> {
        let manifest_text = std::fs::read_to_string(dir.join("manifest.toml"))?;
        let manifest: Manifest = toml::from_str(&manifest_text)?;
        let mut sprites = HashMap::with_capacity(manifest.sprites.len());
        for entry in manifest.sprites {
            let path = dir.join(&entry.file);
            let rgba = std::fs::read(&path).map_err(|source| SpritePackError::SpriteRead {
                file: entry.file.clone(),
                source,
            })?;
            let expected = entry.width as usize * entry.height as usize * 4;
            if rgba.len() != expected {
                return Err(SpritePackError::SizeMismatch {
                    file: entry.file,
                    width: entry.width,
                    height: entry.height,
                    expected,
                    actual: rgba.len(),
                });
            }
            sprites.insert(
                SpriteKey::new(entry.grp, entry.nusiz),
                SpriteBitmap {
                    width: entry.width,
                    height: entry.height,
                    rgba,
                },
            );
        }
        Ok(Self { sprites })
    }

    /// Looks up a replacement bitmap for the given `GRPx`/`NUSIZx` pair.
    #[must_use]
    pub fn lookup(&self, grp: u8, nusiz: u8) -> Option<&SpriteBitmap> {
        self.sprites.get(&SpriteKey::new(grp, nusiz))
    }

    /// The number of loaded replacement entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sprites.len()
    }

    /// Whether the pack has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sprites.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_masks_off_the_nusiz_size_bits() {
        // Bits above the low 3 (missile-width) must not fragment the key.
        assert_eq!(
            SpriteKey::new(0xFF, 0b0000_0000),
            SpriteKey::new(0xFF, 0b0011_0000)
        );
    }

    #[test]
    fn load_reads_manifest_and_matching_sprite_bytes() {
        let dir =
            std::env::temp_dir().join(format!("rusty2600-sprite-pack-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(
            dir.join("manifest.toml"),
            r#"
[[sprite]]
grp = 255
nusiz = 0
width = 2
height = 1
file = "p0.rgba"
"#,
        )
        .expect("write manifest");
        std::fs::write(dir.join("p0.rgba"), [0u8; 8]).expect("write sprite bytes"); // 2x1 RGBA8

        let pack = SpritePack::load(&dir).expect("load pack");
        assert_eq!(pack.len(), 1);
        let sprite = pack.lookup(0xFF, 0).expect("lookup hit");
        assert_eq!((sprite.width, sprite.height), (2, 1));
        assert_eq!(sprite.rgba.len(), 8);

        assert!(pack.lookup(0x00, 0).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_rejects_size_mismatch() {
        let dir = std::env::temp_dir().join(format!(
            "rusty2600-sprite-pack-test-mismatch-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(
            dir.join("manifest.toml"),
            r#"
[[sprite]]
grp = 1
nusiz = 0
width = 4
height = 4
file = "bad.rgba"
"#,
        )
        .expect("write manifest");
        std::fs::write(dir.join("bad.rgba"), [0u8; 4]).expect("write short sprite bytes");

        let err = SpritePack::load(&dir).expect_err("size mismatch must error");
        assert!(matches!(err, SpritePackError::SizeMismatch { .. }));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
