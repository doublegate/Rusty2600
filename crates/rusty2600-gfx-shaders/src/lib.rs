//! WGSL post-process shader sources for Rusty2600's composable shader stack.
//!
//! Each pass is a plain full-screen-triangle fragment shader. Four of the five
//! (`CompositeArtifact`, `CrtScanline`, `HqNx`, `Xbrz`) sample one
//! `texture_2d<f32>` + one `sampler` (binding 0/1 — the same shape
//! `rusty2600-frontend::gfx`'s existing blit pipeline already uses), so they can
//! chain in any order/position without per-pass uniform buffers: each derives
//! everything it needs (texel size, screen row) from `textureDimensions()` and
//! `@builtin(position)` directly in WGSL.
//!
//! [`PassKind::NtscComposite`] is special-cased (`rusty2600-frontend::shader_pass`'s
//! module doc explains why): it samples the raw TIA palette-index byte, not
//! already-decoded RGBA, so it needs a `texture_2d<u32>` + a small `dims`
//! uniform instead, and can only be used as the first pass in a stack.
//!
//! An empty stack (`PassKind`'s absence) is the zero-pass default — the
//! existing direct nearest-blit — so a plain build's output stays
//! byte-identical whether this crate is linked or not.
#![no_std]

/// One post-process pass a [`PassKind`] selects. Order in the stack matters:
/// each pass's output feeds the next (except [`PassKind::NtscComposite`],
/// which must be first — see its own doc comment).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PassKind {
    /// A composite-video-style horizontal chroma bleed between adjacent
    /// pixels. This is a stylistic approximation (blur/blend in already-
    /// decoded RGB space) — NOT a genuine composite-signal YIQ decode from
    /// raw palette indices. Labeled honestly as an approximation, per the
    /// project's "never present approximate output as exact" rule. Kept
    /// alongside [`PassKind::NtscComposite`] (the real decode, `v2.10.0`)
    /// as a cheaper, position-independent alternative.
    CompositeArtifact,
    /// CRT scanline darkening: every other screen row is dimmed, computed
    /// directly from `@builtin(position)` with no extra uniform.
    CrtScanline,
    /// A genuine YIQ-domain NTSC composite decode (`v2.10.0`), adapted from
    /// the Bisqwit/Mesen signal-synthesis-plus-demodulation technique for
    /// the TIA's own 4-bit-hue + 3-bit-luma colour model (NOT a port of the
    /// NES-specific technique — the TIA's colour generation is a different
    /// hardware model; see [`NTSC_COMPOSITE_WGSL`]'s own doc comment for the
    /// derivation). Samples the raw palette-index texture directly, so it
    /// can only be the FIRST pass in a stack; a stack that places it
    /// elsewhere silently skips it (`rusty2600-frontend::shader_pass`
    /// enforces this defensively; the Settings UI never constructs such a
    /// stack in the first place). NTSC-only: PAL's colour subcarrier uses a
    /// different (phase-alternating) modulation this pass does not model,
    /// and SECAM has no chroma at all — the frontend only offers this pass
    /// while running the NTSC region.
    NtscComposite,
    /// hqNx-style edge-directed pixel-art smoothing: rounds hard nearest-
    /// neighbour staircase edges while leaving flat colour areas untouched.
    /// An independent WGSL adaptation of the published hqx edge-blend
    /// kernel (Maxim Stepin) — not a port of any existing implementation.
    HqNx,
    /// xBRZ-style edge-directed pixel-art smoothing: like [`PassKind::HqNx`]
    /// but blends along whichever of the two diagonals through a texel is
    /// the smoother edge, giving the rounder corners xBRZ is known for. An
    /// independent WGSL adaptation of the published xBR/xBRZ algorithm
    /// (Hyllian / Zenju), not a port of any existing implementation.
    Xbrz,
}

