mod camera;
mod render;
mod theme;
mod world;

use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Instant;

use camera::{Camera, Input};
use render::line::{Globals, LineBatch, LineRenderer};
use render::post::Post;
use theme::Palette;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
}

struct App {
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    lines: Option<LineRenderer>,
    post: Option<Post>,
    batch: LineBatch,
    camera: Camera,
    world: world::World,
    palette: Palette,
    theme_rx: Option<Receiver<Palette>>,
    input: Input,
    last: Instant,
}

impl App {
    fn new() -> Self {
        let palette = theme::load().unwrap_or_else(|e| {
            eprintln!("using fallback palette: {e}");
            Palette::default()
        });
        Self {
            window: None,
            gpu: None,
            lines: None,
            post: None,
            batch: LineBatch::default(),
            camera: Camera::default(),
            world: world::World::default(),
            palette,
            theme_rx: theme::watch(),
            input: Input::default(),
            last: Instant::now(),
        }
    }

    fn init_gpu(&mut self, window: Arc<Window>) {
        let size = window.inner_size();
        // GL is excluded on purpose: on this NVIDIA/Wayland stack, wgpu's GLES
        // backend segfaults during teardown inside libnvidia-egl-wayland's
        // wl_proxy_marshal_flags. Vulkan doesn't have that bug and is what we
        // want anyway.
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .unwrap();

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("neonzone"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: Default::default(),
            trace: wgpu::Trace::Off,
        }))
        .unwrap();

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        self.lines = Some(LineRenderer::new(&device, 16_384));
        self.post = Some(Post::new(&device, format, config.width, config.height));
        self.gpu = Some(Gpu { surface, device, queue, config });
        self.window = Some(window);
    }

    fn resize(&mut self, width: u32, height: u32) {
        let Some(gpu) = self.gpu.as_mut() else { return };
        if width == 0 || height == 0 {
            return;
        }
        gpu.config.width = width;
        gpu.config.height = height;
        gpu.surface.configure(&gpu.device, &gpu.config);
        if let Some(post) = self.post.as_mut() {
            post.resize(&gpu.device, width, height);
        }
    }

    fn frame(&mut self) {
        let (Some(gpu), Some(lines), Some(post)) =
            (self.gpu.as_ref(), self.lines.as_mut(), self.post.as_mut())
        else {
            return;
        };

        if let Some(rx) = &self.theme_rx {
            while let Ok(p) = rx.try_recv() {
                // TODO: cross-fade over ~250ms instead of snapping.
                self.palette = p;
            }
        }

        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.1);
        self.last = now;
        self.camera.update(self.input, dt);

        self.world.build(&mut self.batch, &self.palette, self.camera.pos);

        let aspect = gpu.config.width as f32 / gpu.config.height as f32;
        let globals = Globals {
            view: self.camera.view().to_cols_array_2d(),
            proj: self.camera.proj(aspect).to_cols_array_2d(),
            viewport: [gpu.config.width as f32, gpu.config.height as f32],
            near: self.camera.near,
            fade_near: 60.0,
            fade_far: 620.0,
            fade_floor: 0.15,
            dwell_gain: 0.6,
            _pad: 0.0,
        };
        lines.upload(&gpu.device, &gpu.queue, &globals, &self.batch);
        post.upload(&gpu.queue);

        let Ok(frame) = gpu.surface.get_current_texture() else { return };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = gpu.device.create_command_encoder(&Default::default());

        post.decay_pass(&mut encoder);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("lines"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: post.scene_view(),
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            lines.draw(&mut pass);
        }
        post.tonemap_pass(&mut encoder, &view);
        post.swap();

        gpu.queue.submit([encoder.finish()]);
        frame.present();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes().with_title("neonzone");
        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        self.init_gpu(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => self.resize(size.width, size.height),
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                let v = if pressed { 1.0 } else { 0.0 };
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::Escape) if pressed => event_loop.exit(),
                    PhysicalKey::Code(KeyCode::KeyW) | PhysicalKey::Code(KeyCode::ArrowUp) => {
                        self.input.forward = v
                    }
                    PhysicalKey::Code(KeyCode::KeyS) | PhysicalKey::Code(KeyCode::ArrowDown) => {
                        self.input.forward = -v
                    }
                    PhysicalKey::Code(KeyCode::KeyA) | PhysicalKey::Code(KeyCode::ArrowLeft) => {
                        self.input.turn = -v
                    }
                    PhysicalKey::Code(KeyCode::KeyD) | PhysicalKey::Code(KeyCode::ArrowRight) => {
                        self.input.turn = v
                    }
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                self.frame();
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn main() -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut App::new())?;
    Ok(())
}
