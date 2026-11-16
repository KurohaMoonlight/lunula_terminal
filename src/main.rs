mod pty;
mod terminal;
mod text;
mod wobble;

use pty::PtyManager;
use std::sync::Arc;
use terminal::TerminalGrid;
use text::TextEngine;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowAttributes, WindowId};
use wobble::{WindowControlAction, WobbleWindow};

#[repr(C)]
struct POINT {
    x: i32,
    y: i32,
}
unsafe extern "system" {
    fn GetCursorPos(lp: *mut POINT) -> i32;
}

fn screen_cursor() -> (i32, i32) {
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
    wobble: WobbleWindow,
    text_engine: TextEngine,
    pty: PtyManager,
    terminal: TerminalGrid,

    ctrl_pressed: bool,
    shift_pressed: bool,
    alt_pressed: bool,
    dragging: bool,
    drag_start_cursor: (i32, i32),
    drag_start_win: PhysicalPosition<i32>,
    last_win_pos: PhysicalPosition<i32>,
    mouse_pos: [f32; 2],
}

impl GpuState {
    async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).expect("surface");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .expect("adapter");

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Lunula"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: Default::default(),
                memory_hints: Default::default(),
                trace: Default::default(),
            })
            .await
            .expect("device");

        let w = size.width.max(1);
        let h = size.height.max(1);
        let mut sc = surface.get_default_config(&adapter, w, h).expect("config");
        let caps = surface.get_capabilities(&adapter);
        if let Some(f) = caps.formats.iter().copied().find(|f| f.is_srgb()) {
            sc.format = f;
        }

        if caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            sc.alpha_mode = wgpu::CompositeAlphaMode::PreMultiplied;
        } else if caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PostMultiplied)
        {
            sc.alpha_mode = wgpu::CompositeAlphaMode::PostMultiplied;
        }
        sc.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &sc);

        let wobble = WobbleWindow::new(&device, sc.format, [w as f32, h as f32]);
        let text_engine = TextEngine::new(
            &device,
            &queue,
            sc.format,
            [wobble.content_size[0] as f32, wobble.content_size[1] as f32],
        );

        // Compute Grid Dimensions based on exact Font Cell Metrics
        let content_w = wobble.content_size[0] as f32 - 32.0; // Margin padding
        let content_h = wobble.content_size[1] as f32 - 32.0;
        let cols = (content_w / text_engine.cell_width).max(10.0) as u16;
        let rows = (content_h / text_engine.cell_height).max(5.0) as u16;

        let terminal = TerminalGrid::new(cols as usize, rows as usize);
        let pty = PtyManager::new(cols, rows).expect("Failed to initialize ConPTY PowerShell");

        let win_pos = window.outer_position().unwrap_or_default();

        Self {
            window,
            device,
            queue,
            surface,
            surface_config: sc,
            wobble,
            text_engine,
            pty,
            terminal,
            ctrl_pressed: false,
            shift_pressed: false,
            alt_pressed: false,
            dragging: false,
            drag_start_cursor: (0, 0),
            drag_start_win: PhysicalPosition::new(0, 0),
            last_win_pos: win_pos,
            mouse_pos: [0.0, 0.0],
        }
    }

    fn resize(&mut self, s: PhysicalSize<u32>) {
        if s.width > 0 && s.height > 0 {
            self.surface_config.width = s.width;
            self.surface_config.height = s.height;
            self.surface.configure(&self.device, &self.surface_config);

            self.wobble
                .resize(&self.device, s.width as f32, s.height as f32);
            self.text_engine.resize(
                &self.queue,
                self.wobble.content_size[0] as f32,
                self.wobble.content_size[1] as f32,
            );

            // Resize PTY and Terminal Grid
            let content_w = self.wobble.content_size[0] as f32 - 32.0;
            let content_h = self.wobble.content_size[1] as f32 - 32.0;
            let cols = (content_w / self.text_engine.cell_width).max(10.0) as u16;
            let rows = (content_h / self.text_engine.cell_height).max(5.0) as u16;

            self.terminal.resize(cols as usize, rows as usize);
            self.pty.resize(cols, rows);
        }
    }

    fn drain_pty(&mut self) {
        let mut received = false;
        while let Ok(bytes) = self.pty.rx.try_recv() {
            self.terminal.advance_bytes(&bytes);
            received = true;
        }

        if !self.terminal.responses.is_empty() {
            for resp in self.terminal.responses.drain(..) {
                self.pty.write_bytes(&resp);
            }
        }

        if received || self.terminal.dirty {
            let rendered_text = self.terminal.render_to_string();
            self.text_engine.set_text(&rendered_text);
            self.terminal.dirty = false;
        }
    }

    fn render(&mut self) {
        self.wobble.update_physics();
        self.drain_pty();

        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
                t
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.resize(self.window.inner_size());
                return;
            }
            _ => return,
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let ss = [
            self.surface_config.width as f32,
            self.surface_config.height as f32,
        ];

        // 1. Prepare Text inside offscreen texture
        self.text_engine.prepare(&self.device, &self.queue, 16.0, 16.0);

        // 2. PASS 1: Render Text to Offscreen Content Texture
        {
            let mut text_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Offscreen Text Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.wobble.content_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.10,
                            g: 0.10,
                            b: 0.15,
                            a: 0.95,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            self.text_engine.render(&mut text_pass);
        }

        // 3. PASS 2: Render Wobbly Window Mesh with Texture Mapping
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main Swapchain Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            self.wobble.render(&self.queue, &mut rp, ss);
        }

        self.queue.submit(std::iter::once(enc.finish()));
        self.queue.present(output);
    }
}

