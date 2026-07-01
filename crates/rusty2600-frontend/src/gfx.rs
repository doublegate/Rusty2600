//! wgpu surface + texture-blit pipeline for the 2600 beam-raced framebuffer.
//!
//! The TIA has no framebuffer; it races the beam, and the frontend accumulates the emitted dots
//! into an RGBA8 display buffer (`present_buffer`). Each frame the frontend uploads that buffer to
//! a wgpu texture; a fullscreen-triangle render pass samples it with nearest filtering. With no
//! post-process filter active the direct nearest-blit is taken and the output is pixel-identical
//! to a filter-less build.
//!
//! 2600 specifics vs. the NES/SNES templates:
//! - Framebuffer dims: 160 visible color clocks wide x 192 (NTSC) / 228 (PAL/SECAM) visible lines.
//!   2600 pixels are tall — the natural display stretches the 160-wide buffer ~1.8x horizontally
//!   to a 4:3 frame (noted, not perfected, at v0.1).
//! - Color: the 2600 palette tables are already RGB (`palette::Rgb` = `0xRRGGBB`); `rgb_to_rgba8`
//!   packs an entry into the little-endian RGBA8 word a wgpu texture upload wants. (There is no
//!   15-bit color word to decode, unlike the SNES BGR555 path.)
//!
//! See `docs/frontend.md` for the render-path architecture.
//!
//! v0.1.0: the deep post-process chain (CRT / NTSC / upscalers) is a TODO — only the direct
//! nearest-blit ships in the skeleton, which presents a deterministically cleared frame.

// The `u32 as f32` casts in this module (`uv_scale` ratios) are all over dimensions
// bounded by `MAX_W`/`MAX_H` (160 / 228) — nowhere near f32's 23-bit mantissa limit.
#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

use std::sync::Arc;

use wgpu::util::DeviceExt as _;
use winit::window::Window;

/// 2600 visible width in TIA color clocks (constant across all regions).
pub const VCS_W: u32 = 160;
/// 2600 NTSC active-region height (192 visible scanlines).
pub const VCS_H_NTSC: u32 = 192;
/// 2600 PAL / SECAM active-region height (228 visible scanlines).
pub const VCS_H_PAL: u32 = 228;

/// The maximum framebuffer width the texture is sized for, so a region change never needs a
/// texture realloc. Sub-modes upload into the top-left sub-rect.
pub const MAX_W: u32 = VCS_W;
/// The maximum framebuffer height the texture is sized for (the taller PAL/SECAM case).
pub const MAX_H: u32 = VCS_H_PAL;

/// Pack a 2600 palette entry (`0x00RRGGBB`) into the little-endian RGBA8 word (`0xAABBGGRR`,
/// byte order R, G, B, A) a wgpu `Rgba8UnormSrgb` texture upload expects. Alpha is forced opaque.
///
/// The 2600's `palette::Rgb` table entries are already 8-bit-per-channel RGB (decoded from the
/// measured Stella/TIA palette), so this is a byte-swizzle, not a channel expansion.
#[must_use]
pub const fn rgb_to_rgba8(rgb: u32) -> u32 {
    let r8 = (rgb >> 16) & 0xFF;
    let g8 = (rgb >> 8) & 0xFF;
    let b8 = rgb & 0xFF;
    // Pack as 0xAABBGGRR (little-endian RGBA8 byte order: R, G, B, A).
    0xFF00_0000 | (b8 << 16) | (g8 << 8) | r8
}

/// Resolve the configured present-mode string against the surface's supported modes.
///
/// Recognized (case-insensitive): `"fifo"` (vsync; safe default), `"mailbox"` (triple-buffered,
/// no tearing, no vsync gate), `"immediate"` (uncapped, may tear). An unsupported request falls
/// back to `Fifo`, which every wgpu backend guarantees. The native wall-clock pacer is the
/// authoritative timing source.
fn select_present_mode(pref: &str, supported: &[wgpu::PresentMode]) -> wgpu::PresentMode {
    let requested = match pref.to_ascii_lowercase().as_str() {
        "mailbox" => wgpu::PresentMode::Mailbox,
        "immediate" => wgpu::PresentMode::Immediate,
        _ => wgpu::PresentMode::Fifo,
    };
    if supported.contains(&requested) {
        requested
    } else {
        wgpu::PresentMode::Fifo
    }
}

