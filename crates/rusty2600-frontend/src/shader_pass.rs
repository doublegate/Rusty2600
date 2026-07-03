//! The composable post-process shader stack.
//!
//! Chains an ARBITRARY number of full-screen passes (`rusty2600-gfx-shaders`'
//! [`rusty2600_gfx_shaders::PassKind`] sources) after the base framebuffer
//! blit.
//!
//! An empty pass list is the zero-cost default: [`crate::gfx::Gfx::present`]
//! skips this module entirely and calls the existing direct blit, so a plain build's
//! output stays byte-identical whether a user has ever opened the Settings
//! shader picklist or not (the same invariant `uv_scale` preserved when it
//! landed in `[1.1.0]`).
//!
//! ## Arbitrary-length ping-pong (`v2.10.0`, corrected from the earlier fixed
//! 2-slot design)
//!
//! A linear chain of single-input/single-output passes only ever needs TWO
//! intermediate textures, regardless of chain length: pass `i` reads
//! whichever texture pass `i-1` wrote and writes the OTHER one (or the
//! swapchain target, if it's the last pass). The stack's own prior doc
//! comment claimed a third texture would be needed to grow past two passes
//! — that turned out to be overly conservative; [`crate::shader_pass::ShaderStack::render`]
//! below alternates strictly by pass index parity (`tex_a` on even
//! indices, `tex_b` on odd), which generalizes to any pass count with the
//! same two textures already allocated for the original 2-pass design.
//!
//! ## The `NtscComposite` special case
//!
//! [`rusty2600_gfx_shaders::PassKind::NtscComposite`] samples the RAW
//! palette-index texture (`texture_2d<u32>`), not the RGBA ping-pong
//! texture every other pass uses — it needs the un-decoded per-dot TIA
//! byte to do its own YIQ decode (see its WGSL doc comment for why). It can
//! only usefully be the FIRST pass in a stack (mirroring RustyNES ADR
//! 0013's Bisqwit-pass precedent): [`crate::gfx::Gfx::present`] skips the
//! base RGB blit entirely when `passes[0]` is `NtscComposite`, feeding the
//! stack's own `index_bind_group` as pass 0's source instead. If
//! `NtscComposite` appears at any OTHER position (a hand-edited config,
//! never something the Settings UI itself constructs — see `shell.rs`'s
//! checkbox handler, which always keeps it pinned to position 0 when
//! enabled), [`crate::shader_pass::ShaderStack::render`] defensively skips that pass rather
//! than binding a mismatched bind group (which would be a wgpu validation
//! panic).

// The `u32 as f32` casts in this module (index-texture dims uniform) are all over dimensions
// bounded by `crate::gfx::MAX_W`/`MAX_H` (160 / 228) — nowhere near f32's 23-bit mantissa limit,
// matching `gfx.rs`'s own identical allow for the same reason.
#![allow(clippy::cast_precision_loss)]

use rusty2600_gfx_shaders::PassKind;

/// One full-screen-triangle post-process pipeline.
struct Pass {
    pipeline: wgpu::RenderPipeline,
}

/// The composable, arbitrary-length ping-pong shader stack.
pub struct ShaderStack {
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    composite_artifact: Pass,
    crt_scanline: Pass,
    hqx: Pass,
    xbrz: Pass,
    ntsc_composite: Pass,
    tex_a: wgpu::Texture,
    tex_b: wgpu::Texture,
    bind_group_a: wgpu::BindGroup,
    bind_group_b: wgpu::BindGroup,
    width: u32,
    height: u32,
    /// The raw TIA palette-index texture (`R8Uint`), sized to the
    /// PAL/SECAM worst case (`index_max_w` x `index_max_h`, matching
    /// `crate::gfx`'s `MAX_W`/`MAX_H`) — fixed at framebuffer resolution,
    /// independent of the window/ping-pong textures' size, and never
    /// resized on a window resize.
    index_texture: wgpu::Texture,
    /// The active sub-rect dims (`fb_w`, `fb_h` as f32), updated whenever
    /// [`ShaderStack::upload_index`] is called with a new size — mirrors
    /// `crate::gfx::Gfx`'s own `uv_scale_buffer` convention.
    index_dims_buffer: wgpu::Buffer,
    index_bind_group: wgpu::BindGroup,
    index_max_w: u32,
    index_max_h: u32,
}

