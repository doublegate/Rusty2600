//! The composable post-process shader stack.
//!
//! Chains 0-2 full-screen passes (`rusty2600-gfx-shaders`'
//! [`rusty2600_gfx_shaders::PassKind`] sources) after the base framebuffer
//! blit.
//!
//! An empty pass list is the zero-cost default: [`crate::gfx::Gfx::present`]
//! skips this module entirely and calls the existing direct blit, so a plain build's
//! output stays byte-identical whether a user has ever opened the Settings
//! shader picklist or not (the same invariant `uv_scale` preserved when it
//! landed in `[1.1.0]`).
//!
//! Fixed at a 2-slot ping-pong (not an arbitrary-`N` chain) since exactly
//! two passes exist today (see [`rusty2600_gfx_shaders::PassKind`]) —
//! extending this to more passes would need a third intermediate texture,
//! not just a longer loop, so it's left for whenever a third pass actually
//! ships rather than built speculatively now.

use rusty2600_gfx_shaders::PassKind;

/// One full-screen-triangle post-process pipeline, paired with the bind
/// group that samples whichever intermediate texture feeds it.
struct Pass {
    pipeline: wgpu::RenderPipeline,
}

/// The 2-slot ping-pong shader stack.
pub struct ShaderStack {
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    composite_artifact: Pass,
    crt_scanline: Pass,
    tex_a: wgpu::Texture,
    tex_b: wgpu::Texture,
    bind_group_a: wgpu::BindGroup,
    bind_group_b: wgpu::BindGroup,
    width: u32,
    height: u32,
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
    /// Builds the stack's pipelines + intermediate textures, sized to the
    /// surface's current `(width, height)`.
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
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
        let tex_a = make_intermediate(device, format, width, height);
        let tex_b = make_intermediate(device, format, width, height);
        let bind_group_a = make_bind_group(device, &bind_group_layout, &sampler, &tex_a);
        let bind_group_b = make_bind_group(device, &bind_group_layout, &sampler, &tex_b);
        Self {
            bind_group_layout,
            sampler,
            composite_artifact,
            crt_scanline,
            tex_a,
            tex_b,
            bind_group_a,
            bind_group_b,
            width,
            height,
        }
    }

    /// Recreates the intermediate textures for a new surface size. A no-op
    /// if the size hasn't actually changed.
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
    /// non-empty (the stack's first ping-pong slot).
    #[must_use]
    pub fn first_target_view(&self) -> wgpu::TextureView {
        self.tex_a
            .create_view(&wgpu::TextureViewDescriptor::default())
    }

    const fn pass(&self, kind: PassKind) -> &Pass {
        match kind {
            PassKind::CompositeArtifact => &self.composite_artifact,
            PassKind::CrtScanline => &self.crt_scanline,
        }
    }

    /// Runs `passes` in order, reading the base blit's output (already
    /// written to [`Self::first_target_view`]) and writing the final pass's
    /// output to `target` (the swapchain view).
    ///
    /// # Panics
    ///
    /// Panics if `passes.len() > 2` — see the module doc for why this is a
    /// fixed 2-slot stack, not an arbitrary chain.
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        passes: &[PassKind],
    ) {
        assert!(
            passes.len() <= 2,
            "ShaderStack supports at most 2 chained passes"
        );
        if passes.is_empty() {
            return;
        }
        let final_target = if passes.len() == 1 {
            target
        } else {
            &self
                .tex_b
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        self.run_pass(encoder, passes[0], &self.bind_group_a, final_target);
        if passes.len() == 2 {
            self.run_pass(encoder, passes[1], &self.bind_group_b, target);
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

    #[test]
    fn composite_artifact_wgsl_validates() {
        let module = naga::front::wgsl::parse_str(PassKind::CompositeArtifact.wgsl())
            .expect("composite-artifact WGSL parses");
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        );
        validator
            .validate(&module)
            .expect("composite-artifact WGSL validates");
    }

    #[test]
    fn crt_scanline_wgsl_validates() {
        let module = naga::front::wgsl::parse_str(PassKind::CrtScanline.wgsl())
            .expect("crt-scanline WGSL parses");
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        );
        validator
            .validate(&module)
            .expect("crt-scanline WGSL validates");
    }
}
