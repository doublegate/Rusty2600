//! WGSL post-process shader sources for Rusty2600's composable shader stack.
//!
//! Each pass is a plain full-screen-triangle fragment shader sampling one
//! `texture_2d<f32>` + one `sampler` (binding 0/1 — the same shape
//! `rusty2600-frontend::gfx`'s existing blit pipeline already uses), so the
//! stack can chain passes without per-pass uniform buffers: both passes here
//! derive everything they need (texel size, screen row) from
//! `textureDimensions()` and `@builtin(position)` directly in WGSL.
//!
//! An empty stack (`PassKind`'s absence) is the zero-pass default — the
//! existing direct nearest-blit — so a plain build's output stays
//! byte-identical whether this crate is linked or not.
#![no_std]

/// One post-process pass a [`PassKind`] selects. Order in the stack matters:
/// each pass's output feeds the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PassKind {
    /// A composite-video-style horizontal chroma bleed between adjacent
    /// pixels. This is a stylistic approximation (blur/blend in already-
    /// decoded RGB space) — NOT a genuine composite-signal YIQ decode from
    /// raw palette indices, which the render pipeline doesn't currently
    /// carry through to this stage. Labeled honestly as an approximation,
    /// per the project's "never present approximate output as exact" rule.
    CompositeArtifact,
    /// CRT scanline darkening: every other screen row is dimmed, computed
    /// directly from `@builtin(position)` with no extra uniform.
    CrtScanline,
}

impl PassKind {
    /// The WGSL source implementing this pass.
    #[must_use]
    pub const fn wgsl(self) -> &'static str {
        match self {
            Self::CompositeArtifact => COMPOSITE_ARTIFACT_WGSL,
            Self::CrtScanline => CRT_SCANLINE_WGSL,
        }
    }

    /// A short label for the Settings UI.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::CompositeArtifact => "Composite artifact (approximation)",
            Self::CrtScanline => "CRT scanline",
        }
    }
}

/// Shared full-screen-triangle vertex stage every pass's WGSL source embeds
/// verbatim (WGSL has no `#include`, so each constant below is
/// self-contained rather than referencing this one).
pub const FULLSCREEN_VERTEX_WGSL: &str = r"
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    var out: VsOut;
    let x = f32((vi << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(vi & 2u) * 2.0 - 1.0;
    out.pos = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}
";

/// A horizontal chroma-bleed blur between a pixel and its immediate neighbors.
///
/// An approximation of the color fringing composite video exhibits, not a
/// real signal decode. See [`PassKind::CompositeArtifact`].
pub const COMPOSITE_ARTIFACT_WGSL: &str = r"
@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    var out: VsOut;
    let x = f32((vi << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(vi & 2u) * 2.0 - 1.0;
    out.pos = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(tex));
    let texel = 1.0 / dims.x;
    let center = textureSample(tex, samp, in.uv);
    let left = textureSample(tex, samp, in.uv - vec2<f32>(texel, 0.0));
    let right = textureSample(tex, samp, in.uv + vec2<f32>(texel, 0.0));
    // A gentle 50/25/25 horizontal blend — enough to soften hard color
    // transitions without meaningfully blurring luminance detail.
    let blended = center * 0.5 + left * 0.25 + right * 0.25;
    return vec4<f32>(blended.rgb, center.a);
}
";

/// Darkens every other screen row (`@builtin(position).y`, no uniform
/// needed) to approximate CRT scanline structure.
pub const CRT_SCANLINE_WGSL: &str = r"
@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    var out: VsOut;
    let x = f32((vi << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(vi & 2u) * 2.0 - 1.0;
    out.pos = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let color = textureSample(tex, samp, in.uv);
    let row = u32(in.pos.y);
    let scanline_darken = select(1.0, 0.75, row % 2u == 0u);
    return vec4<f32>(color.rgb * scanline_darken, color.a);
}
";
