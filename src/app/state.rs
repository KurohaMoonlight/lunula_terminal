//! Core GPU and application state (`GpuState`), window resizing, and live Lua hot-reload.

use super::help_modal::build_help_buffer;
use crate::config::LunulaConfig;
use crate::pane;
use crate::platform::get_git_branch;
use crate::render::TextEngine;
use crate::terminal;
use crate::wobble::WobbleWindow;
use std::sync::Arc;
use std::time::Instant;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event_loop::EventLoopProxy;
use winit::window::Window;

pub struct GpuState {
    pub window: Arc<Window>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub wobble: WobbleWindow,
    pub text_engine: TextEngine,
    pub tab_manager: pane::TabManager,
    pub config: LunulaConfig,
    pub config_path: std::path::PathBuf,
    pub last_config_check: Instant,
    pub last_config_modified: Option<std::time::SystemTime>,
    pub active_window_title: String,

    pub is_maximized: bool,
    pub is_focused: bool,
    pub saved_win_pos: PhysicalPosition<i32>,
    pub saved_win_size: PhysicalSize<u32>,

    pub ctrl_pressed: bool,
    pub shift_pressed: bool,
    pub alt_pressed: bool,
    pub dragging: bool,
    pub mouse_left_down: bool,
    pub last_drag_cell: Option<(usize, usize)>,
    pub selecting: bool,
    pub help_visible: bool,
    pub help_buffer: glyphon::Buffer,
    pub vi_status_buffer: glyphon::Buffer,
    pub help_scroll_y: f32,
    pub help_total_height: f32,
    pub help_dragging_scrollbar: bool,

    pub bar_center_buffer: glyphon::Buffer,
    pub bar_clock_buffer: glyphon::Buffer,
    pub bar_gear_buffer: glyphon::Buffer,
    pub bar_help_buffer: glyphon::Buffer,
    pub bar_center_width: f32,
    pub bar_clock_width: f32,
    pub bar_gear_width: f32,
    pub bar_help_width: f32,
    pub bar_icons_dirty: bool,
    pub cached_username: String,
    pub cached_git_branch: String,
    pub bar_center_dirty: bool,
    pub last_git_check: Instant,
    pub last_time_str: String,

    pub drag_start_cursor: (i32, i32),
    pub drag_start_win: PhysicalPosition<i32>,
    pub last_win_pos: PhysicalPosition<i32>,
    pub last_cursor_pos: (i32, i32),
    pub last_cursor_time: Instant,
    pub mouse_pos: [f32; 2],
    pub last_click_time: Instant,
    pub last_click_cell: (usize, usize),
    pub click_streak: usize,

    pub last_frame_time: Instant,
    pub high_fps_animating: bool,
    pub next_blink_frame: Option<Instant>,
    pub initial_trim_frames: u8,
}