impl PassKind {
    /// The WGSL source implementing this pass.
    #[must_use]
    pub const fn wgsl(self) -> &'static str {
        match self {
            Self::CompositeArtifact => COMPOSITE_ARTIFACT_WGSL,
            Self::CrtScanline => CRT_SCANLINE_WGSL,
            Self::NtscComposite => NTSC_COMPOSITE_WGSL,
            Self::HqNx => HQX_WGSL,
            Self::Xbrz => XBRZ_WGSL,
        }
    }

    /// A short label for the Settings UI.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::CompositeArtifact => "Composite artifact (approximation)",
            Self::CrtScanline => "CRT scanline",
            Self::NtscComposite => "NTSC composite (YIQ decode, NTSC only)",
            Self::HqNx => "hqNx smoothing",
            Self::Xbrz => "xBRZ smoothing",
        }
    }

    /// `true` for the one pass that must occupy stack position 0 (it samples
    /// the raw palette-index texture, not the RGBA ping-pong texture — see
    /// [`PassKind::NtscComposite`]'s doc comment).
    #[must_use]
    pub const fn requires_first_position(self) -> bool {
        matches!(self, Self::NtscComposite)
    }
}

/// Shared full-screen-triangle vertex stage every RGBA pass's WGSL source
/// embeds verbatim (WGSL has no `#include`, so each constant below is
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

/// A genuine YIQ-domain NTSC composite decode for the TIA's colour model.
///
/// ## The technique (adapted from Bisqwit/Mesen's NES composite decoder, NOT
/// ported — the TIA's colour generation is a different hardware model)
///
/// The NES's PPU generates a fixed 64-colour x emphasis palette from a
/// separate video-DAC/encoder stage sampled several times per pixel, so a
/// faithful decode there re-synthesizes the composite *signal* at
/// sub-pixel resolution and demodulates it with a sliding window. The
/// TIA is architecturally simpler and more direct: it **is** the composite
/// encoder — one TIA "dot" (colour clock) is driven out as one point on
/// the NTSC waveform, and the TIA's own dot clock runs at exactly the NTSC
/// colour-subcarrier frequency (3.579545 MHz). One whole subcarrier cycle
/// elapses per dot, so (unlike the NES) there is no progressive phase drift
/// from dot to dot purely from clock timing: a given `(hue, luma)` value
/// decodes, IN ISOLATION, to one fixed point in YIQ space — which is
/// exactly what the existing measured Stella/TIA RGB palette already
/// captures (`rusty2600-frontend::palette::NTSC`, duplicated below as
/// `NTSC_RGB` so this `no_std` crate doesn't need a dependency on the
/// frontend; a frontend-side test keeps the two tables in sync).
///
/// So the genuinely TIA-specific "signal decode" work isn't about
/// synthesizing sub-pixel phase samples the way the NES pass does — it's
/// about modelling the ONE physically real effect a raw per-hue/luma RGB
/// lookup can't: **a real composite decoder's chroma (I/Q) bandwidth is
/// much narrower than its luma (Y) bandwidth** (roughly 0.6-1.5 MHz for
/// chroma vs. ~4.2 MHz for luma in a broadcast NTSC decoder), so adjacent
/// dots' *chroma* bleeds into each other while *luma* stays comparatively
/// sharp. That differential bandwidth — not a uniform RGB blur — is the
/// actual physical origin of 2600 "artifact colors" (rapid hue dithering
/// blending into an intermediate hue a TV's chroma decoder can't resolve,
/// while fine luma detail like a single-dot-wide sprite edge stays crisp).
///
/// This pass: converts each of a 5-tap horizontal neighbourhood's raw
/// `(hue, luma)` palette entries to YIQ via the standard NTSC matrix, takes
/// the CENTRE tap's Y unblended (sharp luma), and takes a weighted
/// (0.1/0.2/0.4/0.2/0.1) average of the taps' I/Q (blurred chroma), then
/// converts the composited `(Y, I, Q)` back to RGB via the matching
/// inverse matrix.
///
/// **Verification**: because the forward (RGB->YIQ) and inverse (YIQ->RGB)
/// matrices are a true inverse pair, a UNIFORM neighbourhood (every tap the
/// same hue/luma — no colour transition nearby) reproduces the SAME output
/// RGB as a direct palette lookup, since the weighted chroma average of five
/// identical values equals that value. `rusty2600-frontend::shader_pass`'s
/// test suite verifies this round-trip identity in pure Rust against
/// `rusty2600-frontend::palette::Region::Ntsc`'s own table (the ground truth
/// this pass's baked `NTSC_RGB` array is transcribed from), independent of
/// the WGSL — the WGSL is separately checked to parse/validate via naga.
///
/// **Honesty**: this is NOT a transistor-level composite-signal simulation
/// (no colorburst timing, no sub-pixel oversampling, no PAL "PAL switch"
/// phase-alternation model, no SECAM chroma at all — the frontend only
/// offers this pass while running the NTSC region, see [`PassKind::NtscComposite`]'s
/// own doc comment). It IS a genuine signal-domain (YIQ, not RGB) decode
/// that models a real, named NTSC phenomenon (chroma/luma bandwidth
/// differential) the prior `CompositeArtifact` RGB blur does not attempt.
pub const NTSC_COMPOSITE_WGSL: &str = r"
@group(0) @binding(0) var idx_tex: texture_2d<u32>;
@group(0) @binding(1) var<uniform> src_dims: vec2<f32>;

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

