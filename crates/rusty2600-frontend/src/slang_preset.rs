//! Constrained RetroArch `.slangp` / `.cgp` shader-preset importer (`v2.10.0`).
//!
//! Matches RustyNES's own ADR 0013 design philosophy exactly (see that
//! project's `crates/rustynes-frontend/src/slang_preset.rs` — the precedent
//! this module's scope is modeled on, independently re-implemented against
//! Rusty2600's own, much smaller built-in pass set): a full GLSL/Slang ->
//! WGSL translation layer is a large, fragile surface with poor payoff for a
//! built-in-shader emulator, so this stays a deliberately *constrained*
//! importer:
//!
//! 1. **Parse** the preset (the RetroArch `shaderN = path` / flat `key =
//!    value` parameter format shared by both `.slangp` and `.cgp`).
//! 2. **Map** each referenced pass onto a built-in
//!    [`rusty2600_gfx_shaders::PassKind`] by recognizing well-known shader
//!    filename *stems* (a `crt-*`/`*scanline*`/`*aperture*`/`*geom*` pass ->
//!    [`rusty2600_gfx_shaders::PassKind::CrtScanline`]; an `ntsc`/`composite`
//!    pass -> [`rusty2600_gfx_shaders::PassKind::CompositeArtifact`] —
//!    deliberately NOT [`rusty2600_gfx_shaders::PassKind::NtscComposite`],
//!    which is position-constrained and could produce an invalid stack
//!    depending on where the preset places it, exactly the reasoning
//!    RustyNES's own importer applies to its equivalent Bisqwit-vs-lmp88959
//!    choice; an `hqx`/`hq2x`/`hq4x` pass -> [`rusty2600_gfx_shaders::PassKind::HqNx`];
//!    an `xbr`/`xbrz` pass -> [`rusty2600_gfx_shaders::PassKind::Xbrz`]).
//! 3. **Honestly reject** anything it can't translate: an unrecognized
//!    filename becomes an [`crate::slang_preset::ImportedPass::Unsupported`]
//!    entry — reported to the caller, never silently dropped and never
//!    producing a broken stack.
//!
//! This is **not** a GLSL/Slang -> WGSL transpiler and never will be — it
//! does not read or compile the referenced shader files; it recognizes the
//! *intent* of common community presets by filename and re-expresses them
//! with Rusty2600's curated built-in WGSL passes. Unlike RustyNES's own
//! importer, there is no per-pass parameter-override plumbing here:
//! Rusty2600's [`crate::config::VideoConfig::shader_passes`] is a plain
//! `Vec<PassKind>` with no tunable knobs on any built-in pass (see
//! `shader_pass.rs`'s module doc) — that machinery is only worth building
//! once a pass actually needs it, per this project's own "don't build
//! unused generality" convention.
//!
//! Pure data transform (no GPU, no I/O beyond the caller handing over the
//! preset's text), so it is fully unit-testable and safe on every target
//! (native and wasm32 alike).

use rusty2600_gfx_shaders::PassKind;

/// One pass resolved from a preset entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportedPass {
    /// Successfully mapped onto a built-in pass.
    Mapped(PassKind),
    /// Recognized as a pass entry, but no built-in equivalent — reported,
    /// not dropped. Carries the original `shaderN` path for the UI message.
    Unsupported {
        /// The `shaderN` path from the preset (verbatim).
        path: String,
        /// A short human reason (e.g. "no built-in equivalent").
        reason: String,
    },
}

/// The result of importing a preset: the translatable passes (ready to
/// assign directly to [`crate::config::VideoConfig::shader_passes`]) plus
/// the honest list of passes that could not be translated.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportResult {
    /// The mapped passes, in the preset's own order.
    pub passes: Vec<PassKind>,
    /// Passes that were recognized as entries but had no built-in
    /// equivalent.
    pub unsupported: Vec<ImportedPass>,
}

impl ImportResult {
    /// `true` when at least one preset pass mapped to a built-in.
    #[must_use]
    pub const fn any_mapped(&self) -> bool {
        !self.passes.is_empty()
    }

    /// Number of preset passes that could not be translated.
    #[must_use]
    pub const fn unsupported_count(&self) -> usize {
        self.unsupported.len()
    }
}

/// A parsed key/value line from a preset (INI-ish, `key = value`, `#`/`//`
/// comments, optional surrounding quotes on the value).
///
/// Handles a trailing inline comment after the value (`shader0 = "path" #
/// note` / `shader0 = path // note`) — a quoted value keeps everything
/// inside its own quotes verbatim (so a path itself can't be truncated by a
/// `#`/`//` that happens to appear inside it) and only strips a comment
/// starting AFTER the closing quote; an unquoted value simply stops at the
/// first `#` or `//`.
fn parse_kv(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
        return None;
    }
    let (k, v) = line.split_once('=')?;
    let v = v.trim();
    let v = v.strip_prefix('"').map_or_else(
        || {
            let hash = v.find('#').unwrap_or(v.len());
            let slashes = v.find("//").unwrap_or(v.len());
            &v[..hash.min(slashes)]
        },
        |rest| rest.find('"').map_or(v, |end| &v[..=end + 1]),
    );
    let v = v.trim().trim_matches('"').trim();
    Some((k.trim().to_ascii_lowercase(), v.to_string()))
}

