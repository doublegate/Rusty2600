//! `?settings=` share-links (`[v2.9.0]` "Full Circle").
//!
//! Round-trips the current [`Config`] through a compact, URL-safe base64 blob so a user can share
//! their emulator settings (region, video/audio, key bindings) via a single URL, matching
//! RustyNES's own `wasm_share.rs` convention (`?settings=<blob>`).
//!
//! ## Scope: settings only, no ROM reference
//!
//! There is no canonical URL for a loaded ROM on this build — the wasm-winit ROM picker
//! (`app::App::trigger_wasm_rom_picker`) reads a user-local file via a hidden `<input
//! type=file>`, the same as the native `rfd` dialog; neither has a URL a share link could carry.
//! A Rusty2600 share link therefore covers the [`Config`] only ("here is how I have the emulator
//! set up"), never "here is the exact ROM to load" — the recipient still picks their own ROM file.
//!
//! ## Why the WHOLE `Config`, unlike RustyNES's curated subset
//!
//! RustyNES's `ShareSettings` deliberately shares only a curated subset of its `Config`, because
//! that project's config also carries machine-local state (recent-ROM paths, a RetroAchievements
//! login token, HD-pack filesystem paths) that must never leak into a shared URL. Rusty2600's
//! [`Config`] has no such fields — just [`crate::palette::Region`], [`crate::config::VideoConfig`],
//! [`crate::config::AudioConfig`], and the two players' [`crate::input::KeyBindings`] — so the
//! whole thing is safe to round-trip through the exact same `Config::to_toml_string`/
//! `Config::from_toml_str` helpers `[v2.8.0]`'s `localStorage` persistence already established
//! as pure and target-agnostic, rather than hand-duplicating a parallel DTO + validation layer.
//! (Plain code spans, not intra-doc links: both helpers are `pub(crate)`, invisible to the
//! public `cargo doc --workspace --no-deps` build this crate's docs are gated against.)
//!
//! ## Pure vs. wasm32-only
//!
//! `encode`/`decode`/`query_param` touch no browser API and are exercised by the ordinary
//! native `cargo test --workspace` run (matching this project's established test-as-spec
//! discipline for target-agnostic persistence logic). Only `from_location` (reads
//! `window.location().search()`) and `share_url` (reads `window.location().origin()`/
//! `pathname()`) are wasm32-gated, since they need a real `window` — plain code spans here too,
//! since both are absent entirely from a native `cargo doc` build (matching `lib.rs`'s own
//! convention for `cfg`-exclusive items).

use crate::config::Config;

/// Maximum accepted length of the raw `?settings=` value (base64url chars). A legitimate blob
/// encoding the full `Config` (region + video + audio + both players' key bindings) is a few
/// hundred bytes; this cap (8 KiB) stops a pathological URL from forcing a large decode
/// allocation — the same guard RustyNES's own `ShareSettings::decode` applies.
const MAX_SHARE_LEN: usize = 8 * 1024;

/// The URL-safe base64 alphabet (RFC 4648 §5): `+`/`/` replaced with `-`/`_`, no `=` padding
/// (reconstructable from the input length on decode) — the standard "clean inside a query
/// string" variant.
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Hand-rolled rather than pulling in the `base64` crate: this project's dependency ethos (see
/// `Cargo.toml`'s `zip`/`hd-pack` feature comments) is to keep the core dependency-light, and a
/// ~30-line codec is simple enough to own directly — it also keeps [`encode`]/[`decode`] pure
/// Rust with no browser `atob`/`btoa` call, so they compile and are unit-tested identically on
/// every target (native included), unlike a `web_sys`-backed implementation would be.
fn base64url_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied();
        let b2 = chunk.get(2).copied();
        let n = (b0 << 16) | (u32::from(b1.unwrap_or(0)) << 8) | u32::from(b2.unwrap_or(0));
        out.push(ALPHABET[((n >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3F) as usize] as char);
        if b1.is_some() {
            out.push(ALPHABET[((n >> 6) & 0x3F) as usize] as char);
        }
        if b2.is_some() {
            out.push(ALPHABET[(n & 0x3F) as usize] as char);
        }
    }
    out
}