// The measured NTSC Stella/TIA palette (`rusty2600-frontend::palette::NTSC`,
// duplicated here — see this constant's containing doc comment for why, and
// `rusty2600-frontend::shader_pass`'s tests for the parity check), indexed
// `hue * 8 + luma` exactly like the frontend's own table. `var<private>`
// (not `const`/`let`) because naga forbids dynamically indexing a
// module-scope value array, and this table is indexed by the raw per-dot
// byte read from `idx_tex`.
var<private> NTSC_RGB: array<vec3<f32>, 128> = array<vec3<f32>, 128>(
    vec3<f32>(0.000000,0.000000,0.000000), vec3<f32>(0.290196,0.290196,0.290196), vec3<f32>(0.435294,0.435294,0.435294), vec3<f32>(0.556863,0.556863,0.556863),
    vec3<f32>(0.666667,0.666667,0.666667), vec3<f32>(0.752941,0.752941,0.752941), vec3<f32>(0.839216,0.839216,0.839216), vec3<f32>(0.925490,0.925490,0.925490),
    vec3<f32>(0.282353,0.282353,0.000000), vec3<f32>(0.411765,0.411765,0.058824), vec3<f32>(0.525490,0.525490,0.113725), vec3<f32>(0.635294,0.635294,0.164706),
    vec3<f32>(0.733333,0.733333,0.207843), vec3<f32>(0.823529,0.823529,0.250980), vec3<f32>(0.909804,0.909804,0.290196), vec3<f32>(0.988235,0.988235,0.329412),
    vec3<f32>(0.486275,0.172549,0.000000), vec3<f32>(0.564706,0.282353,0.066667), vec3<f32>(0.635294,0.384314,0.129412), vec3<f32>(0.705882,0.478431,0.188235),
    vec3<f32>(0.764706,0.564706,0.239216), vec3<f32>(0.823529,0.643137,0.290196), vec3<f32>(0.874510,0.717647,0.333333), vec3<f32>(0.925490,0.784314,0.376471),
    vec3<f32>(0.564706,0.109804,0.000000), vec3<f32>(0.639216,0.223529,0.082353), vec3<f32>(0.709804,0.325490,0.156863), vec3<f32>(0.776471,0.423529,0.227451),
    vec3<f32>(0.835294,0.509804,0.290196), vec3<f32>(0.890196,0.592157,0.349020), vec3<f32>(0.941176,0.666667,0.403922), vec3<f32>(0.988235,0.737255,0.454902),
    vec3<f32>(0.580392,0.000000,0.000000), vec3<f32>(0.654902,0.101961,0.101961), vec3<f32>(0.721569,0.196078,0.196078), vec3<f32>(0.784314,0.282353,0.282353),
    vec3<f32>(0.839216,0.360784,0.360784), vec3<f32>(0.894118,0.435294,0.435294), vec3<f32>(0.941176,0.501961,0.501961), vec3<f32>(0.988235,0.564706,0.564706),
    vec3<f32>(0.517647,0.000000,0.392157), vec3<f32>(0.592157,0.098039,0.478431), vec3<f32>(0.658824,0.188235,0.560784), vec3<f32>(0.721569,0.274510,0.635294),
    vec3<f32>(0.776471,0.349020,0.701961), vec3<f32>(0.831373,0.423529,0.764706), vec3<f32>(0.878431,0.486275,0.823529), vec3<f32>(0.925490,0.549020,0.878431),
    vec3<f32>(0.313725,0.000000,0.517647), vec3<f32>(0.407843,0.098039,0.603922), vec3<f32>(0.490196,0.188235,0.678431), vec3<f32>(0.572549,0.274510,0.752941),
    vec3<f32>(0.643137,0.349020,0.815686), vec3<f32>(0.709804,0.423529,0.878431), vec3<f32>(0.772549,0.486275,0.933333), vec3<f32>(0.831373,0.549020,0.988235),
    vec3<f32>(0.078431,0.000000,0.564706), vec3<f32>(0.200000,0.101961,0.639216), vec3<f32>(0.305882,0.196078,0.709804), vec3<f32>(0.407843,0.282353,0.776471),
    vec3<f32>(0.498039,0.360784,0.835294), vec3<f32>(0.584314,0.435294,0.890196), vec3<f32>(0.662745,0.501961,0.941176), vec3<f32>(0.737255,0.564706,0.988235),
    vec3<f32>(0.000000,0.000000,0.580392), vec3<f32>(0.094118,0.101961,0.654902), vec3<f32>(0.176471,0.196078,0.721569), vec3<f32>(0.258824,0.282353,0.784314),
    vec3<f32>(0.329412,0.360784,0.839216), vec3<f32>(0.396078,0.435294,0.894118), vec3<f32>(0.458824,0.501961,0.941176), vec3<f32>(0.517647,0.564706,0.988235),
    vec3<f32>(0.000000,0.109804,0.533333), vec3<f32>(0.094118,0.231373,0.615686), vec3<f32>(0.176471,0.341176,0.690196), vec3<f32>(0.258824,0.447059,0.760784),
    vec3<f32>(0.329412,0.541176,0.823529), vec3<f32>(0.396078,0.627451,0.882353), vec3<f32>(0.458824,0.709804,0.937255), vec3<f32>(0.517647,0.784314,0.988235),
    vec3<f32>(0.000000,0.188235,0.392157), vec3<f32>(0.094118,0.313725,0.501961), vec3<f32>(0.176471,0.427451,0.596078), vec3<f32>(0.258824,0.533333,0.690196),
    vec3<f32>(0.329412,0.627451,0.772549), vec3<f32>(0.396078,0.717647,0.850980), vec3<f32>(0.458824,0.800000,0.921569), vec3<f32>(0.517647,0.878431,0.988235),
    vec3<f32>(0.000000,0.250980,0.188235), vec3<f32>(0.094118,0.384314,0.305882), vec3<f32>(0.176471,0.505882,0.411765), vec3<f32>(0.258824,0.619608,0.509804),
    vec3<f32>(0.329412,0.721569,0.600000), vec3<f32>(0.396078,0.819608,0.682353), vec3<f32>(0.458824,0.905882,0.760784), vec3<f32>(0.517647,0.988235,0.831373),
    vec3<f32>(0.000000,0.266667,0.000000), vec3<f32>(0.101961,0.400000,0.101961), vec3<f32>(0.196078,0.517647,0.196078), vec3<f32>(0.282353,0.627451,0.282353),
    vec3<f32>(0.360784,0.729412,0.360784), vec3<f32>(0.435294,0.823529,0.435294), vec3<f32>(0.501961,0.909804,0.501961), vec3<f32>(0.564706,0.988235,0.564706),
    vec3<f32>(0.078431,0.235294,0.000000), vec3<f32>(0.207843,0.372549,0.094118), vec3<f32>(0.321569,0.494118,0.176471), vec3<f32>(0.431373,0.611765,0.258824),
    vec3<f32>(0.529412,0.717647,0.329412), vec3<f32>(0.619608,0.815686,0.396078), vec3<f32>(0.705882,0.905882,0.458824), vec3<f32>(0.784314,0.988235,0.517647),
    vec3<f32>(0.188235,0.219608,0.000000), vec3<f32>(0.313725,0.349020,0.086275), vec3<f32>(0.427451,0.462745,0.168627), vec3<f32>(0.533333,0.572549,0.243137),
    vec3<f32>(0.627451,0.670588,0.309804), vec3<f32>(0.717647,0.760784,0.372549), vec3<f32>(0.800000,0.847059,0.431373), vec3<f32>(0.878431,0.925490,0.486275),
    vec3<f32>(0.282353,0.172549,0.000000), vec3<f32>(0.411765,0.301961,0.078431), vec3<f32>(0.525490,0.415686,0.149020), vec3<f32>(0.635294,0.525490,0.219608),
    vec3<f32>(0.733333,0.623529,0.278431), vec3<f32>(0.823529,0.713725,0.337255), vec3<f32>(0.909804,0.800000,0.388235), vec3<f32>(0.988235,0.878431,0.439216),
);

