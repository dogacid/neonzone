//! Line renderer for a vector-arcade look. Targets wgpu 25.
//!
//! Everything visible in the game is a segment pushed into a `LineBatch`,
//! uploaded once per frame and drawn as instanced screen-space quads into an
//! Rgba16Float target. Bloom and the phosphor accumulation pass read that
//! target afterwards.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Globals {
    pub view: [[f32; 4]; 4],
    pub proj: [[f32; 4]; 4],
    pub viewport: [f32; 2],
    pub near: f32,
    pub fade_near: f32,
    pub fade_far: f32,
    pub fade_floor: f32,
    pub dwell_gain: f32,
    pub _pad: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Pod, Zeroable)]
pub struct LineInstance {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub color: [f32; 3],
    /// x = width in pixels, y = intensity. Intensity above 1.0 is the whole
    /// point -- that is what the bloom threshold picks up.
    pub params: [f32; 2],
}

const INSTANCE_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
    array_stride: std::mem::size_of::<LineInstance>() as wgpu::BufferAddress,
    step_mode: wgpu::VertexStepMode::Instance,
    attributes: &wgpu::vertex_attr_array![
        0 => Float32x3,  // a
        1 => Float32x3,  // b
        2 => Float32x3,  // color
        3 => Float32x2   // params
    ],
};

/// Per-frame CPU-side accumulator. Clear it, fill it, upload it.
#[derive(Default)]
pub struct LineBatch {
    pub lines: Vec<LineInstance>,
}

impl LineBatch {
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    pub fn segment(&mut self, a: [f32; 3], b: [f32; 3], color: [f32; 3], width: f32, intensity: f32) {
        self.lines.push(LineInstance { a, b, color, params: [width, intensity] });
    }

    pub fn polyline(&mut self, pts: &[[f32; 3]], color: [f32; 3], width: f32, intensity: f32) {
        for w in pts.windows(2) {
            self.segment(w[0], w[1], color, width, intensity);
        }
    }

    /// Axis-aligned box from min/max corners. The workhorse for tanks, crates
    /// and anything else you can fake with a cuboid.
    pub fn cuboid(&mut self, min: [f32; 3], max: [f32; 3], color: [f32; 3], width: f32, intensity: f32) {
        let c = |i: usize| -> [f32; 3] {
            [
                if i & 1 == 0 { min[0] } else { max[0] },
                if i & 2 == 0 { min[1] } else { max[1] },
                if i & 4 == 0 { min[2] } else { max[2] },
            ]
        };
        const EDGES: [(usize, usize); 12] = [
            (0, 1), (1, 3), (3, 2), (2, 0),
            (4, 5), (5, 7), (7, 6), (6, 4),
            (0, 4), (1, 5), (2, 6), (3, 7),
        ];
        for (i, j) in EDGES {
            self.segment(c(i), c(j), color, width, intensity);
        }
    }

    /// Perspective ground grid. Lines are emitted in world space and the
    /// vertex shader's near-plane clip handles the ones behind the camera, so
    /// you can just re-centre the grid on the player each frame and let it
    /// scroll forever.
    pub fn ground_grid(&mut self, center: [f32; 3], spacing: f32, half_count: i32, color: [f32; 3], width: f32, intensity: f32) {
        let snap = |v: f32| (v / spacing).round() * spacing;
        let (cx, cz) = (snap(center[0]), snap(center[2]));
        let reach = spacing * half_count as f32;
        for i in -half_count..=half_count {
            let o = i as f32 * spacing;
            self.segment([cx + o, 0.0, cz - reach], [cx + o, 0.0, cz + reach], color, width, intensity);
            self.segment([cx - reach, 0.0, cz + o], [cx + reach, 0.0, cz + o], color, width, intensity);
        }
    }
}

pub struct LineRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    globals: wgpu::Buffer,
    instances: wgpu::Buffer,
    capacity: usize,
    count: u32,
}

impl LineRenderer {
    pub fn new(device: &wgpu::Device, capacity: usize) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("line_glow"),
            source: wgpu::ShaderSource::Wgsl(include_str!("line_glow.wgsl").into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("line_globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("line_globals_buf"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("line_globals_bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("line_pipeline_layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("line_pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[INSTANCE_LAYOUT],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
                    // Pure additive. Overlapping strokes reinforce instead of
                    // occluding, which is exactly how a vector monitor behaves
                    // and is why no depth buffer appears anywhere below.
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("line_instances"),
            size: (capacity * std::mem::size_of::<LineInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self { pipeline, bind_group, globals, instances, capacity, count: 0 }
    }

    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, globals: &Globals, batch: &LineBatch) {
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(globals));

        if batch.lines.len() > self.capacity {
            self.capacity = batch.lines.len().next_power_of_two();
            self.instances = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("line_instances"),
                contents: bytemuck::cast_slice(&batch.lines),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
        } else if !batch.lines.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&batch.lines));
        }
        self.count = batch.lines.len() as u32;
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        pass.draw(0..6, 0..self.count);
    }
}