/// Inverse of [`base64url_encode`]. `None` on any character outside the URL-safe alphabet, or a
/// final group whose length (mod 4) is `1` — never a valid encoding of any byte count.
// Each `n >> shift` is masked to its low 8 bits by construction (`n` packs at most 4x6 = 24
// bits, and every extracted byte is one of its three constituent 8-bit groups) — the truncation
// clippy warns about here can't actually happen.
#[allow(clippy::cast_possible_truncation)]
fn base64url_decode(s: &str) -> Option<Vec<u8>> {
    const fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'-' => Some(62),
            b'_' => Some(63),
            _ => None,
        }
    }

    if !s.is_ascii() {
        return None;
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity((bytes.len() * 3) / 4 + 1);
    for group in bytes.chunks(4) {
        let mut vals = [0u8; 4];
        for (slot, &c) in vals.iter_mut().zip(group) {
            *slot = val(c)?;
        }
        let n = (u32::from(vals[0]) << 18)
            | (u32::from(vals[1]) << 12)
            | (u32::from(vals[2]) << 6)
            | u32::from(vals[3]);
        match group.len() {
            4 => {
                out.push((n >> 16) as u8);
                out.push((n >> 8) as u8);
                out.push(n as u8);
            }
            3 => {
                out.push((n >> 16) as u8);
                out.push((n >> 8) as u8);
            }
            2 => {
                out.push((n >> 16) as u8);
            }
            // A trailing group of exactly 1 base64 character can't encode a whole byte.
            _ => return None,
        }
    }
    Some(out)
}

/// Encode `config` to a compact URL-safe base64 blob (`Config::to_toml_string` → base64url).
///
/// A serialize failure (which a plain struct of primitives/enums/strings realistically never
/// hits) degrades to encoding an empty body rather than panicking — matching this module's
/// "never block the user over a share-link cosmetic" posture.
#[must_use]
pub fn encode(config: &Config) -> String {
    let toml = config.to_toml_string().unwrap_or_default();
    base64url_encode(toml.as_bytes())
}

/// Decode a `?settings=` blob into a [`Config`].
///
/// Returns `None` only when the blob itself is untrustworthy as a share-link value: empty,
/// oversized (see `MAX_SHARE_LEN`), not valid base64url, or not valid UTF-8. A blob that
/// decodes cleanly but contains malformed or foreign TOML still returns `Some` — falling back to
/// `Config::default()` for the unparsable fields, exactly like `Config::from_toml_str`'s own
/// never-block posture for a corrupt persisted value (both plain code spans: `MAX_SHARE_LEN` is
/// private, `Config::from_toml_str` is `pub(crate)` — neither is linkable from this crate's
/// public `cargo doc` build). This means a share link minted by an older or newer build (fields
/// added/removed) always decodes to a usable config.
#[must_use]
pub fn decode(blob: &str) -> Option<Config> {
    if blob.is_empty() || blob.len() > MAX_SHARE_LEN {
        return None;
    }
    let bytes = base64url_decode(blob)?;
    let text = core::str::from_utf8(&bytes).ok()?;
    Some(Config::from_toml_str(text))
}

/// Minimal `key=value` extractor for a `?a=b&c=d` query string (a leading `?` is tolerated).
///
/// Avoids pulling in a URL-parsing dependency for one parameter. The value is returned
/// undecoded — a share blob is already URL-safe base64, so it needs no percent-decoding, and
/// this project's key/value pairs never carry a literal `&` or `=` that would need it either.
///
/// Pure (no `web_sys`), so it's covered by the same native unit tests as [`encode`]/[`decode`];
/// only `from_location` (wasm32-only; plain code span since it's absent from a native `cargo
/// doc` build) actually reads a real `window.location().search()`.
#[must_use]
pub fn query_param(search: &str, key: &str) -> Option<String> {
    let q = search.strip_prefix('?').unwrap_or(search);
    for pair in q.split('&') {
        if let Some((k, v)) = pair.split_once('=')
            && k == key
        {
            return Some(v.to_owned());
        }
    }
    None
}

/// Read the `?settings=` value from the current page URL and decode it, if present + valid.
///
/// Called once at boot (`wasm.rs::run_winit`). Returns `None` when there is no `window`, no
/// `settings` query parameter, or the value fails [`decode`]'s guards — every case falls through
/// to the caller's own `Config::load()` result, so a missing/malformed share link never blocks
/// launch.
#[cfg(target_arch = "wasm32")]
#[must_use]
pub fn from_location() -> Option<Config> {
    let search = web_sys::window()?.location().search().ok()?;
    let raw = query_param(&search, "settings")?;
    decode(&raw)
}