impl ApplicationHandler for LunulaApp {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.state.is_none() {
            let attrs = WindowAttributes::default()
                .with_title("Lunula Terminal")
                .with_decorations(false)
                .with_transparent(true)
                .with_inner_size(PhysicalSize::new(980, 640));

            let window = Arc::new(el.create_window(attrs).expect("window"));
            let gs = pollster::block_on(GpuState::new(window.clone()));
            self.state = Some(gs);
            window.request_redraw();
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _wid: WindowId, event: WindowEvent) {
        let Some(s) = self.state.as_mut() else { return };

        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(sz) => {
                s.resize(sz);
                s.window.request_redraw();
            }

            WindowEvent::ModifiersChanged(mods) => {
                s.ctrl_pressed = mods.state().control_key();
                s.shift_pressed = mods.state().shift_key();
                s.alt_pressed = mods.state().alt_key();
            }

            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                if key_event.state.is_pressed() {
                    let mut ctrl_state = 0u32;
                    if s.alt_pressed {
                        ctrl_state |= 0x0002;
                    }
                    if s.ctrl_pressed {
                        ctrl_state |= 0x0008;
                    }
                    if s.shift_pressed {
                        ctrl_state |= 0x0010;
                    }

                    match &key_event.logical_key {
                        Key::Named(NamedKey::Space) => {
                            s.pty.write_bytes(b" ");
                        }
                        Key::Named(NamedKey::Enter) => {
                            s.pty.write_bytes(b"\r");
                        }
                        Key::Named(NamedKey::Backspace) => {
                            if s.ctrl_pressed {
                                s.pty.write_bytes(b"\x08");
                            } else {
                                s.pty.write_bytes(b"\x7f");
                            }
                        }
                        Key::Named(NamedKey::Tab) => {
                            s.pty.write_bytes(b"\t");
                        }
                        Key::Named(NamedKey::Escape) => {
                            s.pty.write_bytes(b"\x1b");
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            if s.terminal.win32_input_mode {
                                s.pty.write_win32_key(0x26, 72, 0, ctrl_state | 0x0100);
                            } else {
                                s.pty.write_bytes(b"\x1b[A");
                            }
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            if s.terminal.win32_input_mode {
                                s.pty.write_win32_key(0x28, 80, 0, ctrl_state | 0x0100);
                            } else {
                                s.pty.write_bytes(b"\x1b[B");
                            }
                        }
                        Key::Named(NamedKey::ArrowRight) => {
                            if s.terminal.win32_input_mode {
                                s.pty.write_win32_key(0x27, 77, 0, ctrl_state | 0x0100);
                            } else {
                                s.pty.write_bytes(b"\x1b[C");
                            }
                        }
                        Key::Named(NamedKey::ArrowLeft) => {
                            if s.terminal.win32_input_mode {
                                s.pty.write_win32_key(0x25, 75, 0, ctrl_state | 0x0100);
                            } else {
                                s.pty.write_bytes(b"\x1b[D");
                            }
                        }
                        Key::Named(NamedKey::Home) => {
                            if s.terminal.win32_input_mode {
                                s.pty.write_win32_key(0x24, 71, 0, ctrl_state | 0x0100);
                            } else {
                                s.pty.write_bytes(b"\x1b[H");
                            }
                        }
                        Key::Named(NamedKey::End) => {
                            if s.terminal.win32_input_mode {
                                s.pty.write_win32_key(0x23, 79, 0, ctrl_state | 0x0100);
                            } else {
                                s.pty.write_bytes(b"\x1b[F");
                            }
                        }
                        Key::Named(NamedKey::Delete) => {
                            if s.terminal.win32_input_mode {
                                s.pty.write_win32_key(0x2E, 83, 0, ctrl_state | 0x0100);
                            } else {
                                s.pty.write_bytes(b"\x1b[3~");
                            }
                        }
                        Key::Named(NamedKey::PageUp) => {
                            if s.terminal.win32_input_mode {
                                s.pty.write_win32_key(0x21, 73, 0, ctrl_state | 0x0100);
                            } else {
                                s.pty.write_bytes(b"\x1b[5~");
                            }
                        }
                        Key::Named(NamedKey::PageDown) => {
                            if s.terminal.win32_input_mode {
                                s.pty.write_win32_key(0x22, 81, 0, ctrl_state | 0x0100);
                            } else {
                                s.pty.write_bytes(b"\x1b[6~");
                            }
                        }
                        Key::Named(NamedKey::Insert) => {
                            if s.terminal.win32_input_mode {
                                s.pty.write_win32_key(0x2D, 82, 0, ctrl_state | 0x0100);
                            } else {
                                s.pty.write_bytes(b"\x1b[2~");
                            }
                        }
                        Key::Character(text_str) => {
                            if s.ctrl_pressed {
                                for ch in text_str.chars() {
                                    match ch {
                                        'c' | 'C' | '\x03' => s.pty.write_bytes(b"\x03"),
                                        'v' | 'V' | '\x16' => s.pty.write_bytes(b"\x16"),
                                        'l' | 'L' | '\x0c' => s.pty.write_bytes(b"\x0c"),
                                        'd' | 'D' | '\x04' => s.pty.write_bytes(b"\x04"),
                                        'a' | 'A' | '\x01' => s.pty.write_bytes(b"\x01"),
                                        'z' | 'Z' | '\x1a' => s.pty.write_bytes(b"\x1a"),
                                        _ => {
                                            if ch.is_ascii_alphabetic() {
                                                let b = (ch.to_ascii_uppercase() as u8 - b'A') + 1;
                                                s.pty.write_bytes(&[b]);
                                            }
                                        }
                                    }
                                }
                            } else {
                                s.pty.write_bytes(text_str.as_bytes());
                            }
                        }
                        _ => {}
                    }
                    s.drain_pty();
                    s.window.request_redraw();
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                s.mouse_pos = [position.x as f32, position.y as f32];

                if s.dragging {
                    let (cx, cy) = screen_cursor();
                    let dx = cx - s.drag_start_cursor.0;
                    let dy = cy - s.drag_start_cursor.1;
                    let new_pos = PhysicalPosition::new(
                        s.drag_start_win.x + dx,
                        s.drag_start_win.y + dy,
                    );
                    s.window.set_outer_position(new_pos);

                    let moved_x = (new_pos.x - s.last_win_pos.x) as f32;
                    let moved_y = (new_pos.y - s.last_win_pos.y) as f32;
                    s.wobble.notify_window_moved(moved_x, moved_y);
                    s.last_win_pos = new_pos;
                }
                s.window.request_redraw();
            }

            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: bs,
                ..
            } => {
                match bs {
                    ElementState::Pressed => {
                        let action = s.wobble.handle_mouse_click(s.mouse_pos[0], s.mouse_pos[1]);
                        match action {
                            WindowControlAction::Close => el.exit(),
                            WindowControlAction::Minimize => s.window.set_minimized(true),
                            WindowControlAction::Maximize => {
                                s.window.set_maximized(!s.window.is_maximized())
                            }
                            WindowControlAction::Drag => {
                                s.dragging = true;
                                s.drag_start_cursor = screen_cursor();
                                s.drag_start_win = s.window.outer_position().unwrap_or_default();
                                s.last_win_pos = s.drag_start_win;
                            }
                            WindowControlAction::None => {}
                        }
                    }
                    ElementState::Released => {
                        s.dragging = false;
                        s.wobble.handle_mouse_up();
                    }
                }
                s.window.request_redraw();
            }

            WindowEvent::RedrawRequested => {
                s.render();
                s.window.request_redraw();
            }
            _ => {}
        }
    }
}

fn main() {
    env_logger::init();
    let el = EventLoop::new().expect("event loop");
    el.set_control_flow(ControlFlow::Poll);
    let mut app = LunulaApp { state: None };
    el.run_app(&mut app).expect("run");
}