// Standard NTSC RGB<->YIQ matrices (FCC/SMPTE coefficients). `yiq_to_rgb`
// is the matrix inverse of `rgb_to_yiq` (up to f32 rounding), so composing
// them is a numeric identity — the round-trip identity the module doc
// comment's Verification section relies on.
fn rgb_to_yiq(c: vec3<f32>) -> vec3<f32> {
    let y = dot(c, vec3<f32>(0.299, 0.587, 0.114));
    let i = dot(c, vec3<f32>(0.595716, -0.274453, -0.321263));
    let q = dot(c, vec3<f32>(0.211456, -0.522591, 0.311135));
    return vec3<f32>(y, i, q);
}

fn yiq_to_rgb(c: vec3<f32>) -> vec3<f32> {
    let r = c.x + 0.9563 * c.y + 0.6210 * c.z;
    let g = c.x - 0.2721 * c.y - 0.6474 * c.z;
    let b = c.x - 1.1070 * c.y + 1.7046 * c.z;
    return clamp(vec3<f32>(r, g, b), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Map this output fragment's UV to the raw source dot it covers (the
    // index texture's valid top-left sub-rect is `src_dims` wide/tall,
    // matching `rusty2600-frontend::gfx`'s `uv_scale` sub-rect convention).
    let src = vec2<i32>(floor(in.uv * src_dims));
    let cy = clamp(src.y, 0, i32(src_dims.y) - 1);
    let cx = clamp(src.x, 0, i32(src_dims.x) - 1);
    let max_x = i32(src_dims.x) - 1;

    var weights = array<f32, 5>(0.1, 0.2, 0.4, 0.2, 0.1);
    var y_c = 0.0;
    var i_sum = 0.0;
    var q_sum = 0.0;
    for (var k = -2; k <= 2; k = k + 1) {
        let sx = clamp(cx + k, 0, max_x);
        let raw = textureLoad(idx_tex, vec2<i32>(sx, cy), 0).r;
        let yiq = rgb_to_yiq(NTSC_RGB[raw & 127u]);
        if (k == 0) {
            // The centre tap's luma passes through unblended — luma
            // bandwidth is far wider than chroma bandwidth in a real NTSC
            // decoder, so it does not bleed at this scale.
            y_c = yiq.x;
        }
        let w = weights[k + 2];
        i_sum = i_sum + yiq.y * w;
        q_sum = q_sum + yiq.z * w;
    }
    let rgb = yiq_to_rgb(vec3<f32>(y_c, i_sum, q_sum));
    return vec4<f32>(rgb, 1.0);
}
";

/// hqNx-style single-pass edge-directed smoothing.
///
/// Examines the 4-connected neighbourhood of the source texel and, where a
/// diagonal edge is detected (the sub-pixel's bordering neighbours agree
/// with each other but differ from the centre), blends the sub-pixel toward
/// the neighbour pair — the hqx interpolation rule, simplified to one pass
/// at output resolution (the `ShaderStack` ping-pongs at the WINDOW's
/// resolution, not a fixed console resolution, so `textureDimensions`
/// drives the texel size rather than a baked constant).
///
/// Independent WGSL adaptation of the published hqx algorithm (Maxim
/// Stepin) — not a port of any existing GPL/LGPL source, and not a port of
/// `RustyNES`'s own (separately authored) `upscale.rs` WGSL, which this
/// crate's version independently re-derives from the published technique
/// rather than copies.
pub const HQX_WGSL: &str = r"
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

// Perceptual colour distance (weighted YUV), the standard hqx/xBR edge
// metric: luma differences matter far more to the eye than pure chroma
// differences, so `y` is weighted much more heavily than `u`/`v`.
fn cdist(a: vec3<f32>, b: vec3<f32>) -> f32 {
    let d = a - b;
    let y = dot(d, vec3<f32>(0.299, 0.587, 0.114));
    let u = dot(d, vec3<f32>(-0.169, -0.331, 0.5));
    let v = dot(d, vec3<f32>(0.5, -0.419, -0.081));
    return abs(y) * 0.5 + abs(u) * 0.25 + abs(v) * 0.25;
}

const HQX_THRESH: f32 = 0.08;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(tex));
    let ts = 1.0 / dims;

    let c = textureSample(tex, samp, in.uv).rgb;
    let n = textureSample(tex, samp, in.uv - vec2<f32>(0.0, ts.y)).rgb;
    let s = textureSample(tex, samp, in.uv + vec2<f32>(0.0, ts.y)).rgb;
    let w = textureSample(tex, samp, in.uv - vec2<f32>(ts.x, 0.0)).rgb;
    let e = textureSample(tex, samp, in.uv + vec2<f32>(ts.x, 0.0)).rgb;

    let f = fract(in.uv / ts);
    let h = select(w, e, f.x > 0.5);
    let v = select(n, s, f.y > 0.5);

    var outc = c;
    if (cdist(h, v) < HQX_THRESH && cdist(c, h) > HQX_THRESH) {
        let corner = clamp(abs(f.x - 0.5) + abs(f.y - 0.5), 0.0, 1.0);
        outc = mix(c, 0.5 * (h + v), corner);
    }
    return vec4<f32>(outc, 1.0);
}
";

