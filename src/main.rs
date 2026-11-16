mod wobble;

use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};
use wobble::{WindowControlAction, WobbleWindow};

#[repr(C)]
struct POINT {
    x: i32,
    y: i32,
}
unsafe extern "system" {
    fn GetCursorPos(lpPoint: *mut POINT) -> i32;
}

fn get_screen_cursor() -> (i32, i32) {
    let mut pt = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut pt) };
    (pt.x, pt.y)
}

struct LunulaApp {
    state: Option<GpuState>,
}

struct GpuState {
    window: Arc<Window>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    bg_color: wgpu::Color,
    wobble_window: WobbleWindow,

    // OS Desktop Drag State
    is_dragging_os: bool,
    drag_start_cursor: (i32, i32),
    drag_start_win_pos: PhysicalPosition<i32>,
    last_cursor_pos: (i32, i32),
    last_cursor_time: Instant,
    mouse_pos: [f32; 2],
}

impl GpuState {
    async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::default();

        let surface = instance
            .create_surface(window.clone())
            .expect("Failed to create surface");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .expect("Failed to find suitable GPU adapter");

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Lunula Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: Default::default(),
                memory_hints: Default::default(),
                trace: Default::default(),
            })
            .await
            .expect("Failed to create wgpu device");

        let width = size.width.max(1);
        let height = size.height.max(1);
        let mut surface_config = surface
            .get_default_config(&adapter, width, height)
            .expect("Surface not supported by adapter");

        let caps = surface.get_capabilities(&adapter);
        if let Some(format) = caps.formats.iter().copied().find(|f| f.is_srgb()) {
            surface_config.format = format;
        }

        // Enable alpha transparency composite mode
        let alpha_mode = if caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else if caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PostMultiplied)
        {
            wgpu::CompositeAlphaMode::PostMultiplied
        } else {
            caps.alpha_modes[0]
        };
        surface_config.alpha_mode = alpha_mode;
        surface_config.present_mode = wgpu::PresentMode::AutoVsync;

        surface.configure(&device, &surface_config);

        let wobble_window =
            WobbleWindow::new(&device, surface_config.format, [width as f32, height as f32]);

        Self {
            window,
            device,
            queue,
            surface,
            surface_config,
            // 100% transparent clear color outside the wobbly terminal
            bg_color: wgpu::Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            },
            wobble_window,
            is_dragging_os: false,
            drag_start_cursor: (0, 0),
            drag_start_win_pos: PhysicalPosition::new(0, 0),
            last_cursor_pos: (0, 0),
            last_cursor_time: Instant::now(),
            mouse_pos: [0.0, 0.0],
        }
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.surface_config.width = new_size.width;
            self.surface_config.height = new_size.height;
            self.surface.configure(&self.device, &self.surface_config);
            self.wobble_window
                .resize(new_size.width as f32, new_size.height as f32);
        }
    }

    fn render(&mut self) {
        self.wobble_window.update_physics();

        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(tex)
            | wgpu::CurrentSurfaceTexture::Suboptimal(tex) => tex,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                let size = self.window.inner_size();
                self.resize(size);
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
            wgpu::CurrentSurfaceTexture::Validation => {
                log::error!("Surface validation error");
                return;
            }
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        let screen_size = [
            self.surface_config.width as f32,
            self.surface_config.height as f32,
        ];

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.bg_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            self.wobble_window
                .render(&self.queue, &mut render_pass, screen_size);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        self.queue.present(output);
    }
}

impl ApplicationHandler for LunulaApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_none() {
            let win_attrs = WindowAttributes::default()
                .with_title("Lunula Terminal")
                .with_decorations(false)
                .with_transparent(true)
                .with_inner_size(PhysicalSize::new(980, 640));

            let window = Arc::new(
                event_loop
                    .create_window(win_attrs)
                    .expect("Failed to create window"),
            );
            let gpu_state = pollster::block_on(GpuState::new(window.clone()));
            self.state = Some(gpu_state);
            window.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else { return };

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                state.resize(new_size);
                state.window.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                state.mouse_pos = [position.x as f32, position.y as f32];

                if state.is_dragging_os {
                    let (cx, cy) = get_screen_cursor();
                    let dx = cx - state.drag_start_cursor.0;
                    let dy = cy - state.drag_start_cursor.1;

                    let new_win_pos = PhysicalPosition::new(
                        state.drag_start_win_pos.x + dx,
                        state.drag_start_win_pos.y + dy,
                    );
                    state.window.set_outer_position(new_win_pos);

                    let now = Instant::now();
                    let dt = (now - state.last_cursor_time).as_secs_f32().max(0.001);
                    let vel_x = (cx - state.last_cursor_pos.0) as f32 / dt;
                    let vel_y = (cy - state.last_cursor_pos.1) as f32 / dt;

                    state.last_cursor_pos = (cx, cy);
                    state.last_cursor_time = now;

                    state
                        .wobble_window
                        .inject_desktop_drag_velocity(vel_x, vel_y);
                }

                state.window.request_redraw();
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: button_state,
                ..
            } => {
                match button_state {
                    ElementState::Pressed => {
                        let action = state
                            .wobble_window
                            .handle_mouse_click(state.mouse_pos[0], state.mouse_pos[1]);

                        match action {
                            WindowControlAction::Close => {
                                event_loop.exit();
                            }
                            WindowControlAction::Minimize => {
                                state.window.set_minimized(true);
                            }
                            WindowControlAction::Maximize => {
                                state.window.set_maximized(!state.window.is_maximized());
                            }
                            WindowControlAction::Drag => {
                                state.is_dragging_os = true;
                                state.drag_start_cursor = get_screen_cursor();
                                state.drag_start_win_pos =
                                    state.window.outer_position().unwrap_or_default();
                                state.last_cursor_pos = state.drag_start_cursor;
                                state.last_cursor_time = Instant::now();
                            }
                            WindowControlAction::None => {}
                        }
                    }
                    ElementState::Released => {
                        state.is_dragging_os = false;
                        state.wobble_window.handle_mouse_up();
                    }
                }
                state.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                state.render();
                state.window.request_redraw();
            }
            _ => {}
        }
    }
}

fn main() {
    env_logger::init();

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = LunulaApp { state: None };
    event_loop.run_app(&mut app).expect("Error running event loop");
}