/// The lowercase filename stem of a shader path (`shaders/crt/crt-geom.slang`
/// -> `crt-geom`).
fn shader_stem(path: &str) -> String {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let stem = file.split('.').next().unwrap_or(file);
    stem.to_ascii_lowercase()
}

/// The outcome of classifying a preset pass by its filename stem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StemMap {
    /// Maps onto the named built-in pass.
    Builtin(PassKind),
    /// A pure pass-through stage (`stock` / `passthrough` / `pixellate`): it
    /// contributes nothing visual, so it is silently **skipped** — not
    /// reported as unsupported (there is nothing missing to report).
    Skip,
    /// No built-in equivalent and not a pass-through: recorded as
    /// [`ImportedPass::Unsupported`] so the limit is visible.
    Unsupported,
}

/// Classify a shader filename stem, recognizing common community-preset
/// naming conventions. Order matters: more specific tokens are checked
/// first (`xbrz`/`xbr` before the generic `crt`-family check, etc.).
fn map_stem_to_builtin(stem: &str) -> StemMap {
    if stem.contains("xbrz") || stem.contains("xbr") {
        StemMap::Builtin(PassKind::Xbrz)
    } else if stem.contains("hqx") || stem.contains("hq2x") || stem.contains("hq4x") {
        StemMap::Builtin(PassKind::HqNx)
    } else if stem.contains("ntsc") || stem.contains("composite") {
        // Deliberately the position-flexible RGBA approximation, NOT
        // `NtscComposite` (which is position-constrained — see this
        // module's own doc comment for why).
        StemMap::Builtin(PassKind::CompositeArtifact)
    } else if stem.contains("crt")
        || stem.contains("scanline")
        || stem.contains("aperture")
        || stem.contains("geom")
    {
        StemMap::Builtin(PassKind::CrtScanline)
    } else if stem.contains("stock") || stem.contains("passthrough") || stem.contains("pixellate") {
        StemMap::Skip
    } else {
        StemMap::Unsupported
    }
}