/// xBRZ-style single-pass edge-directed smoothing.
///
/// Like [`HQX_WGSL`] but uses the xBR diagonal-dominance test: compares the
/// two diagonals through the texel and blends along whichever is the
/// *smoother* (lower-energy) edge, which is what gives xBRZ its
/// characteristically rounder corners.
///
/// Independent WGSL adaptation of the published xBR/xBRZ algorithm
/// (Hyllian / Zenju) — same IP posture as [`HQX_WGSL`]'s own doc comment.
pub const XBRZ_WGSL: &str = r"
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

fn cdist(a: vec3<f32>, b: vec3<f32>) -> f32 {
    let d = a - b;
    let y = dot(d, vec3<f32>(0.299, 0.587, 0.114));
    let u = dot(d, vec3<f32>(-0.169, -0.331, 0.5));
    let v = dot(d, vec3<f32>(0.5, -0.419, -0.081));
    return abs(y) * 0.5 + abs(u) * 0.25 + abs(v) * 0.25;
}

const XBRZ_THRESH: f32 = 0.06;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(tex));
    let ts = 1.0 / dims;

    let c = textureSample(tex, samp, in.uv).rgb;
    let nw = textureSample(tex, samp, in.uv + vec2<f32>(-ts.x, -ts.y)).rgb;
    let ne = textureSample(tex, samp, in.uv + vec2<f32>(ts.x, -ts.y)).rgb;
    let sw = textureSample(tex, samp, in.uv + vec2<f32>(-ts.x, ts.y)).rgb;
    let se = textureSample(tex, samp, in.uv + vec2<f32>(ts.x, ts.y)).rgb;

    let f = fract(in.uv / ts);
    let d_main = cdist(nw, se);
    let d_anti = cdist(ne, sw);
    let on_main = (f.x - 0.5) * (f.y - 0.5) > 0.0;
    let chosen = select(0.5 * (ne + sw), 0.5 * (nw + se), on_main);
    let d_this = select(d_anti, d_main, on_main);
    let d_other = select(d_main, d_anti, on_main);

    var outc = c;
    if (d_this < XBRZ_THRESH && d_other > d_this + XBRZ_THRESH && cdist(c, chosen) > XBRZ_THRESH) {
        let corner = clamp(abs(f.x - 0.5) + abs(f.y - 0.5), 0.0, 1.0);
        outc = mix(c, chosen, corner);
    }
    return vec4<f32>(outc, 1.0);
}
";