/// The wgpu device + surface + the framebuffer-blit pipeline.
///
/// Owns the streaming texture the display buffer uploads into each frame and the
/// fullscreen-triangle pass that samples it. The egui pass (the always-on shell) is layered on
/// top by the caller after this blit.
pub struct Gfx {
    /// The wgpu device (kept for resource creation + the per-frame upload).
    pub device: wgpu::Device,
    /// The command queue (texture uploads + submit).
    pub queue: wgpu::Queue,
    /// The window surface presented each frame.
    pub surface: wgpu::Surface<'static>,
    /// The negotiated surface configuration (format + size + present mode).
    pub config: wgpu::SurfaceConfiguration,
    /// The streaming framebuffer texture (sized to the PAL/SECAM worst case).
    texture: wgpu::Texture,
    /// The `uv_scale` uniform (`fb_w/MAX_W, fb_h/MAX_H`) — see [`Self::uv_scale_buffer`].
    uv_scale_buffer: wgpu::Buffer,
    /// The bind group binding `texture` + the nearest sampler + `uv_scale_buffer` for the blit.
    bind_group: wgpu::BindGroup,
    /// The fullscreen-triangle blit pipeline.
    pipeline: wgpu::RenderPipeline,
    /// The active framebuffer width (the sub-rect of the texture that's live this region).
    fb_w: u32,
    /// See [`Gfx::fb_w`].
    fb_h: u32,
}

impl Gfx {
    /// Initialize wgpu against `window`. Blocks on adapter/device acquisition via `pollster` on
    /// native (the wasm path uses the async constructor — TODO when `wasm.rs` is filled).
    ///
    /// # Errors
    /// Returns a [`GfxError`] if no compatible adapter is found or device request fails.
    pub fn new(window: Arc<Window>, present_pref: &str) -> Result<Self, GfxError> {
        pollster::block_on(Self::new_async(window, present_pref))
    }

    /// The async core of [`Gfx::new`] (shared with the future wasm path).
    ///
    /// # Errors
    /// See [`Gfx::new`].
    // Linear wgpu device/surface/pipeline setup: one straight-line init sequence that reads more
    // clearly as a unit than split across helpers.
    #[allow(clippy::too_many_lines)]
    pub async fn new_async(window: Arc<Window>, present_pref: &str) -> Result<Self, GfxError> {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window)
            .map_err(|e| GfxError::Surface(e.to_string()))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .map_err(|e| GfxError::Adapter(e.to_string()))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("rusty2600-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
            })
            .await
            .map_err(|e| GfxError::Device(e.to_string()))?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: select_present_mode(present_pref, &caps.present_modes),
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        // The streaming framebuffer texture, sized to the PAL/SECAM worst case; the NTSC sub-mode
        // uploads into the top-left sub-rect, so a region change never reallocates.
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rusty2600-framebuffer"),
            size: wgpu::Extent3d {
                width: MAX_W,
                height: MAX_H,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rusty2600-nearest"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        // `uv_scale = (fb_w/MAX_W, fb_h/MAX_H)` crops the fullscreen blit's UV
        // sampling down to just the active region's sub-rect of the
        // PAL/SECAM-worst-case-sized texture, instead of stretching the WHOLE
        // texture (including the NTSC case's never-written bottom rows) across
        // the window — this is what made the display only show the correct
        // picture in the top ~84% of the window (192/228) with undefined
        // content below, and let a region change's tall PAL content sample the
        // right sub-rect too. Starts at the NTSC default; `upload` keeps it in
        // sync with `fb_w`/`fb_h`.
        let uv_scale_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rusty2600-uv-scale"),
            contents: bytemuck::cast_slice(&[
                VCS_W as f32 / MAX_W as f32,
                VCS_H_NTSC as f32 / MAX_H as f32,
            ]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rusty2600-blit"),
            source: wgpu::ShaderSource::Wgsl(BLIT_WGSL.into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rusty2600-blit-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rusty2600-blit-bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uv_scale_buffer.as_entire_binding(),
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rusty2600-blit-pl"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rusty2600-blit-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        // A one-time empty upload keeps the texture deterministically cleared (the skeleton
        // presents a blank frame until the TIA model lands).
        let _ = &device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rusty2600-noop"),
            contents: &[0u8; 4],
            usage: wgpu::BufferUsages::COPY_SRC,
        });

        Ok(Self {
            device,
            queue,
            surface,
            config,
            texture,
            uv_scale_buffer,
            bind_group,
            pipeline,
            fb_w: VCS_W,
            fb_h: VCS_H_NTSC,
        })
    }

    /// Re-negotiate the surface on a window resize.
    pub fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
    }

    /// Upload an RGBA8 framebuffer (`w*h*4` bytes) into the streaming texture's top-left sub-rect
    /// and record the active region dims. A length mismatch is skipped (mirrors the RustyNES
    /// ROM-close fix: never feed wgpu an empty/short source).
    pub fn upload(&mut self, rgba: &[u8], w: u32, h: u32) {
        if w == 0 || h == 0 || w > MAX_W || h > MAX_H {
            return;
        }
        if rgba.len() < (w as usize) * (h as usize) * 4 {
            return;
        }
        if w != self.fb_w || h != self.fb_h {
            self.fb_w = w;
            self.fb_h = h;
            self.queue.write_buffer(
                &self.uv_scale_buffer,
                0,
                bytemuck::cast_slice(&[w as f32 / MAX_W as f32, h as f32 / MAX_H as f32]),
            );
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }

    /// The active framebuffer dimensions (`(w, h)`), for the caller's aspect math.
    #[must_use]
    pub const fn fb_dims(&self) -> (u32, u32) {
        (self.fb_w, self.fb_h)
    }

    /// Acquire the next surface texture for the frame, or `None` if the surface is lost (the
    /// caller reconfigures and retries next frame).
    ///
    /// wgpu 29 returns the [`wgpu::CurrentSurfaceTexture`] enum (not a `Result`): use the texture
    /// on `Success`/`Suboptimal`, reconfigure on `Lost`/`Outdated`, skip otherwise.
    pub fn acquire(&mut self) -> Option<wgpu::SurfaceTexture> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => Some(t),
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                None
            }
            _ => None,
        }
    }

    /// Record the framebuffer blit into `encoder`, clearing then drawing the fullscreen triangle
    /// that samples the streaming texture. The egui shell pass is layered after this.
    pub fn blit(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("rusty2600-blit-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        // Sample only the live sub-rect by drawing a quad scaled to fb/MAX; the blit shader maps
        // UVs over the full triangle, so a future region-aware UV uniform is a TODO. For the
        // skeleton (blank frame) the full-texture sample is acceptable.
        pass.draw(0..3, 0..1);
    }
}

