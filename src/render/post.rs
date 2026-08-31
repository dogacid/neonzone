//! Phosphor accumulation and tonemap.
//!
//! Frame order:
//!   1. decay pass  -- blit accum[prev] * decay into accum[next]
//!   2. line pass   -- draw this frame's segments additively into accum[next]
//!   3. tonemap     -- accum[next] -> swapchain
//!
//! TODO: bloom belongs between 2 and 3, reading accum[next]. It must run
//! *after* accumulation so the trails bloom too -- bloom the raw frame instead
//! and the smear reads as a separate effect pasted on top.

use bytemuck::{Pod, Zeroable};

use super::line::HDR_FORMAT;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Params {
    decay: f32,
    exposure: f32,
    _pad: [f32; 2],
}

pub struct Post {
    decay_pipeline: wgpu::RenderPipeline,
    tonemap_pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    params: wgpu::Buffer,
    targets: [wgpu::TextureView; 2],
    binds: [wgpu::BindGroup; 2],
    front: usize,
    /// 0.0 = no persistence, 0.9 = long lazy trails. 0.86 is a good start.
    pub decay: f32,
    pub exposure: f32,
}

impl Post {
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post"),
            source: wgpu::ShaderSource::Wgsl(include_str!("post.wgsl").into()),
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post_bgl"),
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
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("post_params"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post_pl"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });

        let make = |entry: &str, format: wgpu::TextureFormat| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("post_pipeline"),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_fullscreen"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(format.into())],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview: None,
                cache: None,
            })
        };

        let (targets, binds) = Self::alloc(device, &layout, &params, width, height);

        Self {
            decay_pipeline: make("fs_decay", HDR_FORMAT),
            tonemap_pipeline: make("fs_tonemap", surface_format),
            layout,
            params,
            targets,
            binds,
            front: 0,
            decay: 0.86,
            exposure: 1.0,
        }
    }

    fn alloc(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        params: &wgpu::Buffer,
        width: u32,
        height: u32,
    ) -> ([wgpu::TextureView; 2], [wgpu::BindGroup; 2]) {
        let mut views = Vec::new();
        let mut binds = Vec::new();
        for i in 0..2 {
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("accum"),
                size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: HDR_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = tex.create_view(&Default::default());
            binds.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("post_bg"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: params.as_entire_binding() },
                ],
            }));
            views.push(view);
            let _ = i;
        }
        (
            [views.remove(0), views.remove(0)],
            [binds.remove(0), binds.remove(0)],
        )
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let (t, b) = Self::alloc(device, &self.layout, &self.params, width, height);
        self.targets = t;
        self.binds = b;
    }

    pub fn upload(&self, queue: &wgpu::Queue) {
        queue.write_buffer(
            &self.params,
            0,
            bytemuck::bytes_of(&Params { decay: self.decay, exposure: self.exposure, _pad: [0.0; 2] }),
        );
    }

    /// The HDR view this frame's lines should be drawn into.
    pub fn scene_view(&self) -> &wgpu::TextureView {
        &self.targets[self.front ^ 1]
    }

    /// Pass 1. Must run before the line pass, into the same attachment.
    pub fn decay_pass(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("decay"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: self.scene_view(),
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.decay_pipeline);
        pass.set_bind_group(0, &self.binds[self.front], &[]);
        pass.draw(0..3, 0..1);
    }

    /// Pass 3. Call after the line pass, then `swap`.
    pub fn tonemap_pass(&self, encoder: &mut wgpu::CommandEncoder, out: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("tonemap"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: out,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.tonemap_pipeline);
        pass.set_bind_group(0, &self.binds[self.front ^ 1], &[]);
        pass.draw(0..3, 0..1);
    }

    pub fn swap(&mut self) {
        self.front ^= 1;
    }
}