fn make_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    wgsl: &str,
    label: &str,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
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
    })
}

fn make_intermediate(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    w: u32,
    h: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("rusty2600-shader-stack-intermediate"),
        size: wgpu::Extent3d {
            width: w.max(1),
            height: h.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

fn make_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    texture: &wgpu::Texture,
) -> wgpu::BindGroup {
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("rusty2600-shader-stack-bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

impl ShaderStack {
    /// Builds the stack's pipelines + intermediate textures.
    ///
    /// `width`/`height` size the RGBA ping-pong textures (the window/surface
    /// resolution, matching every RGBA pass); `index_max_w`/`index_max_h`
    /// size the fixed raw-index texture ([`crate::gfx::MAX_W`]/[`crate::gfx::MAX_H`]
    /// — the PAL/SECAM framebuffer worst case), independent of the window
    /// size.
    // One straight-line pipeline/texture/bind-group setup sequence — reads more
    // clearly as a unit than split across helpers (same rationale `gfx.rs`'s
    // own `new_async` gives for the identical allow).
    #[allow(clippy::too_many_lines)]
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        index_max_w: u32,
        index_max_h: u32,
    ) -> Self {
        use wgpu::util::DeviceExt as _;

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rusty2600-shader-stack-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rusty2600-shader-stack-pl"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rusty2600-shader-stack-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let composite_artifact = Pass {
            pipeline: make_pipeline(
                device,
                &layout,
                format,
                PassKind::CompositeArtifact.wgsl(),
                "rusty2600-composite-artifact",
            ),
        };
        let crt_scanline = Pass {
            pipeline: make_pipeline(
                device,
                &layout,
                format,
                PassKind::CrtScanline.wgsl(),
                "rusty2600-crt-scanline",
            ),
        };
        let hqx = Pass {
            pipeline: make_pipeline(
                device,
                &layout,
                format,
                PassKind::HqNx.wgsl(),
                "rusty2600-hqx",
            ),
        };
        let xbrz = Pass {
            pipeline: make_pipeline(
                device,
                &layout,
                format,
                PassKind::Xbrz.wgsl(),
                "rusty2600-xbrz",
            ),
        };
        let tex_a = make_intermediate(device, format, width, height);
        let tex_b = make_intermediate(device, format, width, height);
        let bind_group_a = make_bind_group(device, &bind_group_layout, &sampler, &tex_a);
        let bind_group_b = make_bind_group(device, &bind_group_layout, &sampler, &tex_b);

        // The NTSC-composite pass's own bind group layout: a `u32` (non-
        // filterable, `textureLoad`-only) index texture + a small `dims`
        // uniform, entirely separate from the shared RGBA layout above.
        let index_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("rusty2600-ntsc-composite-bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Uint,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
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
        let index_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rusty2600-ntsc-composite-pl"),
            bind_group_layouts: &[Some(&index_bind_group_layout)],
            immediate_size: 0,
        });
        let ntsc_composite = Pass {
            pipeline: make_pipeline(
                device,
                &index_layout,
                format,
                PassKind::NtscComposite.wgsl(),
                "rusty2600-ntsc-composite",
            ),
        };
        let index_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rusty2600-shader-stack-index"),
            size: wgpu::Extent3d {
                width: index_max_w.max(1),
                height: index_max_h.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let index_view = index_texture.create_view(&wgpu::TextureViewDescriptor::default());
        // Starts at the full max-dims sub-rect; `upload_index` narrows it to
        // whichever region (NTSC's) is actually active once a frame lands.
        let index_dims_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rusty2600-ntsc-composite-dims"),
            contents: bytemuck::cast_slice(&[index_max_w as f32, index_max_h as f32]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let index_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rusty2600-ntsc-composite-bg"),
            layout: &index_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&index_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: index_dims_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            bind_group_layout,
            sampler,
            composite_artifact,
            crt_scanline,
            hqx,
            xbrz,
            ntsc_composite,
            tex_a,
            tex_b,
            bind_group_a,
            bind_group_b,
            width,
            height,
            index_texture,
            index_dims_buffer,
            index_bind_group,
            index_max_w,
            index_max_h,
        }
    }

    /// Recreates the RGBA ping-pong textures for a new surface size. A no-op
    /// if the size hasn't actually changed. The raw-index texture is NOT
    /// touched — it stays fixed at framebuffer (not window) resolution.
    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) {
        if width == self.width && height == self.height {
            return;
        }
        self.width = width;
        self.height = height;
        self.tex_a = make_intermediate(device, format, width, height);
        self.tex_b = make_intermediate(device, format, width, height);
        self.bind_group_a =
            make_bind_group(device, &self.bind_group_layout, &self.sampler, &self.tex_a);
        self.bind_group_b =
            make_bind_group(device, &self.bind_group_layout, &self.sampler, &self.tex_b);
    }

    /// The view the base framebuffer blit should target when `passes` is
    /// non-empty AND `passes[0]` is NOT [`PassKind::NtscComposite`] (the
    /// stack's first ping-pong slot).
    #[must_use]
    pub fn first_target_view(&self) -> wgpu::TextureView {
        self.tex_a
            .create_view(&wgpu::TextureViewDescriptor::default())
    }

    /// Upload the raw per-dot TIA palette-index byte (`hue << 3 | luma`,
    /// exactly `colour7 = colu >> 1`) into the index texture's top-left
    /// sub-rect, and record the active `(w, h)` so [`PassKind::NtscComposite`]
    /// knows which sub-rect of the (PAL/SECAM-worst-case-sized) texture is
    /// live this frame — mirrors `crate::gfx::Gfx::upload`'s own sub-rect
    /// convention exactly.
    pub fn upload_index(&mut self, queue: &wgpu::Queue, indices: &[u8], w: u32, h: u32) {
        if w == 0 || h == 0 || w > self.index_max_w || h > self.index_max_h {
            return;
        }
        if indices.len() < (w as usize) * (h as usize) {
            return;
        }
        queue.write_buffer(
            &self.index_dims_buffer,
            0,
            bytemuck::cast_slice(&[w as f32, h as f32]),
        );
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.index_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            indices,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }

    const fn pass(&self, kind: PassKind) -> &Pass {
        match kind {
            PassKind::CompositeArtifact => &self.composite_artifact,
            PassKind::CrtScanline => &self.crt_scanline,
            PassKind::NtscComposite => &self.ntsc_composite,
            PassKind::HqNx => &self.hqx,
            PassKind::Xbrz => &self.xbrz,
        }
    }

    /// Runs `passes` in order, writing the final pass's output to `target`
    /// (the swapchain view). An arbitrary-length ping-pong: pass `i` reads
    /// `bind_group_a` on even `i` / `bind_group_b` on odd `i` (matching
    /// whichever texture the previous pass just wrote — or, for `i == 0`,
    /// whatever the caller already wrote into `tex_a`, normally the base
    /// blit — see [`Self::first_target_view`]) and writes the other
    /// texture, unless it's the last pass, in which case it writes `target`
    /// directly.
    ///
    /// [`PassKind::NtscComposite`] is special-cased ONLY at `i == 0`: it
    /// reads the index bind group's raw palette-index texture instead of
    /// `bind_group_a` (the caller must have skipped the base RGB blit in
    /// that case — see [`crate::gfx::Gfx::present`]). If it appears at any
    /// other position, the pass is skipped defensively (it would otherwise
    /// bind a `u32` texture against a pipeline built for a `f32`+sampler
    /// bind group, a wgpu validation panic) — the Settings UI never
    /// constructs such a stack itself (see `shell.rs`).
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        passes: &[PassKind],
    ) {
        let n = passes.len();
        for (i, &kind) in passes.iter().enumerate() {
            if kind.requires_first_position() && i != 0 {
                continue;
            }
            let is_last = i == n - 1;
            let src: &wgpu::BindGroup = if i == 0 && kind.requires_first_position() {
                &self.index_bind_group
            } else if i % 2 == 0 {
                &self.bind_group_a
            } else {
                &self.bind_group_b
            };
            if is_last {
                self.run_pass(encoder, kind, src, target);
            } else if i % 2 == 0 {
                let view = self
                    .tex_b
                    .create_view(&wgpu::TextureViewDescriptor::default());
                self.run_pass(encoder, kind, src, &view);
            } else {
                let view = self
                    .tex_a
                    .create_view(&wgpu::TextureViewDescriptor::default());
                self.run_pass(encoder, kind, src, &view);
            }
        }
    }

    fn run_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        kind: PassKind,
        source: &wgpu::BindGroup,
        target: &wgpu::TextureView,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("rusty2600-shader-stack-pass"),
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
        pass.set_pipeline(&self.pass(kind).pipeline);
        pass.set_bind_group(0, source, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validate(src: &str, what: &str) {
        let module = naga::front::wgsl::parse_str(src)
            .unwrap_or_else(|e| panic!("{what} WGSL must parse: {e:?}"));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{what} WGSL must validate: {e:?}"));
    }

    #[test]
    fn composite_artifact_wgsl_validates() {
        validate(PassKind::CompositeArtifact.wgsl(), "composite-artifact");
    }

    #[test]
    fn crt_scanline_wgsl_validates() {
        validate(PassKind::CrtScanline.wgsl(), "crt-scanline");
    }

    #[test]
    fn ntsc_composite_wgsl_validates() {
        validate(PassKind::NtscComposite.wgsl(), "ntsc-composite");
    }

    #[test]
    fn hqx_wgsl_validates() {
        validate(PassKind::HqNx.wgsl(), "hqx");
    }

    #[test]
    fn xbrz_wgsl_validates() {
        validate(PassKind::Xbrz.wgsl(), "xbrz");
    }

    #[test]
    fn only_ntsc_composite_requires_first_position() {
        assert!(PassKind::NtscComposite.requires_first_position());
        for kind in [
            PassKind::CompositeArtifact,
            PassKind::CrtScanline,
            PassKind::HqNx,
            PassKind::Xbrz,
        ] {
            assert!(!kind.requires_first_position());
        }
    }

    /// The genuine part of the NTSC-composite decode's honesty claim (see
    /// `rusty2600_gfx_shaders::NTSC_COMPOSITE_WGSL`'s doc comment): the
    /// RGB<->YIQ matrix pair the WGSL uses is a true inverse (up to f32
    /// rounding), so a UNIFORM neighbourhood (no colour transition) decodes
    /// back to the same RGB it started from. Verified here in pure Rust,
    /// independent of the WGSL/naga/wgpu execution path, against EVERY
    /// entry of the real measured NTSC palette
    /// (`rusty2600_frontend::palette::Region::Ntsc`) — the same ground
    /// truth the WGSL's baked `NTSC_RGB` table is transcribed from.
    #[test]
    // `r`/`g`/`b`/`y`/`i`/`q` mirror the WGSL matrix's own variable names exactly (see
    // `NTSC_COMPOSITE_WGSL`'s `rgb_to_yiq`/`yiq_to_rgb`) — spelling them out would make this
    // side-by-side comparison harder to audit, not easier. The literals are the standard
    // published FCC/SMPTE YIQ matrix coefficients transcribed verbatim; grouping their digits
    // would only make them LESS directly comparable to the reference matrix and to the WGSL
    // copy. `mul_add` would change the exact rounding this identity test depends on.
    #[allow(
        clippy::many_single_char_names,
        clippy::unreadable_literal,
        clippy::suboptimal_flops
    )]
    fn ntsc_yiq_round_trip_matches_palette_for_every_entry() {
        fn rgb_to_yiq(c: [f32; 3]) -> [f32; 3] {
            let [r, g, b] = c;
            let y = 0.299 * r + 0.587 * g + 0.114 * b;
            let i = 0.595716 * r - 0.274453 * g - 0.321263 * b;
            let q = 0.211456 * r - 0.522591 * g + 0.311135 * b;
            [y, i, q]
        }
        fn yiq_to_rgb(c: [f32; 3]) -> [f32; 3] {
            let [y, i, q] = c;
            let r = y + 0.9563 * i + 0.6210 * q;
            let g = y - 0.2721 * i - 0.6474 * q;
            let b = y - 1.1070 * i + 1.7046 * q;
            [r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0)]
        }
        for &rgb_u32 in crate::palette::Region::Ntsc.table() {
            let r = f32::from(((rgb_u32 >> 16) & 0xFF) as u8) / 255.0;
            let g = f32::from(((rgb_u32 >> 8) & 0xFF) as u8) / 255.0;
            let b = f32::from((rgb_u32 & 0xFF) as u8) / 255.0;
            let original = [r, g, b];
            // A "uniform neighbourhood" decode: every one of the 5 taps is
            // this same colour, so the weighted chroma average (weights sum
            // to 1.0) equals the single tap's own (I, Q), and the centre
            // tap's Y is used unblended — this is EXACTLY what
            // `NTSC_COMPOSITE_WGSL`'s fragment shader computes for a flat
            // colour region.
            let yiq = rgb_to_yiq(original);
            let decoded = yiq_to_rgb(yiq);
            for (c, (&d, &o)) in decoded.iter().zip(original.iter()).enumerate() {
                assert!(
                    (d - o).abs() < 1e-4,
                    "round-trip mismatch at channel {c}: original={original:?} decoded={decoded:?}"
                );
            }
        }
    }

    /// Keeps the WGSL's baked `NTSC_RGB` table (duplicated into
    /// `rusty2600-gfx-shaders` so that `no_std` crate doesn't need a
    /// frontend dependency) honest against `palette::NTSC`'s own values —
    /// catches accidental transcription drift between the two copies.
    #[test]
    fn ntsc_composite_wgsl_table_matches_frontend_palette() {
        let wgsl = PassKind::NtscComposite.wgsl();
        // Scan only the `NTSC_RGB` array literal itself (bounded by its own
        // declaration and closing `);`) — the same fragment shader also has
        // a handful of OTHER `vec3<f32>(...)` call sites with exactly 3
        // numeric literals (the RGB<->YIQ matrix ROWS in `rgb_to_yiq`), which
        // would otherwise be miscounted as extra palette entries.
        let table_start = wgsl
            .find("array<vec3<f32>, 128>(")
            .expect("NTSC_RGB array literal must be present");
        let table_end = table_start
            + wgsl[table_start..]
                .find(");")
                .expect("NTSC_RGB array literal must be closed");
        let table_src = &wgsl[table_start..table_end];
        let mut floats = Vec::new();
        for tok in table_src.split("vec3<f32>(").skip(1) {
            let Some(inner) = tok.split(')').next() else {
                continue;
            };
            let parts: Vec<f32> = inner
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            if parts.len() == 3 {
                floats.push((parts[0], parts[1], parts[2]));
            }
        }
        assert_eq!(
            floats.len(),
            128,
            "expected exactly 128 baked NTSC_RGB entries in the WGSL"
        );
        for (i, &rgb_u32) in crate::palette::Region::Ntsc.table().iter().enumerate() {
            let r = f32::from(((rgb_u32 >> 16) & 0xFF) as u8) / 255.0;
            let g = f32::from(((rgb_u32 >> 8) & 0xFF) as u8) / 255.0;
            let b = f32::from((rgb_u32 & 0xFF) as u8) / 255.0;
            let (wr, wg, wb) = floats[i];
            assert!(
                (wr - r).abs() < 1e-4 && (wg - g).abs() < 1e-4 && (wb - b).abs() < 1e-4,
                "NTSC_RGB[{i}] mismatch: wgsl=({wr},{wg},{wb}) palette=({r},{g},{b})"
            );
        }
    }
}