/// Import a RetroArch `.slangp` / `.cgp` preset from its text contents.
///
/// The format is shared between the two extensions for the keys read here
/// (`shaders = N`, `shaderN = path`); any other `key = value` line is
/// ignored (Rusty2600's built-in passes have no tunable parameters to carry
/// over — see the module doc).
///
/// # Errors
///
/// Returns `Err` with a human message when the preset declares no
/// `shaderN` entries at all (i.e. it is not a recognizable RetroArch
/// preset).
pub fn import_preset(text: &str) -> Result<ImportResult, String> {
    // A `BTreeMap` rather than a `Vec` + sort: keeps entries ordered by index
    // AND deduplicates a repeated `shaderN` key to its LAST declared value
    // (standard RetroArch/INI-style override semantics — a later line wins)
    // instead of pushing both and producing a duplicate/redundant pass.
    let mut shader_paths: std::collections::BTreeMap<usize, String> =
        std::collections::BTreeMap::new();

    for line in text.lines() {
        let Some((k, v)) = parse_kv(line) else {
            continue;
        };
        if let Some(rest) = k.strip_prefix("shader")
            && let Ok(idx) = rest.parse::<usize>()
        {
            shader_paths.insert(idx, v);
        }
    }

    if shader_paths.is_empty() {
        return Err(
            "no `shaderN = …` entries found — not a recognizable RetroArch preset".to_string(),
        );
    }

    let mut result = ImportResult::default();
    for (_, path) in shader_paths {
        let stem = shader_stem(&path);
        match map_stem_to_builtin(&stem) {
            StemMap::Builtin(kind) => result.passes.push(kind),
            // Pure pass-through: contributes nothing and is not a missing
            // capability, so drop it silently (not counted as unsupported).
            StemMap::Skip => {}
            StemMap::Unsupported => {
                result.unsupported.push(ImportedPass::Unsupported {
                    path,
                    reason: "no built-in equivalent (source translation is out of scope)"
                        .to_string(),
                });
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_crt_preset() {
        let text = "\
            shaders = 1\n\
            shader0 = shaders/crt/crt-geom.slang\n";
        let r = import_preset(text).unwrap();
        assert_eq!(r.passes, vec![PassKind::CrtScanline]);
        assert_eq!(r.unsupported_count(), 0);
        assert!(r.any_mapped());
    }

    #[test]
    fn maps_multipass_ntsc_then_crt() {
        let text = "\
            shaders = 2\n\
            shader0 = shaders/ntsc/ntsc-composite.slang\n\
            shader1 = shaders/crt/crt-easymode.slang\n";
        let r = import_preset(text).unwrap();
        assert_eq!(
            r.passes,
            vec![PassKind::CompositeArtifact, PassKind::CrtScanline]
        );
    }

    #[test]
    fn ntsc_stem_never_maps_to_the_position_constrained_pass() {
        // `NtscComposite` requires first position; an imported preset could
        // place an "ntsc"-named shader anywhere, so the importer must never
        // produce it — see the module doc's rationale.
        for text in [
            "shader0 = ntsc.slang\n",
            "shader0 = crt.slang\nshader1 = ntsc-composite.slang\n",
        ] {
            let r = import_preset(text).unwrap();
            assert!(!r.passes.contains(&PassKind::NtscComposite));
        }
    }

    #[test]
    fn maps_upscalers() {
        let text = "shader0 = hqx/hq4x.slang\nshader1 = xbr/xbrz-freescale.slang\n";
        let r = import_preset(text).unwrap();
        assert_eq!(r.passes, vec![PassKind::HqNx, PassKind::Xbrz]);
    }

    #[test]
    fn reports_unsupported_honestly() {
        let text = "\
            shaders = 2\n\
            shader0 = shaders/crt/crt-royale.slang\n\
            shader1 = shaders/anti-aliasing/advanced-aa.slang\n";
        let r = import_preset(text).unwrap();
        assert_eq!(r.passes, vec![PassKind::CrtScanline]);
        assert_eq!(r.unsupported_count(), 1);
        match &r.unsupported[0] {
            ImportedPass::Unsupported { path, .. } => assert!(path.contains("advanced-aa")),
            ImportedPass::Mapped(_) => panic!("expected unsupported"),
        }
    }

    #[test]
    fn cgp_format_is_same_keys() {
        let text = "shaders = 1\nshader0 = \"crt-aperture.cg\"\n";
        let r = import_preset(text).unwrap();
        assert_eq!(r.passes, vec![PassKind::CrtScanline]);
    }

    #[test]
    fn empty_preset_is_an_error() {
        assert!(import_preset("# just a comment\nfoo = 1\n").is_err());
    }

    #[test]
    fn stock_passthrough_is_skipped_not_errored() {
        let text = "shader0 = stock.slang\nshader1 = crt-geom.slang\n";
        let r = import_preset(text).unwrap();
        assert_eq!(r.passes, vec![PassKind::CrtScanline]);
        assert_eq!(
            r.unsupported_count(),
            0,
            "a pure pass-through must skip, not report as unsupported"
        );
    }

    #[test]
    fn passthrough_and_pixellate_skip_without_unsupported() {
        let text = "shader0 = passthrough.slang\nshader1 = pixellate.slang\n";
        let r = import_preset(text).unwrap();
        assert_eq!(r.passes, Vec::new());
        assert_eq!(r.unsupported_count(), 0);
        assert!(!r.any_mapped());
    }

    #[test]
    fn preset_order_is_preserved_regardless_of_shadern_declaration_order() {
        let text = "shader1 = crt-geom.slang\nshader0 = hq4x.slang\n";
        let r = import_preset(text).unwrap();
        assert_eq!(r.passes, vec![PassKind::HqNx, PassKind::CrtScanline]);
    }

    // Regression tests for PR #20 bot-review findings (Gemini Code Assist).

    #[test]
    fn parse_kv_strips_trailing_hash_comment_on_unquoted_value() {
        let (k, v) = parse_kv("shader0 = crt-geom.slang # a note").unwrap();
        assert_eq!(k, "shader0");
        assert_eq!(v, "crt-geom.slang");
    }

    #[test]
    fn parse_kv_strips_trailing_slash_comment_on_unquoted_value() {
        let (k, v) = parse_kv("shader0 = crt-geom.slang // a note").unwrap();
        assert_eq!(k, "shader0");
        assert_eq!(v, "crt-geom.slang");
    }

    #[test]
    fn parse_kv_keeps_quoted_value_verbatim_even_with_trailing_comment() {
        let (k, v) = parse_kv("shader0 = \"crt-geom.slang\" # a note").unwrap();
        assert_eq!(k, "shader0");
        assert_eq!(v, "crt-geom.slang");
    }

    #[test]
    fn duplicate_shadern_key_keeps_the_last_declared_value() {
        // RetroArch/INI override semantics: a later `shader0 = ...` line wins over an
        // earlier one, rather than both being pushed as separate passes.
        let text = "shader0 = crt-geom.slang\nshader0 = hq4x.slang\n";
        let r = import_preset(text).unwrap();
        assert_eq!(r.passes, vec![PassKind::HqNx]);
    }

    #[test]
    fn stock_and_passthrough_match_as_substrings_not_just_exact_names() {
        let text = "shader0 = my-stock-shader.slang\nshader1 = passthrough_alt.slang\n";
        let r = import_preset(text).unwrap();
        assert_eq!(r.passes, Vec::new());
        assert_eq!(r.unsupported_count(), 0);
    }
}
