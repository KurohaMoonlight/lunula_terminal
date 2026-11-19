//! Top-level Winit `ApplicationHandler` with `WaitUntil` idle cursor blink pacing.

pub mod help_modal;
pub mod input_keyboard;
pub mod input_mouse;
pub mod render;
pub mod settings_modal;
pub mod state;

pub use state::GpuState;

pub const HELP_MODAL_WIDTH: f32 = 780.0;
pub const HELP_MODAL_HEIGHT: f32 = 660.0;
pub const SETTINGS_MODAL_WIDTH: f32 = 760.0;
pub const SETTINGS_MODAL_HEIGHT: f32 = 520.0;

use crate::config::LunulaConfig;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::window::{WindowAttributes, WindowId};

pub struct LunulaApp {
    pub config: LunulaConfig,
    pub proxy: EventLoopProxy<()>,
    pub state: Option<GpuState>,
}

impl ApplicationHandler for LunulaApp {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.state.is_none() {
            let mon = el
                .primary_monitor()
                .or_else(|| el.available_monitors().next());
            let (mon_w, mon_h) = if let Some(m) = mon.as_ref() {
                (m.size().width, m.size().height)
            } else {
                (1920, 1080)
            };
            let (init_w, init_h) = self.config.window.get_effective_dimensions(mon_w, mon_h);

            let mut attrs = WindowAttributes::default()
                .with_title(&self.config.window.title)
                .with_decorations(false)
                .with_transparent(true)
                .with_inner_size(PhysicalSize::new(init_w, init_h));

            const ICON_RGBA: &[u8] = include_bytes!("../../assets/icon_32.rgba");
            if let Ok(icon) = winit::window::Icon::from_rgba(ICON_RGBA.to_vec(), 32, 32) {
                attrs = attrs.with_window_icon(Some(icon));
            }

            let window = Arc::new(el.create_window(attrs).expect("window"));

            if !self.config.window.lock_size {
                let pos_x = (mon_w.saturating_sub(init_w)) / 2;
                let pos_y = (mon_h.saturating_sub(init_h)) / 2;
                window.set_outer_position(PhysicalPosition::new(pos_x as i32, pos_y as i32));
            }

            let gs = pollster::block_on(GpuState::new(
                window.clone(),
                self.config.clone(),
                self.proxy.clone(),
            ));
            self.state = Some(gs);
            window.request_redraw();
        }
    }

    fn user_event(&mut self, el: &ActiveEventLoop, _event: ()) {
        if let Some(s) = self.state.as_mut() {
            if !s.drain_pty() || s.should_exit {
                el.exit();
                return;
            }
            s.window.request_redraw();
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let Some(s) = self.state.as_mut() else {
            el.set_control_flow(ControlFlow::Wait);
            return;
        };

        if s.should_exit {
            el.exit();
            return;
        }

        if s.high_fps_animating {
            el.set_control_flow(ControlFlow::Poll);
        } else if let Some(deadline) = s.next_blink_frame {
            if Instant::now() >= deadline {
                s.next_blink_frame = None;
                s.window.request_redraw();
            } else {
                el.set_control_flow(ControlFlow::WaitUntil(deadline));
            }
        } else {
            el.set_control_flow(ControlFlow::Wait);
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _wid: WindowId, event: WindowEvent) {
        let Some(s) = self.state.as_mut() else { return };

        if s.should_exit {
            el.exit();
            return;
        }

        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Focused(focused) => {
                s.is_focused = focused;
                s.window.request_redraw();
            }
            WindowEvent::Resized(sz) => {
                s.resize(sz);
                s.window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                s.resize(s.window.inner_size());
                s.window.request_redraw();
            }
            WindowEvent::Ime(ime_event) => match ime_event {
                winit::event::Ime::Commit(text) => {
                    if let Some(pane) = s.tab_manager.active_pane_mut() {
                        pane.terminal.clear_selection();
                        pane.pty.write_bytes(text.as_bytes());
                        pane.terminal.scroll_to_bottom();
                        if !s.drain_pty() || s.should_exit {
                            el.exit();
                            return;
                        }
                        s.window.request_redraw();
                    }
                }
                winit::event::Ime::Preedit(_, _) => {
                    s.window.request_redraw();
                }
                _ => {}
            },
            WindowEvent::MouseWheel { delta, .. } => {
                s.handle_mouse_wheel(delta);
            }
            WindowEvent::ModifiersChanged(mods) => {
                s.ctrl_pressed = mods.state().control_key();
                s.shift_pressed = mods.state().shift_key();
                s.alt_pressed = mods.state().alt_key();
                s.super_pressed = mods.state().super_key();
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                s.handle_keyboard_input(el, key_event);
            }
            WindowEvent::CursorMoved { position, .. } => {
                s.handle_cursor_moved(position);
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: bs,
                ..
            } => {
                s.handle_mouse_left(el, bs);
            }
            WindowEvent::MouseInput {
                button: MouseButton::Right,
                state: ElementState::Pressed,
                ..
            } => {
                s.handle_mouse_right();
            }
            WindowEvent::RedrawRequested => {
                if !s.render() || s.should_exit {
                    el.exit();
                    return;
                }
            }
            _ => {}
        }
    }
}
