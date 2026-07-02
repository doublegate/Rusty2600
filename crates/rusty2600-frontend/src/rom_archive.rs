//! Extracting a ROM image out of a `.zip` archive.
//!
//! Shared between the native `OpenRom` file-dialog path (`app`) and the wasm
//! demo's `<input type=file>` loader (`wasm`) — neither could previously
//! read a ROM out of a zip at all, only a bare `.a26`/`.bin`/`.rom` file,
//! even though zipped ROM redistribution is the norm.
//!
//! Reads only ever happen from an in-memory [`std::io::Cursor`] over bytes
//! already fully read into memory (the native file's bytes, or the wasm
//! `FileReader`'s `ArrayBuffer` copy) — this module never touches a real
//! filesystem path, so it needs no path-traversal guard (there is nowhere
//! for a malicious zip entry name to write to) and builds cleanly for
//! `wasm32-unknown-unknown` the same as any other in-memory computation.

use std::io::{Cursor, Read};

/// The largest committed 2600 ROM image size across the whole cart catalogue
/// (`BankAr`'s 64 KiB Supercharker image, `rusty2600-cart`'s largest fixed
/// bank array) is `0x1_0000` (64 KiB). 1 MiB is a generous ceiling — 16x
/// headroom — against a zip entry lying about (or exploiting) its size.
const MAX_ROM_SIZE: usize = 1024 * 1024;

/// Extensions treated as "this zip entry is a ROM image", checked
/// case-insensitively against the entry's own name (not the archive's).
const ROM_EXTENSIONS: [&str; 3] = ["a26", "bin", "rom"];

/// Everything that can go wrong extracting a ROM from a `.zip`.
#[derive(Debug)]
pub enum RomArchiveError {
    /// The zip central directory / entry couldn't be parsed at all
    /// (truncated, corrupt, or not actually a zip despite the `.zip` name).
    Malformed(String),
    /// The archive parsed fine but contains no entry whose extension looks
    /// like a ROM image.
    NoRomEntry,
    /// A ROM-extension entry's decompressed data exceeded the (private)
    /// `MAX_ROM_SIZE` ceiling before decompression was even allowed to
    /// finish — rejected outright rather than let a mis-sized/bomb entry
    /// allocate without bound.
    EntryTooLarge {
        /// The zip entry's own name.
        name: String,
    },
}

impl core::fmt::Display for RomArchiveError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Malformed(e) => write!(f, "not a valid zip archive: {e}"),
            Self::NoRomEntry => {
                write!(f, "zip archive contains no .a26/.bin/.rom entry")
            }
            Self::EntryTooLarge { name } => {
                write!(
                    f,
                    "zip entry {name:?} is larger than the {MAX_ROM_SIZE} byte ROM-size ceiling"
                )
            }
        }
    }
}

/// Returns the extension of `name` (the part after the last `.`), lowercased,
/// or an empty string if there is none.
fn extension_of(name: &str) -> String {
    name.rsplit('.')
        .next()
        .filter(|_| name.contains('.'))
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Whether `filename` (the file the user picked / dropped) looks like a zip
/// archive by extension.
#[must_use]
pub fn looks_like_zip(filename: &str) -> bool {
    extension_of(filename) == "zip"
}

/// Extracts the first `.a26`/`.bin`/`.rom` entry (by archive order) out of
/// `bytes`, treated as a zip archive.
///
/// Returns the entry's raw (decompressed) bytes and its in-archive name.
/// Never panics on malformed input — every failure mode is a typed
/// [`RomArchiveError`], since `bytes` is untrusted, user-supplied data.
///
/// # Errors
/// Returns [`RomArchiveError::Malformed`] if `bytes` doesn't parse as a zip
/// (or an entry can't be read), [`RomArchiveError::NoRomEntry`] if no entry
/// has a `.a26`/`.bin`/`.rom` extension, or
/// [`RomArchiveError::EntryTooLarge`] if the first such entry's decompressed
/// size exceeds the (private) `MAX_ROM_SIZE` ceiling.
pub fn extract_first_rom(bytes: &[u8]) -> Result<(Vec<u8>, String), RomArchiveError> {
    let cursor = Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| RomArchiveError::Malformed(e.to_string()))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| RomArchiveError::Malformed(e.to_string()))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        if !ROM_EXTENSIONS.contains(&extension_of(&name).as_str()) {
            continue;
        }

        // Bound the READ, not just the declared size — a zip's central
        // directory can claim any uncompressed size it likes, so trusting
        // `entry.size()` alone would not actually stop a decompression
        // bomb. `take(MAX_ROM_SIZE + 1)` caps how many decompressed bytes
        // can ever be produced, regardless of what the header claims.
        let mut limited = entry
            .by_ref()
            .take(u64::try_from(MAX_ROM_SIZE + 1).unwrap_or(u64::MAX));
        let mut out = Vec::new();
        limited
            .read_to_end(&mut out)
            .map_err(|e| RomArchiveError::Malformed(e.to_string()))?;
        if out.len() > MAX_ROM_SIZE {
            return Err(RomArchiveError::EntryTooLarge { name });
        }
        return Ok((out, name));
    }

    Err(RomArchiveError::NoRomEntry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let cursor = Cursor::new(&mut buf);
            let mut writer = zip::ZipWriter::new(cursor);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for (name, data) in entries {
                writer.start_file(*name, options).expect("start_file");
                writer.write_all(data).expect("write_all");
            }
            writer.finish().expect("finish");
        }
        buf
    }

    #[test]
    fn extracts_the_first_rom_entry() {
        let rom_bytes: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
        let zip_bytes = make_zip(&[
            ("readme.txt", b"not a rom"),
            ("game.a26", &rom_bytes),
            ("game2.a26", b"should not be picked"),
        ]);

        let (extracted, name) = extract_first_rom(&zip_bytes).expect("extraction should succeed");
        assert_eq!(name, "game.a26");
        assert_eq!(extracted, rom_bytes);
    }

    #[test]
    fn case_insensitive_extension_and_looks_like_zip() {
        assert!(looks_like_zip("Kaboom.ZIP"));
        assert!(looks_like_zip("kaboom.zip"));
        assert!(!looks_like_zip("kaboom.a26"));

        let zip_bytes = make_zip(&[("GAME.A26", b"data")]);
        let (extracted, name) = extract_first_rom(&zip_bytes).expect("extraction should succeed");
        assert_eq!(name, "GAME.A26");
        assert_eq!(extracted, b"data");
    }

    #[test]
    fn no_rom_entry_is_a_typed_error() {
        let zip_bytes = make_zip(&[("readme.txt", b"not a rom"), ("notes.md", b"also not")]);
        let err = extract_first_rom(&zip_bytes).expect_err("should fail — no ROM entry");
        assert!(matches!(err, RomArchiveError::NoRomEntry));
    }

    #[test]
    fn malformed_zip_is_a_typed_error_not_a_panic() {
        let err = extract_first_rom(b"this is not a zip file at all")
            .expect_err("garbage input must not panic");
        assert!(matches!(err, RomArchiveError::Malformed(_)));
    }

    #[test]
    fn oversized_entry_is_rejected() {
        // A ROM-extension entry whose decompressed size exceeds the ceiling
        // must be rejected, not silently truncated or allowed through.
        let oversized = vec![0xAAu8; MAX_ROM_SIZE + 1];
        let zip_bytes = make_zip(&[("huge.bin", &oversized)]);
        let err = extract_first_rom(&zip_bytes).expect_err("oversized entry must be rejected");
        assert!(matches!(err, RomArchiveError::EntryTooLarge { .. }));
    }
}