impl GpuState {
    pub async fn new(window: Arc<Window>, config: LunulaConfig, proxy: EventLoopProxy<()>) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            flags: wgpu::InstanceFlags::empty(),
            backend_options: Default::default(),
            display: None,
            memory_budget_thresholds: Default::default(),
        });
        let surface = instance.create_surface(window.clone()).expect("surface");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .expect("adapter");

        // Request MemoryHints::MemoryUsage to minimize WGPU/D3D12 suballocator heap bloat
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Lunula"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: Default::default(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: Default::default(),
            })
            .await
            .expect("device");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let present_mode = if surface_caps.present_modes.contains(&wgpu::PresentMode::AutoVsync) {
            wgpu::PresentMode::AutoVsync
        } else if surface_caps.present_modes.contains(&wgpu::PresentMode::Fifo) {
            wgpu::PresentMode::Fifo
        } else {
            surface_caps.present_modes[0]
        };

        let (alpha_mode, _is_transparent) = if surface_caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            (wgpu::CompositeAlphaMode::PreMultiplied, true)
        } else if surface_caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PostMultiplied)
        {
            (wgpu::CompositeAlphaMode::PostMultiplied, true)
        } else if surface_caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::Inherit)
        {
            (wgpu::CompositeAlphaMode::Inherit, true)
        } else {
            (surface_caps.alpha_modes[0], false)
        };

        let mut sc = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .expect("Surface config not supported");
        sc.format = surface_format;
        sc.present_mode = present_mode;
        sc.alpha_mode = alpha_mode;
        surface.configure(&device, &sc);

        let w = sc.width.max(200);
        let h = sc.height.max(150);

        let wobble = WobbleWindow::new(
            &device,
            sc.format,
            [w as f32, h as f32],
            &config.wobble,
            config.window.opacity,
        );
        let mut text_engine = TextEngine::new(
            &device,
            &queue,
            sc.format,
            [
                wobble.content_size[0] as f32,
                wobble.content_size[1] as f32,
            ],
            config.terminal.font_size,
            &config.terminal.font_family,
        );

        let content_w = wobble.content_size[0] as f32;
        let content_h = wobble.content_size[1] as f32;
        let tab_manager = pane::TabManager::new(
            content_w,
            content_h,
            &mut text_engine,
            proxy,
            &config,
        )
        .expect("Failed to initialize TabManager");

        let win_pos = window.outer_position().unwrap_or_default();
        let win_size = window.inner_size();
        let config_path = LunulaConfig::find_config_path()
            .unwrap_or_else(|| std::path::PathBuf::from("lunula.lua"));
        let last_config_modified = std::fs::metadata(&config_path)
            .ok()
            .and_then(|m| m.modified().ok());

        let mut help_buffer = text_engine.create_buffer();
        let help_total_height = build_help_buffer(&mut text_engine, &mut help_buffer);

        let bar_center_buffer = text_engine.create_buffer();
        let bar_clock_buffer = text_engine.create_buffer();
        let bar_gear_buffer = text_engine.create_buffer();
        let bar_help_buffer = text_engine.create_buffer();
        let vi_status_buffer = text_engine.create_buffer();
        let cached_username = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_else(|_| "user".to_string());
        let cached_git_branch = get_git_branch().unwrap_or_else(|| "detached".to_string());

        // Trim working set after initialization to release temporary startup allocations
        crate::platform::trim_working_set();

        Self {
            window,
            device,
            queue,
            surface,
            surface_config: sc,
            wobble,
            text_engine,
            tab_manager,
            config,
            config_path,
            last_config_check: Instant::now(),
            last_config_modified,
            active_window_title: String::new(),
            is_maximized: false,
            is_focused: true,
            saved_win_pos: win_pos,
            saved_win_size: win_size,
            ctrl_pressed: false,
            shift_pressed: false,
            alt_pressed: false,
            dragging: false,
            mouse_left_down: false,
            last_drag_cell: None,
            selecting: false,
            help_visible: false,
            help_buffer,
            vi_status_buffer,
            help_scroll_y: 0.0,
            help_total_height,
            help_dragging_scrollbar: false,
            bar_center_buffer,
            bar_clock_buffer,
            bar_gear_buffer,
            bar_help_buffer,
            bar_center_width: 140.0,
            bar_clock_width: 44.0,
            bar_gear_width: 12.0,
            bar_help_width: 10.0,
            bar_icons_dirty: true,
            cached_username,
            cached_git_branch,
            bar_center_dirty: true,
            last_git_check: Instant::now(),
            last_time_str: String::new(),
            drag_start_cursor: (0, 0),
            drag_start_win: PhysicalPosition::new(0, 0),
            last_win_pos: win_pos,
            last_cursor_pos: (0, 0),
            last_cursor_time: Instant::now(),
            mouse_pos: [0.0, 0.0],
            last_click_time: Instant::now(),
            last_click_cell: (0, 0),
            click_streak: 0,
            last_frame_time: Instant::now(),
            high_fps_animating: false,
            next_blink_frame: None,
            initial_trim_frames: 0,
        }
    }

    pub fn toggle_help(&mut self) {
        self.help_visible = !self.help_visible;
        if self.help_visible {
            self.help_scroll_y = 0.0;
            self.help_dragging_scrollbar = false;
        }
        self.window.request_redraw();
    }

    pub fn reload_config(&mut self) {
        let new_cfg = LunulaConfig::load();
        self.config = new_cfg.clone();

        self.wobble.stiffness = new_cfg.wobble.stiffness;
        self.wobble.damping = new_cfg.wobble.damping;
        self.wobble.max_deform = new_cfg.wobble.max_deform;
        self.wobble.spring_shear = new_cfg.wobble.spring_shear;
        self.wobble.opacity = new_cfg.window.opacity;
        self.wobble.mesh_dirty = true;

        for pane in self.tab_manager.panes.values_mut() {
            pane.terminal.selection_color = terminal::ColorRgb {
                r: new_cfg.colors.selection[0],
                g: new_cfg.colors.selection[1],
                b: new_cfg.colors.selection[2],
            };
            pane.terminal.max_scrollback = new_cfg.terminal.max_scrollback;
            pane.terminal.cursor_config.shape = match new_cfg.cursor.shape.as_str() {
                "beam" => terminal::CursorShape::Beam,
                "underline" => terminal::CursorShape::Underline,
                _ => terminal::CursorShape::Block,
            };
            pane.terminal.cursor_config.blinking = new_cfg.cursor.blinking;
            pane.terminal.cursor_config.color = terminal::ColorRgb {
                r: new_cfg.cursor.color[0],
                g: new_cfg.cursor.color[1],
                b: new_cfg.cursor.color[2],
            };
            pane.terminal.dirty = true;
        }

        if (self.text_engine.font_size - new_cfg.terminal.font_size).abs() > 0.1
            || self.text_engine.requested_font_family != new_cfg.terminal.font_family
        {
            if self.text_engine.requested_font_family != new_cfg.terminal.font_family {
                self.text_engine
                    .update_font_family(&new_cfg.terminal.font_family);
            }
            self.text_engine.update_font_size(new_cfg.terminal.font_size);
            self.help_total_height =
                build_help_buffer(&mut self.text_engine, &mut self.help_buffer);
            self.bar_center_dirty = true;
            self.bar_icons_dirty = true;
            self.last_time_str.clear();
            self.resize(self.window.inner_size());
        }

        self.window.request_redraw();
    }

    pub fn resize(&mut self, s: PhysicalSize<u32>) {
        if s.width > 0 && s.height > 0 {
            self.surface_config.width = s.width;
            self.surface_config.height = s.height;
            self.surface.configure(&self.device, &self.surface_config);

            self.wobble
                .resize(&self.device, s.width as f32, s.height as f32, self.is_maximized);
            self.text_engine.resize(
                &self.queue,
                self.wobble.content_size[0] as f32,
                self.wobble.content_size[1] as f32,
            );

            let content_w = self.wobble.content_size[0] as f32;
            let content_h = self.wobble.content_size[1] as f32;
            self.tab_manager
                .layout(content_w, content_h, &mut self.text_engine, &self.config);
            self.last_win_pos = self.window.outer_position().unwrap_or_default();

            let card_h = 580.0f32.min(content_h - 24.0);
            let visible_h = card_h - 32.0;
            let max_scroll = (self.help_total_height - visible_h).max(0.0);
            self.help_scroll_y = self.help_scroll_y.clamp(0.0, max_scroll);
        }
    }

    pub fn toggle_maximize(&mut self) {
        if self.is_maximized {
            self.is_maximized = false;
            self.window.set_outer_position(self.saved_win_pos);
            let _ = self.window.request_inner_size(self.saved_win_size);
            self.resize(self.saved_win_size);
        } else {
            if let Ok(pos) = self.window.outer_position() {
                self.saved_win_pos = pos;
            }
            self.saved_win_size = self.window.inner_size();

            let mon = self
                .window
                .current_monitor()
                .or_else(|| self.window.primary_monitor());
            if let Some(m) = mon {
                let m_size = m.size();
                let m_pos = m.position();
                self.is_maximized = true;
                self.window.set_outer_position(m_pos);
                let _ = self.window.request_inner_size(m_size);
                self.resize(m_size);
            }
        }
        self.window.request_redraw();
    }

    pub fn drain_pty(&mut self) {
        let content_w = self.wobble.content_size[0] as f32;
        let content_h = self.wobble.content_size[1] as f32;
        let _ = self.tab_manager.reap_exited_panes(
            content_w,
            content_h,
            &mut self.text_engine,
            &self.config,
        );
        let _changed = self
            .tab_manager
            .drain_all_ptys(&mut self.text_engine, &self.config);

        if let Some(pane) = self.tab_manager.active_pane() {
            if !pane.title.is_empty() && pane.title != self.active_window_title {
                self.active_window_title = pane.title.clone();
                self.window
                    .set_title(&format!("Lunula - {}", self.active_window_title));
            }
        }
    }

    /// Periodically checks if `lunula.lua` or `.git/HEAD` changed when a user interaction occurs.
    pub fn check_background_metadata(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.last_config_check).as_millis() > 1000 {
            self.last_config_check = now;
            if let Ok(meta) = std::fs::metadata(&self.config_path) {
                if let Ok(modified) = meta.modified() {
                    if let Some(prev) = self.last_config_modified {
                        if modified > prev {
                            self.last_config_modified = Some(modified);
                            self.reload_config();
                        }
                    } else {
                        self.last_config_modified = Some(modified);
                    }
                }
            }
        }

        if now.duration_since(self.last_git_check).as_secs_f32() > 3.0 {
            self.last_git_check = now;
            let active_cwd = self.tab_manager.active_pane().and_then(|p| {
                if let Some(ref cwd) = p.terminal.cwd {
                    return Some(cwd.clone());
                }
                let title_path = std::path::Path::new(&p.terminal.title);
                if title_path.is_dir() {
                    return Some(title_path.to_path_buf());
                }
                if let Some((_, rest)) = p.terminal.title.split_once(": ") {
                    let rest_path = std::path::Path::new(rest.trim());
                    if rest_path.is_dir() {
                        return Some(rest_path.to_path_buf());
                    }
                }
                None
            });
            let new_branch = active_cwd
                .as_deref()
                .and_then(crate::platform::get_git_branch_for_path)
                .or_else(get_git_branch)
                .unwrap_or_else(|| "detached".to_string());
            if new_branch != self.cached_git_branch {
                self.cached_git_branch = new_branch;
                self.bar_center_dirty = true;
            }
        }
    }
}