/// The fullscreen-triangle blit shader (nearest sample of the framebuffer texture).
const BLIT_WGSL: &str = r"
@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> uv_scale: vec2<f32>;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    // Fullscreen triangle: (-1,-1), (3,-1), (-1,3) in clip space.
    var out: VsOut;
    let x = f32((vi << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(vi & 2u) * 2.0 - 1.0;
    out.pos = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // `uv_scale` crops sampling to the active region's sub-rect of the
    // PAL/SECAM-worst-case-sized texture (`fb_w/MAX_W, fb_h/MAX_H`), so a
    // window always shows the whole active picture stretched to fill it,
    // never the never-written padding rows below a shorter (e.g. NTSC)
    // active region.
    return textureSample(tex, samp, in.uv * uv_scale);
}
";

/// wgpu initialization failures.
#[derive(Debug, thiserror::Error)]
pub enum GfxError {
    /// Surface creation failed.
    #[error("wgpu surface creation failed: {0}")]
    Surface(String),
    /// No compatible adapter was found.
    #[error("no compatible wgpu adapter: {0}")]
    Adapter(String),
    /// Device request failed.
    #[error("wgpu device request failed: {0}")]
    Device(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_black_and_white() {
        assert_eq!(rgb_to_rgba8(0x0000_0000), 0xFF00_0000); // opaque black
        assert_eq!(rgb_to_rgba8(0x00FF_FFFF), 0xFFFF_FFFF); // opaque white
    }

    #[test]
    fn rgb_pure_red_swizzles_to_low_byte() {
        // 0xRRGGBB red -> RGBA8 little-endian R in the low byte.
        assert_eq!(rgb_to_rgba8(0x00FF_0000), 0xFF00_00FF);
    }

    #[test]
    fn rgb_pure_blue_swizzles_to_high_byte() {
        assert_eq!(rgb_to_rgba8(0x0000_00FF), 0xFFFF_0000);
    }

    #[test]
    fn present_mode_falls_back_to_fifo() {
        let supported = [wgpu::PresentMode::Fifo];
        assert_eq!(
            select_present_mode("mailbox", &supported),
            wgpu::PresentMode::Fifo
        );
        assert_eq!(
            select_present_mode("fifo", &supported),
            wgpu::PresentMode::Fifo
        );
    }

    #[test]
    fn blit_wgsl_validates() {
        // Validate the embedded WGSL with the same naga wgpu uses at runtime.
        let module = naga::front::wgsl::parse_str(BLIT_WGSL).expect("WGSL parses");
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        );
        validator.validate(&module).expect("WGSL validates");
    }
}