/// Build a full `?settings=` share URL for `config` (this page's origin + path + the encoded
/// blob).
///
/// For the Settings panel's "Generate share link" affordance
/// ([`crate::shell::MenuAction::GenerateShareLink`]). Returns an empty string if the page
/// location is unavailable (no `window`) — the caller (`app.rs`'s dispatch) treats that the same
/// as "nothing to show" rather than a hard error.
#[cfg(target_arch = "wasm32")]
#[must_use]
pub fn share_url(config: &Config) -> String {
    let blob = encode(config);
    web_sys::window()
        .map(|w| w.location())
        .and_then(|loc| {
            let origin = loc.origin().ok()?;
            let pathname = loc.pathname().ok()?;
            Some(format!("{origin}{pathname}?settings={blob}"))
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::Region;

    #[test]
    fn base64url_round_trips_arbitrary_byte_lengths() {
        // Exercise all three padding remainders (len % 3 == 0, 1, 2) in one sweep.
        for len in 0_u32..16 {
            let bytes: Vec<u8> = (0..len)
                .map(|i| u8::try_from((i * 37 + 5) % 256).expect("mod 256 always fits u8"))
                .collect();
            let encoded = base64url_encode(&bytes);
            assert!(
                !encoded.contains(['+', '/', '=']),
                "must be URL-safe: {encoded}"
            );
            let decoded = base64url_decode(&encoded).expect("valid encoding must decode");
            assert_eq!(decoded, bytes, "round trip failed for length {len}");
        }
    }

    #[test]
    fn base64url_decode_rejects_invalid_length_and_chars() {
        assert!(base64url_decode("A").is_none(), "len % 4 == 1 is invalid");
        assert!(base64url_decode("A!!!").is_none(), "'!' not in alphabet");
        assert!(base64url_decode("has spaces").is_none());
    }

    #[test]
    fn encode_decode_round_trips_a_full_config() {
        let mut cfg = Config {
            region: Region::Pal,
            ..Config::default()
        };
        cfg.audio.volume = 0.33;
        cfg.video.integer_scale = true;
        cfg.p1
            .binds
            .push(("KeyP".into(), crate::input::InputAction::SwitchSelect));

        let blob = encode(&cfg);
        // Must actually be URL-clean, not just base64.
        assert!(!blob.contains(['+', '/', '=', '?', '&']));

        let back = decode(&blob).expect("a freshly-encoded blob must decode");
        assert_eq!(back.region, cfg.region);
        assert!((back.audio.volume - 0.33).abs() < f32::EPSILON);
        assert!(back.video.integer_scale);
        assert_eq!(back.p1.binds.len(), cfg.p1.binds.len());
    }

    #[test]
    fn decode_rejects_empty_and_oversized_blobs() {
        assert!(decode("").is_none());
        let oversized = "A".repeat(MAX_SHARE_LEN + 4);
        assert!(decode(&oversized).is_none());
    }

    #[test]
    fn decode_rejects_non_base64url_input() {
        assert!(decode("not valid base64url!!").is_none());
    }

    #[test]
    fn decode_of_valid_base64_but_garbage_toml_falls_back_to_default() {
        // A validly-encoded blob whose TOML body is nonsense must still yield a usable Config
        // (Config::from_toml_str's own never-block posture), not `None` — only the base64/UTF-8
        // shape itself is a hard rejection.
        let blob = base64url_encode(b"this is not valid toml {{{");
        let back = decode(&blob).expect("valid base64/utf-8 must still decode to a Config");
        assert_eq!(back.region, Config::default().region);
    }

    #[test]
    fn query_param_extracts_the_settings_value() {
        assert_eq!(
            query_param("?settings=abc123&foo=bar", "settings"),
            Some("abc123".to_string())
        );
        assert_eq!(query_param("foo=bar", "settings"), None);
        assert_eq!(query_param("", "settings"), None);
        assert_eq!(
            query_param("?a=1&settings=xyz", "settings"),
            Some("xyz".to_string())
        );
    }
}
