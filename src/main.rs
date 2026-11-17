mod clipboard;
mod config;
mod pane;
mod pty;
mod terminal;
mod text;
mod wobble;

use config::LunulaConfig;
use std::sync::Arc;
use std::time::Instant;
use text::TextEngine;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};
use wobble::{WindowControlAction, WobbleWindow};

#[repr(C)]
struct POINT {
    x: i32,
    y: i32,
}

#[repr(C)]
#[allow(non_snake_case)]
struct SYSTEMTIME {
    wYear: u16,
    wMonth: u16,
    wDayOfWeek: u16,
    wDay: u16,
    wHour: u16,
    wMinute: u16,
    wSecond: u16,
    wMilliseconds: u16,
}

unsafe extern "system" {
    fn GetCursorPos(lp: *mut POINT) -> i32;
    fn GetLocalTime(lpSystemTime: *mut SYSTEMTIME);
}

fn screen_cursor() -> (i32, i32) {
    let mut pt = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut pt) };
    (pt.x, pt.y)
}

fn get_local_time_hm() -> String {
    let mut st = SYSTEMTIME {
        wYear: 0,
        wMonth: 0,
        wDayOfWeek: 0,
        wDay: 0,
        wHour: 0,
        wMinute: 0,
        wSecond: 0,
        wMilliseconds: 0,
    };
    unsafe { GetLocalTime(&mut st) };
    format!("{:02}:{:02}", st.wHour, st.wMinute)
}

fn get_git_branch() -> Option<String> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let git_head = dir.join(".git").join("HEAD");
        if git_head.exists() {
            if let Ok(content) = std::fs::read_to_string(git_head) {
                let trimmed = content.trim();
                if let Some(branch) = trimmed.strip_prefix("ref: refs/heads/") {
                    return Some(branch.to_string());
                } else if trimmed.len() >= 7 {
                    return Some(trimmed[..7].to_string());
                }
            }
            return None;
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

struct LunulaApp {
    config: LunulaConfig,
    proxy: EventLoopProxy<()>,
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
    tab_manager: pane::TabManager,
    config: LunulaConfig,
    config_path: std::path::PathBuf,
    last_config_check: Instant,
    last_config_modified: Option<std::time::SystemTime>,
    active_window_title: String,

    is_maximized: bool,
    saved_win_pos: PhysicalPosition<i32>,
    saved_win_size: PhysicalSize<u32>,

    ctrl_pressed: bool,
    shift_pressed: bool,
    alt_pressed: bool,
    dragging: bool,
    selecting: bool,
    help_visible: bool,
    help_buffer: glyphon::Buffer,
    help_scroll_y: f32,
    help_total_height: f32,
    help_dragging_scrollbar: bool,
    bar_center_buffer: glyphon::Buffer,
    bar_right_buffer: glyphon::Buffer,
    cached_username: String,
    cached_git_branch: String,
    last_git_check: Instant,
    last_time_str: String,
    drag_start_cursor: (i32, i32),
    drag_start_win: PhysicalPosition<i32>,
    last_win_pos: PhysicalPosition<i32>,
    last_cursor_pos: (i32, i32),
    last_cursor_time: Instant,
    mouse_pos: [f32; 2],
    last_click_time: Instant,
    last_click_cell: (usize, usize),
    click_streak: usize,

    last_frame_time: Instant,
}

impl GpuState {
    async fn new(window: Arc<Window>, config: LunulaConfig, proxy: EventLoopProxy<()>) -> Self {
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

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        // PresentMode::Mailbox eliminates up to 16.6ms of VSync blocking per frame
        let present_mode = if surface_caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
            wgpu::PresentMode::Mailbox
        } else if surface_caps.present_modes.contains(&wgpu::PresentMode::Immediate) {
            wgpu::PresentMode::Immediate
        } else {
            wgpu::PresentMode::AutoVsync
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
            [wobble.content_size[0] as f32, wobble.content_size[1] as f32],
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
        ).expect("Failed to initialize TabManager");

        let win_pos = window.outer_position().unwrap_or_default();
        let win_size = window.inner_size();
        let config_path = LunulaConfig::find_config_path()
            .unwrap_or_else(|| std::path::PathBuf::from("lunula.lua"));
        let last_config_modified = std::fs::metadata(&config_path)
            .ok()
            .and_then(|m| m.modified().ok());

        let mut help_buffer = text_engine.create_buffer();
        let help_total_height = Self::build_help_buffer(&mut text_engine, &mut help_buffer);

        let bar_center_buffer = text_engine.create_buffer();
        let bar_right_buffer = text_engine.create_buffer();
        let cached_username = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_else(|_| "user".to_string());
        let cached_git_branch = get_git_branch().unwrap_or_else(|| "detached".to_string());
        let last_time_str = String::new();

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
            saved_win_pos: win_pos,
            saved_win_size: win_size,
            ctrl_pressed: false,
            shift_pressed: false,
            alt_pressed: false,
            dragging: false,
            selecting: false,
            help_visible: false,
            help_buffer,
            help_scroll_y: 0.0,
            help_total_height,
            help_dragging_scrollbar: false,
            bar_center_buffer,
            bar_right_buffer,
            cached_username,
            cached_git_branch,
            last_git_check: Instant::now(),
            last_time_str,
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
        }
    }

    fn toggle_help(&mut self) {
        self.help_visible = !self.help_visible;
        if self.help_visible {
            self.help_scroll_y = 0.0;
            self.help_dragging_scrollbar = false;
        }
        self.window.request_redraw();
    }

    fn build_help_buffer(text_engine: &mut TextEngine, buffer: &mut glyphon::Buffer) -> f32 {
        let mut spans = Vec::new();

        let header_style = terminal::CellStyle {
            fg: Some(terminal::ColorRgb { r: 180, g: 190, b: 254 }), // Catppuccin Lavender
            bold: true,
            ..Default::default()
        };
        let section_style = terminal::CellStyle {
            fg: Some(terminal::ColorRgb { r: 249, g: 226, b: 175 }), // Catppuccin Yellow
            bold: true,
            ..Default::default()
        };
        let key_style = terminal::CellStyle {
            fg: Some(terminal::ColorRgb { r: 166, g: 227, b: 161 }), // Catppuccin Green
            bold: true,
            ..Default::default()
        };
        let desc_style = terminal::CellStyle {
            fg: Some(terminal::ColorRgb { r: 205, g: 214, b: 244 }), // Catppuccin Text
            ..Default::default()
        };
        let sub_style = terminal::CellStyle {
            fg: Some(terminal::ColorRgb { r: 147, g: 153, b: 178 }), // Catppuccin Overlay2
            dim: true,
            ..Default::default()
        };
        let border_style = terminal::CellStyle {
            fg: Some(terminal::ColorRgb { r: 108, g: 112, b: 134 }), // Catppuccin Overlay0
            ..Default::default()
        };

        // Title
        spans.push(("  🌙 LUNULA TERMINAL ── Shortcuts & Features Reference\n".to_string(), header_style));
        spans.push(("  Scroll with Mouse Wheel or ↑ ↓ / PgUp PgDn • Press [ESC] or [Q] to close\n\n".to_string(), sub_style));

        // Panes & Splits section
        spans.push(("  🪟 SPLIT PANES\n".to_string(), section_style));
        spans.push(("  Ctrl + Shift + E    ".to_string(), key_style));
        spans.push(("Split Vertically (side-by-side)\n".to_string(), desc_style));
        spans.push(("  Ctrl + Shift + D    ".to_string(), key_style));
        spans.push(("Split Horizontally (top / bottom)\n".to_string(), desc_style));
        spans.push(("  Ctrl + Shift + W    ".to_string(), key_style));
        spans.push(("Close Active Pane / Split (or exit if last)\n".to_string(), desc_style));
        spans.push(("  Ctrl + W            ".to_string(), key_style));
        spans.push(("Close Active Split Pane (when multiple panes open)\n".to_string(), desc_style));
        spans.push(("  Alt + Arrows        ".to_string(), key_style));
        spans.push(("Navigate focus between panes (← ↑ → ↓)\n".to_string(), desc_style));
        spans.push(("  exit                ".to_string(), key_style));
        spans.push(("Type 'exit' in shell to close split pane\n".to_string(), desc_style));
        spans.push(("  [✕] button          ".to_string(), key_style));
        spans.push(("Click close badge in top-right of split pane\n\n".to_string(), desc_style));

        // Tabs section
        spans.push(("  📑 TABS\n".to_string(), section_style));
        spans.push(("  Ctrl + Shift + T    ".to_string(), key_style));
        spans.push(("New Tab\n".to_string(), desc_style));
        spans.push(("  Ctrl + Tab          ".to_string(), key_style));
        spans.push(("Switch to Next Tab\n".to_string(), desc_style));
        spans.push(("  Ctrl + Shift + Tab  ".to_string(), key_style));
        spans.push(("Switch to Previous Tab\n".to_string(), desc_style));
        spans.push(("  Ctrl + 1..9         ".to_string(), key_style));
        spans.push(("Jump directly to Tab 1 through 9\n\n".to_string(), desc_style));

        // Utilities & Window
        spans.push(("  ⚡ WINDOW & HOT RELOAD\n".to_string(), section_style));
        spans.push(("  F11                 ".to_string(), key_style));
        spans.push(("Toggle Fullscreen / Maximize\n".to_string(), desc_style));
        spans.push(("  Ctrl + Shift + R    ".to_string(), key_style));
        spans.push(("Hot Reload configuration (lunula.lua)\n".to_string(), desc_style));
        spans.push(("  Ctrl + Shift + P    ".to_string(), key_style));
        spans.push(("Hot Reload PowerShell profile (. $PROFILE)\n".to_string(), desc_style));
        spans.push(("  F1 / Ctrl+Shift+H   ".to_string(), key_style));
        spans.push(("Toggle this Help reference menu\n".to_string(), desc_style));
        spans.push(("  Drag Titlebar       ".to_string(), key_style));
        spans.push(("Move window with jelly wobble physics\n\n".to_string(), desc_style));

        // Clipboard & Selection
        spans.push(("  📋 CLIPBOARD & SELECTION\n".to_string(), section_style));
        spans.push(("  Left-Click + Drag   ".to_string(), key_style));
        spans.push(("Select text (auto-clamped to text content)\n".to_string(), desc_style));
        spans.push(("  Right-Click         ".to_string(), key_style));
        spans.push(("Copy selection / Paste from clipboard\n".to_string(), desc_style));
        spans.push(("  Ctrl + C            ".to_string(), key_style));
        spans.push(("Copy selection (or send interrupt SIGINT)\n".to_string(), desc_style));
        spans.push(("  Ctrl + V            ".to_string(), key_style));
        spans.push(("Paste clipboard text\n\n".to_string(), desc_style));

        // Footer
        spans.push(("  ─────────────────────────────────────────────────────────────\n".to_string(), border_style));
        spans.push(("  💡 Tips: Scroll with Mouse Wheel, Arrows (↑ ↓), or drag scrollbar\n".to_string(), key_style));
        spans.push(("  Press [ESC], [F1], [Q], or click outside / [✕] to close this guide\n".to_string(), sub_style));

        text_engine.shape_buffer(buffer, &spans);
        let line_count = spans.iter().map(|(t, _)| t.matches('\n').count()).sum::<usize>() + 2;
        line_count as f32 * text_engine.line_height + 40.0
    }

    fn reload_config(&mut self) {
        let new_cfg = LunulaConfig::load();
        self.config = new_cfg.clone();

        // Update wobble physics live
        self.wobble.stiffness = new_cfg.wobble.stiffness;
        self.wobble.damping = new_cfg.wobble.damping;
        self.wobble.max_deform = new_cfg.wobble.max_deform;
        self.wobble.spring_shear = new_cfg.wobble.spring_shear;
        self.wobble.opacity = new_cfg.window.opacity;

        // Update all terminal panes
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

        // If font size or family changed: update text engine & resize grid
        if (self.text_engine.font_size - new_cfg.terminal.font_size).abs() > 0.1
            || self.text_engine.requested_font_family != new_cfg.terminal.font_family
        {
            if self.text_engine.requested_font_family != new_cfg.terminal.font_family {
                self.text_engine.update_font_family(&new_cfg.terminal.font_family);
            }
            self.text_engine.update_font_size(new_cfg.terminal.font_size);
            self.help_total_height = Self::build_help_buffer(&mut self.text_engine, &mut self.help_buffer);
            self.resize(self.window.inner_size());
        }

        self.window.request_redraw();
    }

    fn resize(&mut self, s: PhysicalSize<u32>) {
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

    fn toggle_maximize(&mut self) {
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

            let mon = self.window.current_monitor().or_else(|| self.window.primary_monitor());
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

    fn drain_pty_raw(&mut self) -> bool {
        let mut changed = false;
        let active_id = self.tab_manager.active_tab().active_pane_id;
        if let Some(pane) = self.tab_manager.panes.get_mut(&active_id) {
            while let Ok(bytes) = pane.pty.rx.try_recv() {
                pane.terminal.advance_bytes(&bytes);
                changed = true;
            }
        }
        changed
    }

    fn drain_pty(&mut self) {
        let content_w = self.wobble.content_size[0] as f32;
        let content_h = self.wobble.content_size[1] as f32;
        let _ = self.tab_manager.reap_exited_panes(content_w, content_h, &mut self.text_engine, &self.config);
        let _changed = self.tab_manager.drain_all_ptys(&mut self.text_engine);

        // Update window title from active tab/pane
        if let Some(pane) = self.tab_manager.active_pane() {
            if !pane.title.is_empty() && pane.title != self.active_window_title {
                self.active_window_title = pane.title.clone();
                self.window
                    .set_title(&format!("Lunula - {}", self.active_window_title));
            }
        }
    }

    fn render(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_frame_time).as_secs_f32().clamp(0.001, 0.05);
        self.last_frame_time = now;

        // Check for file-change hot reload every 500ms
        if now.duration_since(self.last_config_check).as_millis() > 500 {
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

        // 1. Update wobble physics
        self.wobble.update_physics();

        let content_w = self.wobble.content_size[0] as f32;
        let content_h = self.wobble.content_size[1] as f32;
        let cell_w = self.text_engine.cell_width;
        let cell_h = self.text_engine.cell_height;

        // 2. Drain PTY inputs
        self.drain_pty();

        // 3. Update physics & smooth scrolling for active tab panes
        let active_tab = self.tab_manager.active_tab();
        let active_pane_id = active_tab.active_pane_id;
        let visible_leaves = active_tab.root.all_leaves();

        let mut is_animating = self.wobble.is_dragging || self.wobble.is_wobbling();

        for &id in &visible_leaves {
            if let Some(pane) = self.tab_manager.panes.get_mut(&id) {
                if self.config.scroll.smooth {
                    if pane.terminal.update_smooth_scroll(dt) {
                        is_animating = true;
                    }
                }
                pane.update_physics(dt, &self.config, cell_w, cell_h);
                if (pane.cursor_head[0] - pane.cursor_tail[0]).hypot(pane.cursor_head[1] - pane.cursor_tail[1]) > 0.5 {
                    is_animating = true;
                }
            }
        }

        // 4. Assemble all GPU Background Quads & Decorations
        let mut bg_rects = Vec::new();
        let tab_bar_visible = self.tab_manager.tab_bar_visible(&self.config);
        let tab_bar_h = if tab_bar_visible { self.config.tabs.height } else { 0.0 };

        // 40. Update status bar information (time, git branch)
        if now.duration_since(self.last_git_check).as_secs_f32() > 2.0 {
            self.cached_git_branch = get_git_branch().unwrap_or_else(|| "detached".to_string());
            self.last_git_check = now;
        }
        let time_str = get_local_time_hm();
        if time_str != self.last_time_str {
            self.last_time_str = time_str.clone();
            // Update right buffer: "HH:MM     ⚙     ?"
            let right_spans = vec![
                (
                    format!("{:>5}    ", time_str),
                    terminal::CellStyle {
                        fg: Some(terminal::ColorRgb { r: 186, g: 194, b: 222 }), // Subtext 1
                        bold: true,
                        ..Default::default()
                    },
                ),
                (
                    "⚙".to_string(),
                    terminal::CellStyle {
                        fg: Some(terminal::ColorRgb { r: 137, g: 220, b: 235 }), // Sky
                        bold: true,
                        ..Default::default()
                    },
                ),
                (
                    "    ?".to_string(),
                    terminal::CellStyle {
                        fg: Some(terminal::ColorRgb { r: 249, g: 226, b: 175 }), // Yellow
                        bold: true,
                        ..Default::default()
                    },
                ),
            ];
            self.text_engine.shape_buffer(&mut self.bar_right_buffer, &right_spans);
        }

        // Center spans: "username :  branch"
        let center_spans = vec![
            (
                format!("  {} ", self.cached_username),
                terminal::CellStyle {
                    fg: Some(terminal::ColorRgb { r: 137, g: 180, b: 250 }), // Blue
                    bold: true,
                    ..Default::default()
                },
            ),
            (
                ":".to_string(),
                terminal::CellStyle {
                    fg: Some(terminal::ColorRgb { r: 108, g: 112, b: 134 }), // Overlay 0
                    bold: false,
                    ..Default::default()
                },
            ),
            (
                format!("  {}  ", self.cached_git_branch),
                terminal::CellStyle {
                    fg: Some(terminal::ColorRgb { r: 166, g: 227, b: 161 }), // Green
                    bold: true,
                    ..Default::default()
                },
            ),
        ];
        let center_text_len = self.cached_username.len() + self.cached_git_branch.len() + 8;
        self.text_engine.shape_buffer(&mut self.bar_center_buffer, &center_spans);

        // Titlebar Background (0..42px)
        bg_rects.push(terminal::BgRect {
            x: 0.0,
            y: 0.0,
            w: content_w,
            h: 42.0,
            color: terminal::ColorRgb { r: 24, g: 24, b: 37 }, // Mantle
            alpha: 0.98,
        });

        // Center Info Pill on Titlebar: "username :  branch"
        let center_pill_w = (center_text_len as f32 * cell_w + 32.0).clamp(140.0, 360.0);
        let center_pill_x = (content_w - center_pill_w) * 0.5;
        if center_pill_x > 90.0 && center_pill_x + center_pill_w < content_w - 170.0 {
            bg_rects.push(terminal::BgRect {
                x: center_pill_x,
                y: 7.0,
                w: center_pill_w,
                h: 28.0,
                color: terminal::ColorRgb { r: 30, g: 30, b: 46 }, // Base
                alpha: 0.85,
            });
            bg_rects.push(terminal::BgRect {
                x: center_pill_x,
                y: 34.0,
                w: center_pill_w,
                h: 1.0,
                color: terminal::ColorRgb { r: 69, g: 71, b: 90 }, // Surface 1
                alpha: 0.60,
            });
        }

        // Right-aligned status items on Titlebar:
        // [HH:MM] Clock Pill
        let clock_w = 60.0;
        let clock_x = content_w - 150.0;
        bg_rects.push(terminal::BgRect {
            x: clock_x,
            y: 7.0,
            w: clock_w,
            h: 28.0,
            color: terminal::ColorRgb { r: 30, g: 30, b: 46 },
            alpha: 0.75,
        });

        // [⚙] Settings Button
        let is_gear_hover = self.mouse_pos[0] >= content_w - 74.0 && self.mouse_pos[0] <= content_w - 42.0
            && self.mouse_pos[1] >= 6.0 && self.mouse_pos[1] <= 36.0;
        bg_rects.push(terminal::BgRect {
            x: content_w - 74.0,
            y: 7.0,
            w: 32.0,
            h: 28.0,
            color: if is_gear_hover {
                terminal::ColorRgb { r: 49, g: 50, b: 68 }
            } else {
                terminal::ColorRgb { r: 30, g: 30, b: 46 }
            },
            alpha: if is_gear_hover { 0.95 } else { 0.70 },
        });

        // [?] Help Reference Button
        let is_help_hover = self.mouse_pos[0] >= content_w - 38.0 && self.mouse_pos[0] <= content_w - 6.0
            && self.mouse_pos[1] >= 6.0 && self.mouse_pos[1] <= 36.0;
        bg_rects.push(terminal::BgRect {
            x: content_w - 38.0,
            y: 7.0,
            w: 32.0,
            h: 28.0,
            color: if is_help_hover {
                terminal::ColorRgb { r: 49, g: 50, b: 68 }
            } else {
                terminal::ColorRgb { r: 30, g: 30, b: 46 }
            },
            alpha: if is_help_hover { 0.95 } else { 0.70 },
        });

        // 4a. Tab Bar Background & Tabs (below titlebar at y = 42.0..74.0)
        if tab_bar_visible {
            let tab_bar_y = 42.0;
            bg_rects.push(terminal::BgRect {
                x: 0.0,
                y: tab_bar_y,
                w: content_w,
                h: tab_bar_h,
                color: terminal::ColorRgb { r: 24, g: 24, b: 37 }, // Mantle
                alpha: 0.95,
            });
            bg_rects.push(terminal::BgRect {
                x: 0.0,
                y: tab_bar_y + tab_bar_h - 1.0,
                w: content_w,
                h: 1.0,
                color: terminal::ColorRgb { r: 69, g: 71, b: 90 }, // Surface 1
                alpha: 0.8,
            });

            let mut pill_x = 10.0;
            let pill_w = 140.0;
            for (idx, _t) in self.tab_manager.tabs.iter().enumerate() {
                let is_active = idx == self.tab_manager.active_tab_index;
                let bg_col = if is_active {
                    terminal::ColorRgb { r: 49, g: 50, b: 68 } // Surface 0
                } else {
                    terminal::ColorRgb { r: 30, g: 30, b: 46 } // Base
                };
                bg_rects.push(terminal::BgRect {
                    x: pill_x,
                    y: tab_bar_y + 4.0,
                    w: pill_w,
                    h: tab_bar_h - 8.0,
                    color: bg_col,
                    alpha: if is_active { 0.95 } else { 0.70 },
                });

                if is_active {
                    bg_rects.push(terminal::BgRect {
                        x: pill_x + 4.0,
                        y: tab_bar_y + tab_bar_h - 3.0,
                        w: pill_w - 8.0,
                        h: 2.0,
                        color: terminal::ColorRgb { r: 180, g: 190, b: 254 }, // Lavender
                        alpha: 1.0,
                    });
                }
                pill_x += pill_w + 6.0;
            }

            // '+' New Tab Button
            let is_plus_hover = self.mouse_pos[0] >= pill_x && self.mouse_pos[0] <= pill_x + 32.0
                && self.mouse_pos[1] >= tab_bar_y + 4.0 && self.mouse_pos[1] <= tab_bar_y + tab_bar_h - 4.0;
            bg_rects.push(terminal::BgRect {
                x: pill_x,
                y: tab_bar_y + 4.0,
                w: 32.0,
                h: tab_bar_h - 8.0,
                color: if is_plus_hover {
                    terminal::ColorRgb { r: 49, g: 50, b: 68 }
                } else {
                    terminal::ColorRgb { r: 30, g: 30, b: 46 }
                },
                alpha: if is_plus_hover { 0.90 } else { 0.70 },
            });
        }

        // 4b. Visible Panes Backgrounds, Dimming, Cursor & Trail
        for &id in &visible_leaves {
            if let Some(pane) = self.tab_manager.panes.get(&id) {
                let px = pane.rect[0];
                let py = pane.rect[1];
                let pw = pane.rect[2];
                let ph = pane.rect[3];
                let is_active = id == active_pane_id;

                for rect in &pane.cached_bg_rects {
                    let rx = px + rect.x;
                    let ry = py + rect.y;
                    if rx < px + pw && ry < py + ph {
                        bg_rects.push(terminal::BgRect {
                            x: rx,
                            y: ry,
                            w: rect.w.min(px + pw - rx),
                            h: rect.h.min(py + ph - ry),
                            color: rect.color,
                            alpha: rect.alpha,
                        });
                    }
                }

                // Close button [✕] for split panes when multiple panes exist
                if visible_leaves.len() > 1 {
                    let btn_x = px + pw - 24.0;
                    let btn_y = py + 4.0;
                    let is_hovered = self.mouse_pos[0] >= btn_x && self.mouse_pos[0] <= btn_x + 20.0
                        && self.mouse_pos[1] >= btn_y && self.mouse_pos[1] <= btn_y + 20.0;
                    bg_rects.push(terminal::BgRect {
                        x: btn_x,
                        y: btn_y,
                        w: 20.0,
                        h: 20.0,
                        color: if is_hovered {
                            terminal::ColorRgb { r: 243, g: 139, b: 168 }
                        } else {
                            terminal::ColorRgb { r: 49, g: 50, b: 68 }
                        },
                        alpha: if is_hovered { 0.95 } else { 0.65 },
                    });
                    bg_rects.push(terminal::BgRect {
                        x: btn_x + 5.0,
                        y: btn_y + 9.0,
                        w: 10.0,
                        h: 2.0,
                        color: terminal::ColorRgb { r: 205, g: 214, b: 244 },
                        alpha: 0.90,
                    });
                }

                // Inactive Pane Dimming Quad
                if !is_active && self.config.tabs.dim_inactive_panes {
                    bg_rects.push(terminal::BgRect {
                        x: px,
                        y: py,
                        w: pw,
                        h: ph,
                        color: terminal::ColorRgb { r: 17, g: 17, b: 27 }, // Crust
                        alpha: self.config.tabs.dim_opacity,
                    });
                }

                // Active Pane Cursor & Trail
                if is_active && pane.terminal.cursor_visible && pane.terminal.current_scroll_y <= 5.0 {
                    let cur_color = pane.terminal.cursor_config.color;
                    let dist = (pane.cursor_head[0] - pane.cursor_tail[0])
                        .hypot(pane.cursor_head[1] - pane.cursor_tail[1]);

                    // Elastic trail smear
                    if dist > 1.5 && self.config.cursor.trail {
                        let min_x = pane.cursor_tail[0].min(pane.cursor_head[0]);
                        let max_x = pane.cursor_tail[0].max(pane.cursor_head[0]) + cell_w;
                        let min_y = pane.cursor_tail[1].min(pane.cursor_head[1]);
                        let max_y = pane.cursor_tail[1].max(pane.cursor_head[1]) + cell_h;

                        bg_rects.push(terminal::BgRect {
                            x: min_x,
                            y: min_y,
                            w: (max_x - min_x).max(cell_w),
                            h: (max_y - min_y).max(cell_h),
                            color: cur_color,
                            alpha: 0.28,
                        });
                    }

                    // Head cursor with smooth sine blinking
                    let is_moving = now.duration_since(pane.cursor_last_moved).as_secs_f32() < 0.45;
                    let cursor_alpha = if is_moving || !self.config.cursor.blinking {
                        1.0
                    } else {
                        is_animating = true;
                        0.35 + 0.65 * (0.5 + 0.5 * (pane.cursor_blink_time * 5.0).sin())
                    };

                    match pane.terminal.cursor_config.shape {
                        terminal::CursorShape::Block => {
                            bg_rects.push(terminal::BgRect {
                                x: pane.cursor_head[0],
                                y: pane.cursor_head[1],
                                w: cell_w,
                                h: cell_h,
                                color: cur_color,
                                alpha: cursor_alpha,
                            });
                        }
                        terminal::CursorShape::Beam => {
                            bg_rects.push(terminal::BgRect {
                                x: pane.cursor_head[0],
                                y: pane.cursor_head[1],
                                w: 2.5,
                                h: cell_h,
                                color: cur_color,
                                alpha: cursor_alpha,
                            });
                        }
                        terminal::CursorShape::Underline => {
                            bg_rects.push(terminal::BgRect {
                                x: pane.cursor_head[0],
                                y: pane.cursor_head[1] + cell_h - 2.5,
                                w: cell_w,
                                h: 2.5,
                                color: cur_color,
                                alpha: cursor_alpha,
                            });
                        }
                    }
                }
            }
        }

        // 4c. Pane Separators
        for &(_dir, sep) in &self.tab_manager.separators {
            bg_rects.push(terminal::BgRect {
                x: sep[0],
                y: sep[1],
                w: sep[2],
                h: sep[3],
                color: terminal::ColorRgb { r: 69, g: 71, b: 90 }, // Surface 1
                alpha: 0.95,
            });
        }

        // 4d. Help Reference Modal Overlay
        let card_w = 660.0f32.min(content_w - 24.0);
        let card_h = 580.0f32.min(content_h - 24.0);
        let card_x = (content_w - card_w) * 0.5;
        let card_y = (content_h - card_h) * 0.5;

        if self.help_visible {
            // Full-screen Dimming Backdrop
            bg_rects.push(terminal::BgRect {
                x: 0.0,
                y: 0.0,
                w: content_w,
                h: content_h,
                color: terminal::ColorRgb { r: 17, g: 17, b: 27 },
                alpha: 0.78,
            });

            // Card Shadow
            bg_rects.push(terminal::BgRect {
                x: card_x + 8.0,
                y: card_y + 10.0,
                w: card_w,
                h: card_h,
                color: terminal::ColorRgb { r: 0, g: 0, b: 0 },
                alpha: 0.50,
            });

            // Card Background (Mantle)
            bg_rects.push(terminal::BgRect {
                x: card_x,
                y: card_y,
                w: card_w,
                h: card_h,
                color: terminal::ColorRgb { r: 24, g: 24, b: 37 },
                alpha: 0.98,
            });

            // Card Top Accent Bar (Lavender)
            bg_rects.push(terminal::BgRect {
                x: card_x,
                y: card_y,
                w: card_w,
                h: 3.5,
                color: terminal::ColorRgb { r: 180, g: 190, b: 254 },
                alpha: 1.0,
            });

            // Card Subtle Border Lines (Surface 1)
            bg_rects.push(terminal::BgRect {
                x: card_x,
                y: card_y + 3.5,
                w: 1.0,
                h: card_h - 3.5,
                color: terminal::ColorRgb { r: 69, g: 71, b: 90 },
                alpha: 0.85,
            });
            bg_rects.push(terminal::BgRect {
                x: card_x + card_w - 1.0,
                y: card_y + 3.5,
                w: 1.0,
                h: card_h - 3.5,
                color: terminal::ColorRgb { r: 69, g: 71, b: 90 },
                alpha: 0.85,
            });
            bg_rects.push(terminal::BgRect {
                x: card_x,
                y: card_y + card_h - 1.0,
                w: card_w,
                h: 1.0,
                color: terminal::ColorRgb { r: 69, g: 71, b: 90 },
                alpha: 0.85,
            });

            // Close button [✕] at top-right of modal
            let close_x = card_x + card_w - 32.0;
            let close_y = card_y + 8.0;
            let is_close_hovered = self.mouse_pos[0] >= close_x && self.mouse_pos[0] <= close_x + 24.0
                && self.mouse_pos[1] >= close_y && self.mouse_pos[1] <= close_y + 24.0;
            bg_rects.push(terminal::BgRect {
                x: close_x,
                y: close_y,
                w: 24.0,
                h: 24.0,
                color: if is_close_hovered {
                    terminal::ColorRgb { r: 243, g: 139, b: 168 }
                } else {
                    terminal::ColorRgb { r: 49, g: 50, b: 68 }
                },
                alpha: if is_close_hovered { 0.95 } else { 0.70 },
            });
            bg_rects.push(terminal::BgRect {
                x: close_x + 6.0,
                y: close_y + 11.0,
                w: 12.0,
                h: 2.0,
                color: terminal::ColorRgb { r: 205, g: 214, b: 244 },
                alpha: 0.95,
            });

            // Scrollbar Track & Thumb
            let visible_h = card_h - 32.0;
            let max_scroll = (self.help_total_height - visible_h).max(0.0);
            if max_scroll > 0.0 {
                let track_x = card_x + card_w - 12.0;
                let track_y = card_y + 40.0;
                let track_w = 4.0;
                let track_h = card_h - 52.0;

                // Track background
                bg_rects.push(terminal::BgRect {
                    x: track_x,
                    y: track_y,
                    w: track_w,
                    h: track_h,
                    color: terminal::ColorRgb { r: 49, g: 50, b: 68 }, // Surface 0
                    alpha: 0.45,
                });

                // Thumb
                let thumb_ratio = (visible_h / self.help_total_height).clamp(0.1, 0.9);
                let thumb_h = (track_h * thumb_ratio).max(28.0);
                let scroll_ratio = (self.help_scroll_y / max_scroll).clamp(0.0, 1.0);
                let thumb_y = track_y + scroll_ratio * (track_h - thumb_h);

                let is_thumb_hovered = self.mouse_pos[0] >= track_x - 6.0
                    && self.mouse_pos[0] <= track_x + track_w + 6.0
                    && self.mouse_pos[1] >= thumb_y
                    && self.mouse_pos[1] <= thumb_y + thumb_h;

                bg_rects.push(terminal::BgRect {
                    x: track_x - if is_thumb_hovered || self.help_dragging_scrollbar { 1.0 } else { 0.0 },
                    y: thumb_y,
                    w: track_w + if is_thumb_hovered || self.help_dragging_scrollbar { 2.0 } else { 0.0 },
                    h: thumb_h,
                    color: if self.help_dragging_scrollbar || is_thumb_hovered {
                        terminal::ColorRgb { r: 180, g: 190, b: 254 } // Lavender active
                    } else {
                        terminal::ColorRgb { r: 108, g: 112, b: 134 } // Overlay 0
                    },
                    alpha: if self.help_dragging_scrollbar || is_thumb_hovered { 0.95 } else { 0.70 },
                });
            }
        }

        // Fast upload of background, decorations, and cursor quads
        self.text_engine
            .update_bg_rects(&self.device, &self.queue, &bg_rects);

        // Prepare Text Areas
        let mut areas = Vec::new();

        // 1. Center Pill Text on Titlebar: "username :  branch"
        let center_pill_w = (center_text_len as f32 * cell_w + 32.0).clamp(140.0, 360.0);
        let center_pill_x = (content_w - center_pill_w) * 0.5;
        areas.push(glyphon::TextArea {
            buffer: &self.bar_center_buffer,
            left: center_pill_x,
            top: 11.0,
            scale: 1.0,
            bounds: glyphon::TextBounds {
                left: center_pill_x as i32,
                top: 0,
                right: (center_pill_x + center_pill_w) as i32,
                bottom: 42,
            },
            default_color: glyphon::Color::rgb(205, 214, 244),
            custom_glyphs: &[],
        });

        // 2. Right Status Area on Titlebar: Time, Gear icon, Help icon
        areas.push(glyphon::TextArea {
            buffer: &self.bar_right_buffer,
            left: content_w - 150.0,
            top: 11.0,
            scale: 1.0,
            bounds: glyphon::TextBounds {
                left: (content_w - 150.0) as i32,
                top: 0,
                right: content_w as i32,
                bottom: 42,
            },
            default_color: glyphon::Color::rgb(205, 214, 244),
            custom_glyphs: &[],
        });

        // 3. Tab Bar Text Area (if visible, below titlebar at y = 42.0..74.0)
        if tab_bar_visible {
            let tab_bar_y = 42.0;
            areas.push(glyphon::TextArea {
                buffer: &self.tab_manager.tab_bar_buffer,
                left: 0.0,
                top: tab_bar_y + 6.0,
                scale: 1.0,
                bounds: glyphon::TextBounds {
                    left: 0,
                    top: tab_bar_y as i32,
                    right: content_w as i32,
                    bottom: (tab_bar_y + tab_bar_h) as i32,
                },
                default_color: glyphon::Color::rgb(205, 214, 244),
                custom_glyphs: &[],
            });
        }

        for &id in &visible_leaves {
            if let Some(pane) = self.tab_manager.panes.get(&id) {
                areas.push(glyphon::TextArea {
                    buffer: &pane.buffer,
                    left: pane.rect[0] + pane::PANE_PAD,
                    top: pane.rect[1] + pane::PANE_PAD,
                    scale: 1.0,
                    bounds: glyphon::TextBounds {
                        left: (pane.rect[0] + pane::PANE_PAD) as i32,
                        top: (pane.rect[1] + pane::PANE_PAD) as i32,
                        right: (pane.rect[0] + pane.rect[2] - pane::PANE_PAD) as i32,
                        bottom: (pane.rect[1] + pane.rect[3] - pane::PANE_PAD) as i32,
                    },
                    default_color: glyphon::Color::rgb(205, 214, 244),
                    custom_glyphs: &[],
                });
            }
        }

        if self.help_visible {
            let padding_top = 16.0;
            let padding_bottom = 14.0;
            areas.push(glyphon::TextArea {
                buffer: &self.help_buffer,
                left: card_x + 16.0,
                top: card_y + padding_top - self.help_scroll_y,
                scale: 1.0,
                bounds: glyphon::TextBounds {
                    left: (card_x + 10.0) as i32,
                    top: (card_y + padding_top) as i32,
                    right: (card_x + card_w - 20.0) as i32,
                    bottom: (card_y + card_h - padding_bottom) as i32,
                },
                default_color: glyphon::Color::rgb(205, 214, 244),
                custom_glyphs: &[],
            });
        }

        self.text_engine.prepare_areas(&self.device, &self.queue, areas);

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

        // 1. PASS 1: Render Text & Backgrounds to Offscreen Content Texture
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
                            a: self.config.window.opacity as f64,
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

        // 2. PASS 2: Render Wobbly Window Mesh with Texture Mapping
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main Swapchain Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(if self.is_maximized {
                            wgpu::Color {
                                r: 0.10,
                                g: 0.10,
                                b: 0.15,
                                a: 1.0,
                            }
                        } else {
                            wgpu::Color {
                                r: 0.0,
                                g: 0.0,
                                b: 0.0,
                                a: 0.0,
                            }
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

        if is_animating {
            self.window.request_redraw();
        }
    }
}

impl ApplicationHandler for LunulaApp {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.state.is_none() {
            let mon = el.primary_monitor().or_else(|| el.available_monitors().next());
            let (mon_w, mon_h) = if let Some(m) = mon.as_ref() {
                (m.size().width, m.size().height)
            } else {
                (1920, 1080)
            };
            let (init_w, init_h) = self.config.window.get_effective_dimensions(mon_w, mon_h);

            let attrs = WindowAttributes::default()
                .with_title(&self.config.window.title)
                .with_decorations(false)
                .with_transparent(true)
                .with_inner_size(PhysicalSize::new(init_w, init_h));

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

    fn user_event(&mut self, _el: &ActiveEventLoop, _event: ()) {
        if let Some(s) = self.state.as_mut() {
            s.drain_pty();
            s.window.request_redraw();
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

            WindowEvent::MouseWheel { delta, .. } => {
                let cell_h = s.text_engine.cell_height;
                let delta_y = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_x, y) => y * cell_h * 3.0,
                    winit::event::MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
                };

                if s.help_visible {
                    let content_h = s.wobble.content_size[1] as f32;
                    let card_h = 580.0f32.min(content_h - 24.0);
                    let visible_h = card_h - 32.0;
                    let max_scroll = (s.help_total_height - visible_h).max(0.0);
                    s.help_scroll_y = (s.help_scroll_y - delta_y).clamp(0.0, max_scroll);
                    s.window.request_redraw();
                    return;
                }

                if let Some(pane) = s.tab_manager.active_pane_mut() {
                    if pane.terminal.mouse_reporting && !s.shift_pressed {
                        if let Some([cx, cy]) = s.wobble.screen_to_content_pos(s.mouse_pos[0], s.mouse_pos[1]) {
                            let rel_x = cx - pane.rect[0] - pane::PANE_PAD;
                            let rel_y = cy - pane.rect[1] - pane::PANE_PAD;
                            let col = ((rel_x / s.text_engine.cell_width).max(0.0) as usize)
                                .min(pane.terminal.cols.saturating_sub(1));
                            let screen_row = ((rel_y / s.text_engine.cell_height).max(0.0) as usize)
                                .min(pane.terminal.rows.saturating_sub(1));
                            let btn = if delta_y > 0.0 { 64 } else { 65 };
                            let seq = format!("\x1b[<{};{};{}M", btn, col + 1, screen_row + 1);
                            pane.pty.write_bytes(seq.as_bytes());
                            s.drain_pty();
                            s.window.request_redraw();
                            return;
                        }
                    }

                    if s.config.scroll.smooth {
                        if delta_y > 0.0 {
                            pane.terminal.scroll_up_pixels(delta_y, cell_h);
                        } else if delta_y < 0.0 {
                            pane.terminal.scroll_down_pixels(-delta_y, cell_h);
                        }
                    } else {
                        let lines = (delta_y / cell_h).round() as i32;
                        if lines > 0 {
                            pane.terminal.scroll_up_view(lines as usize);
                        } else if lines < 0 {
                            pane.terminal.scroll_down_view((-lines) as usize);
                        }
                    }
                }
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
                    let content_w = s.wobble.content_size[0] as f32;
                    let content_h = s.wobble.content_size[1] as f32;

                    let phys_code = match &key_event.physical_key {
                        PhysicalKey::Code(code) => Some(*code),
                        _ => None,
                    };
                    let logical_char = match &key_event.logical_key {
                        Key::Character(text_str) => text_str.chars().next(),
                        _ => None,
                    };

                    // F1 / KeyCode::F1: Toggle Help Modal
                    if let Key::Named(NamedKey::F1) = &key_event.logical_key {
                        s.toggle_help();
                        return;
                    }
                    if phys_code == Some(KeyCode::F1) {
                        s.toggle_help();
                        return;
                    }

                    // Escape: Dismiss Help if visible
                    if let Key::Named(NamedKey::Escape) = &key_event.logical_key {
                        if s.help_visible {
                            s.help_visible = false;
                            s.help_dragging_scrollbar = false;
                            s.window.request_redraw();
                            return;
                        }
                    }
                    if phys_code == Some(KeyCode::Escape) && s.help_visible {
                        s.help_visible = false;
                        s.help_dragging_scrollbar = false;
                        s.window.request_redraw();
                        return;
                    }

                    if s.help_visible {
                        let card_h = 580.0f32.min(content_h - 24.0);
                        let visible_h = card_h - 32.0;
                        let max_scroll = (s.help_total_height - visible_h).max(0.0);
                        let cell_h = s.text_engine.cell_height;

                        match phys_code {
                            Some(KeyCode::ArrowDown) | Some(KeyCode::KeyJ) => {
                                s.help_scroll_y = (s.help_scroll_y + cell_h * 2.5).clamp(0.0, max_scroll);
                                s.window.request_redraw();
                                return;
                            }
                            Some(KeyCode::ArrowUp) | Some(KeyCode::KeyK) => {
                                s.help_scroll_y = (s.help_scroll_y - cell_h * 2.5).clamp(0.0, max_scroll);
                                s.window.request_redraw();
                                return;
                            }
                            Some(KeyCode::PageDown) | Some(KeyCode::Space) => {
                                s.help_scroll_y = (s.help_scroll_y + visible_h * 0.75).clamp(0.0, max_scroll);
                                s.window.request_redraw();
                                return;
                            }
                            Some(KeyCode::PageUp) => {
                                s.help_scroll_y = (s.help_scroll_y - visible_h * 0.75).clamp(0.0, max_scroll);
                                s.window.request_redraw();
                                return;
                            }
                            Some(KeyCode::Home) => {
                                s.help_scroll_y = 0.0;
                                s.window.request_redraw();
                                return;
                            }
                            Some(KeyCode::End) => {
                                s.help_scroll_y = max_scroll;
                                s.window.request_redraw();
                                return;
                            }
                            _ => {}
                        }

                        if let Some('q' | 'Q') = logical_char {
                            s.help_visible = false;
                            s.help_dragging_scrollbar = false;
                            s.window.request_redraw();
                            return;
                        }

                        if s.ctrl_pressed
                            && s.shift_pressed
                            && (phys_code == Some(KeyCode::KeyH)
                                || logical_char == Some('h')
                                || logical_char == Some('H')
                                || logical_char == Some('\x08'))
                        {
                            s.help_visible = false;
                            s.help_dragging_scrollbar = false;
                            s.window.request_redraw();
                            return;
                        }

                        // Absorb other keys while help overlay is active so typing doesn't bleed into shell
                        return;
                    }

                    // 1. F11 Fullscreen toggle
                    if let Key::Named(NamedKey::F11) = &key_event.logical_key {
                        s.toggle_maximize();
                        return;
                    }
                    if phys_code == Some(KeyCode::F11) {
                        s.toggle_maximize();
                        return;
                    }

                    // 2. Alt + Arrow keys: Navigate between split panes
                    if s.alt_pressed {
                        match &key_event.logical_key {
                            Key::Named(NamedKey::ArrowLeft) => {
                                s.tab_manager.navigate_pane(pane::NavDirection::Left);
                                s.window.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::ArrowRight) => {
                                s.tab_manager.navigate_pane(pane::NavDirection::Right);
                                s.window.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::ArrowUp) => {
                                s.tab_manager.navigate_pane(pane::NavDirection::Up);
                                s.window.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::ArrowDown) => {
                                s.tab_manager.navigate_pane(pane::NavDirection::Down);
                                s.window.request_redraw();
                                return;
                            }
                            _ => {}
                        }
                    }

                    // 3. Ctrl + Shift shortcuts: Tabs, Splits, Help & Config management
                    if s.ctrl_pressed && s.shift_pressed {
                        // Ctrl+Shift+C: Explicit Copy selection to clipboard
                        if phys_code == Some(KeyCode::KeyC)
                            || logical_char == Some('c')
                            || logical_char == Some('C')
                            || logical_char == Some('\x03')
                        {
                            if let Some(pane) = s.tab_manager.active_pane_mut() {
                                if let Some(text) = pane.terminal.get_selected_text() {
                                    clipboard::set_text(&text);
                                }
                                pane.terminal.clear_selection();
                                s.window.request_redraw();
                            }
                            return;
                        }

                        // Ctrl+Shift+V: Explicit Paste from clipboard
                        if phys_code == Some(KeyCode::KeyV)
                            || logical_char == Some('v')
                            || logical_char == Some('V')
                            || logical_char == Some('\x16')
                        {
                            if let Some(pane) = s.tab_manager.active_pane_mut() {
                                if let Some(text) = clipboard::get_text() {
                                    let normalized = text.replace("\r\n", "\r").replace('\n', "\r");
                                    if pane.terminal.bracketed_paste {
                                        pane.pty.write_bytes(b"\x1b[200~");
                                        pane.pty.write_bytes(normalized.as_bytes());
                                        pane.pty.write_bytes(b"\x1b[201~");
                                    } else {
                                        pane.pty.write_bytes(normalized.as_bytes());
                                    }
                                    pane.terminal.scroll_to_bottom();
                                    s.drain_pty();
                                    s.window.request_redraw();
                                }
                            }
                            return;
                        }

                        // Ctrl+Shift+H or ? : Toggle Help
                        if phys_code == Some(KeyCode::KeyH)
                            || logical_char == Some('h')
                            || logical_char == Some('H')
                            || logical_char == Some('\x08')
                            || logical_char == Some('?')
                            || logical_char == Some('/')
                        {
                            s.toggle_help();
                            return;
                        }

                        // Ctrl+Shift+W: Close Active Pane / Tab
                        if phys_code == Some(KeyCode::KeyW)
                            || logical_char == Some('w')
                            || logical_char == Some('W')
                            || logical_char == Some('\x17')
                        {
                            let alive = s.tab_manager.close_active_pane(
                                content_w,
                                content_h,
                                &mut s.text_engine,
                                &s.config,
                            );
                            if !alive {
                                el.exit();
                            } else {
                                s.window.request_redraw();
                            }
                            return;
                        }

                        // Ctrl+Shift+T: New Tab
                        if phys_code == Some(KeyCode::KeyT)
                            || logical_char == Some('t')
                            || logical_char == Some('T')
                            || logical_char == Some('\x14')
                        {
                            let _ = s.tab_manager.new_tab(
                                content_w,
                                content_h,
                                &mut s.text_engine,
                                &s.config,
                            );
                            s.window.request_redraw();
                            return;
                        }

                        // Ctrl+Shift+E: Vertical Split (side-by-side)
                        if phys_code == Some(KeyCode::KeyE)
                            || logical_char == Some('e')
                            || logical_char == Some('E')
                            || logical_char == Some('\x05')
                        {
                            let _ = s.tab_manager.split_active_pane(
                                pane::SplitDirection::Vertical,
                                content_w,
                                content_h,
                                &mut s.text_engine,
                                &s.config,
                            );
                            s.window.request_redraw();
                            return;
                        }

                        // Ctrl+Shift+D: Horizontal Split (top/bottom)
                        if phys_code == Some(KeyCode::KeyD)
                            || logical_char == Some('d')
                            || logical_char == Some('D')
                            || logical_char == Some('\x04')
                        {
                            let _ = s.tab_manager.split_active_pane(
                                pane::SplitDirection::Horizontal,
                                content_w,
                                content_h,
                                &mut s.text_engine,
                                &s.config,
                            );
                            s.window.request_redraw();
                            return;
                        }

                        // Ctrl+Shift+R: Hot reload configuration (lunula.lua)
                        if phys_code == Some(KeyCode::KeyR)
                            || logical_char == Some('r')
                            || logical_char == Some('R')
                            || logical_char == Some('\x12')
                        {
                            s.reload_config();
                            return;
                        }

                        // Ctrl+Shift+P: Hot reload PowerShell profile (. $PROFILE)
                        if phys_code == Some(KeyCode::KeyP)
                            || logical_char == Some('p')
                            || logical_char == Some('P')
                            || logical_char == Some('\x10')
                        {
                            if let Some(pane) = s.tab_manager.active_pane_mut() {
                                pane.pty.write_bytes(b". $PROFILE\r\n");
                                pane.terminal.scroll_to_bottom();
                            }
                            return;
                        }
                    }

                    // 4. Ctrl + W (without shift) when multiple split panes exist: Close active split pane!
                    if s.ctrl_pressed && !s.shift_pressed {
                        if (phys_code == Some(KeyCode::KeyW)
                            || logical_char == Some('w')
                            || logical_char == Some('W')
                            || logical_char == Some('\x17'))
                            && s.tab_manager.active_tab().root.all_leaves().len() > 1
                        {
                            s.tab_manager.close_active_pane(
                                content_w,
                                content_h,
                                &mut s.text_engine,
                                &s.config,
                            );
                            s.window.request_redraw();
                            return;
                        }
                    }

                    // 4. Ctrl + Tab / Ctrl + 1..9: Tab switching
                    if s.ctrl_pressed {
                        match &key_event.logical_key {
                            Key::Named(NamedKey::Tab) => {
                                if s.shift_pressed {
                                    s.tab_manager.prev_tab(content_w, content_h, &mut s.text_engine, &s.config);
                                } else {
                                    s.tab_manager.next_tab(content_w, content_h, &mut s.text_engine, &s.config);
                                }
                                s.window.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::PageDown) => {
                                s.tab_manager.next_tab(content_w, content_h, &mut s.text_engine, &s.config);
                                s.window.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::PageUp) => {
                                s.tab_manager.prev_tab(content_w, content_h, &mut s.text_engine, &s.config);
                                s.window.request_redraw();
                                return;
                            }
                            Key::Character(s_str) => {
                                if let Some(d) = s_str.chars().next().and_then(|c| c.to_digit(10)) {
                                    if d >= 1 && d <= 9 {
                                        s.tab_manager.switch_tab((d - 1) as usize, content_w, content_h, &mut s.text_engine, &s.config);
                                        s.window.request_redraw();
                                        return;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }

                    // 5. Active pane terminal keystroke forwarding
                    if let Some(pane) = s.tab_manager.active_pane_mut() {
                        match &key_event.logical_key {
                            Key::Named(NamedKey::PageUp) => {
                                if s.shift_pressed {
                                    pane.terminal.scroll_up_view(pane.terminal.rows / 2);
                                } else {
                                    pane.pty.write_bytes(b"\x1b[5~");
                                }
                            }
                            Key::Named(NamedKey::PageDown) => {
                                if s.shift_pressed {
                                    pane.terminal.scroll_down_view(pane.terminal.rows / 2);
                                } else {
                                    pane.pty.write_bytes(b"\x1b[6~");
                                }
                            }
                            _ => {
                                pane.terminal.scroll_to_bottom();
                                match &key_event.logical_key {
                                    Key::Named(NamedKey::Space) => {
                                        pane.terminal.clear_selection();
                                        pane.pty.write_bytes(b" ");
                                    }
                                    Key::Named(NamedKey::Enter) => {
                                        pane.terminal.clear_selection();
                                        pane.pty.write_bytes(b"\r");
                                    }
                                    Key::Named(NamedKey::Backspace) => {
                                        pane.terminal.clear_selection();
                                        if s.ctrl_pressed {
                                            pane.pty.write_bytes(b"\x08"); // delete word back
                                        } else {
                                            pane.pty.write_bytes(b"\x7f"); // delete char back
                                        }
                                    }
                                    Key::Named(NamedKey::Tab) => {
                                        pane.terminal.clear_selection();
                                        pane.pty.write_bytes(b"\t");
                                    }
                                    Key::Named(NamedKey::Escape) => {
                                        pane.terminal.clear_selection();
                                        pane.pty.write_bytes(b"\x1b");
                                    }
                                    Key::Named(NamedKey::ArrowUp) => {
                                        if s.ctrl_pressed {
                                            pane.pty.write_bytes(b"\x1b[1;5A");
                                        } else {
                                            pane.pty.write_bytes(b"\x1b[A");
                                        }
                                    }
                                    Key::Named(NamedKey::ArrowDown) => {
                                        if s.ctrl_pressed {
                                            pane.pty.write_bytes(b"\x1b[1;5B");
                                        } else {
                                            pane.pty.write_bytes(b"\x1b[B");
                                        }
                                    }
                                    Key::Named(NamedKey::ArrowRight) => {
                                        if s.ctrl_pressed {
                                            pane.pty.write_bytes(b"\x1b[1;5C"); // jump word right
                                        } else {
                                            pane.pty.write_bytes(b"\x1b[C");
                                        }
                                    }
                                    Key::Named(NamedKey::ArrowLeft) => {
                                        if s.ctrl_pressed {
                                            pane.pty.write_bytes(b"\x1b[1;5D"); // jump word left
                                        } else {
                                            pane.pty.write_bytes(b"\x1b[D");
                                        }
                                    }
                                    Key::Named(NamedKey::Home) => {
                                        if s.ctrl_pressed {
                                            pane.pty.write_bytes(b"\x1b[1;5H");
                                        } else {
                                            pane.pty.write_bytes(b"\x1b[H");
                                        }
                                    }
                                    Key::Named(NamedKey::End) => {
                                        if s.ctrl_pressed {
                                            pane.pty.write_bytes(b"\x1b[1;5F");
                                        } else {
                                            pane.pty.write_bytes(b"\x1b[F");
                                        }
                                    }
                                    Key::Named(NamedKey::Delete) => {
                                        pane.pty.write_bytes(b"\x1b[3~");
                                    }
                                    Key::Named(NamedKey::Insert) => {
                                        if s.shift_pressed {
                                            if let Some(text) = clipboard::get_text() {
                                                let normalized =
                                                    text.replace("\r\n", "\r").replace('\n', "\r");
                                                if pane.terminal.bracketed_paste {
                                                    pane.pty.write_bytes(b"\x1b[200~");
                                                    pane.pty.write_bytes(normalized.as_bytes());
                                                    pane.pty.write_bytes(b"\x1b[201~");
                                                } else {
                                                    pane.pty.write_bytes(normalized.as_bytes());
                                                }
                                                pane.terminal.scroll_to_bottom();
                                            }
                                        } else {
                                            pane.pty.write_bytes(b"\x1b[2~");
                                        }
                                    }
                                    Key::Character(text_str) => {
                                        if s.ctrl_pressed {
                                            for ch in text_str.chars() {
                                                match ch {
                                                    'c' | 'C' | '\x03' => {
                                                        if pane.terminal.has_selection() {
                                                            if let Some(text) =
                                                                pane.terminal.get_selected_text()
                                                            {
                                                                clipboard::set_text(&text);
                                                            }
                                                            pane.terminal.clear_selection();
                                                        } else {
                                                            pane.pty.write_bytes(b"\x03");
                                                        }
                                                    }
                                                    'v' | 'V' | '\x16' => {
                                                        if let Some(text) = clipboard::get_text() {
                                                            let normalized = text
                                                                .replace("\r\n", "\r")
                                                                .replace('\n', "\r");
                                                            if pane.terminal.bracketed_paste {
                                                                pane.pty.write_bytes(b"\x1b[200~");
                                                                pane.pty.write_bytes(normalized.as_bytes());
                                                                pane.pty.write_bytes(b"\x1b[201~");
                                                            } else {
                                                                pane.pty.write_bytes(normalized.as_bytes());
                                                            }
                                                            pane.terminal.scroll_to_bottom();
                                                        }
                                                    }
                                                    'l' | 'L' | '\x0c' => pane.pty.write_bytes(b"\x0c"),
                                                    'd' | 'D' | '\x04' => pane.pty.write_bytes(b"\x04"),
                                                    'a' | 'A' | '\x01' => pane.pty.write_bytes(b"\x01"),
                                                    'z' | 'Z' | '\x1a' => pane.pty.write_bytes(b"\x1a"),
                                                    _ => {
                                                        if ch.is_ascii_alphabetic() {
                                                            let b = (ch.to_ascii_uppercase() as u8
                                                                - b'A')
                                                                + 1;
                                                            pane.pty.write_bytes(&[b]);
                                                        }
                                                    }
                                                }
                                            }
                                        } else {
                                            pane.terminal.clear_selection();
                                            pane.pty.write_bytes(text_str.as_bytes());
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }

                    // BRUTAL LOW-LATENCY MICRO-SPIN (up to 600µs):
                    // Catches PowerShell's local echo immediately in Frame 0!
                    let spin_start = Instant::now();
                    while spin_start.elapsed().as_micros() < 600 {
                        if s.drain_pty_raw() {
                            break;
                        }
                        std::hint::spin_loop();
                    }

                    s.drain_pty();
                    s.window.request_redraw();
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                s.mouse_pos = [position.x as f32, position.y as f32];

                // Update titlebar button hover glow & icons
                let hover = s.wobble.get_hover_button(s.mouse_pos[0], s.mouse_pos[1]);
                if (s.wobble.hover_button - hover).abs() > 0.1 {
                    s.wobble.hover_button = hover;
                    s.window.request_redraw();
                }

                if s.dragging {
                    let (cx, cy) = screen_cursor();
                    let dx = cx - s.drag_start_cursor.0;
                    let dy = cy - s.drag_start_cursor.1;
                    let new_pos = PhysicalPosition::new(
                        s.drag_start_win.x + dx,
                        s.drag_start_win.y + dy,
                    );
                    s.window.set_outer_position(new_pos);

                    let now = Instant::now();
                    let dt = (now - s.last_cursor_time).as_secs_f32().max(0.001);
                    let vel_x = (cx - s.last_cursor_pos.0) as f32 / dt;
                    let vel_y = (cy - s.last_cursor_pos.1) as f32 / dt;

                    s.last_cursor_pos = (cx, cy);
                    s.last_cursor_time = now;

                    s.wobble.inject_desktop_drag_velocity(vel_x, vel_y);
                    s.last_win_pos = new_pos;
                    s.window.request_redraw();
                } else if s.help_visible && s.help_dragging_scrollbar {
                    let content_h = s.wobble.content_size[1] as f32;
                    let card_h = 580.0f32.min(content_h - 24.0);
                    let card_y = (content_h - card_h) * 0.5;
                    let visible_h = card_h - 32.0;
                    let max_scroll = (s.help_total_height - visible_h).max(0.0);
                    let track_y = card_y + 40.0;
                    let track_h = card_h - 52.0;
                    let thumb_ratio = (visible_h / s.help_total_height).clamp(0.1, 0.9);
                    let thumb_h = (track_h * thumb_ratio).max(28.0);

                    if let Some([_cx, cy]) = s.wobble.screen_to_content_pos(s.mouse_pos[0], s.mouse_pos[1]) {
                        let pct = ((cy - track_y - thumb_h * 0.5) / (track_h - thumb_h).max(1.0)).clamp(0.0, 1.0);
                        s.help_scroll_y = pct * max_scroll;
                        s.window.request_redraw();
                    }
                } else if s.selecting {
                    if let Some([cx, cy]) =
                        s.wobble.screen_to_content_pos(s.mouse_pos[0], s.mouse_pos[1])
                    {
                        if let Some(pane) = s.tab_manager.active_pane_mut() {
                            let rel_x = cx - pane.rect[0] - pane::PANE_PAD;
                            let rel_y = cy - pane.rect[1] - pane::PANE_PAD;
                            let col = (rel_x / s.text_engine.cell_width).max(0.0) as usize;
                            let screen_row = (rel_y / s.text_engine.cell_height).max(0.0) as usize;
                            let col = col.min(pane.terminal.cols.saturating_sub(1));
                            let screen_row = screen_row.min(pane.terminal.rows.saturating_sub(1));
                            let abs_row = pane.terminal.screen_to_abs_row(screen_row);

                            if let Some(last_row) = pane.terminal.last_content_abs_row() {
                                let first_row = pane.terminal.first_content_abs_row().unwrap_or(0);
                                let clamped_row = abs_row.clamp(first_row, last_row);
                                let content_len = pane.terminal.abs_row_content_len(clamped_row);
                                let clamped_col = if content_len > 0 {
                                    col.min(content_len.saturating_sub(1))
                                } else {
                                    0
                                };

                                if let Some(sel) = &mut pane.terminal.selection {
                                    if sel.active.col != clamped_col || sel.active.abs_row != clamped_row {
                                        sel.active = terminal::SelectionPoint {
                                            col: clamped_col,
                                            abs_row: clamped_row,
                                        };
                                        pane.terminal.dirty = true;
                                        s.window.request_redraw();
                                    }
                                }
                            }
                        }
                    }
                }

                // Hyperlink cursor pointer check on hover when Ctrl is held
                if s.ctrl_pressed {
                    if let Some([cx, cy]) = s.wobble.screen_to_content_pos(s.mouse_pos[0], s.mouse_pos[1]) {
                        if let Some(pane) = s.tab_manager.active_pane() {
                            let rel_x = cx - pane.rect[0] - pane::PANE_PAD;
                            let rel_y = cy - pane.rect[1] - pane::PANE_PAD;
                            let col = (rel_x / s.text_engine.cell_width).max(0.0) as usize;
                            let screen_row = (rel_y / s.text_engine.cell_height).max(0.0) as usize;
                            if col < pane.terminal.cols && screen_row < pane.terminal.rows {
                                let abs_row = pane.terminal.screen_to_abs_row(screen_row);
                                if pane.terminal.get_hyperlink_at(col, abs_row).is_some() {
                                    s.window.set_cursor(winit::window::CursorIcon::Pointer);
                                } else {
                                    s.window.set_cursor(winit::window::CursorIcon::Default);
                                }
                            }
                        }
                    }
                } else {
                    let hover_btn = s.wobble.get_hover_button(s.mouse_pos[0], s.mouse_pos[1]);
                    if hover_btn > 0.5 {
                        s.window.set_cursor(winit::window::CursorIcon::Pointer);
                    } else if let Some([_cx, cy]) = s.wobble.screen_to_content_pos(s.mouse_pos[0], s.mouse_pos[1]) {
                        if s.tab_manager.tab_bar_visible(&s.config) && cy >= 42.0 && cy <= 42.0 + s.config.tabs.height {
                            s.window.set_cursor(winit::window::CursorIcon::Pointer);
                        } else {
                            s.window.set_cursor(winit::window::CursorIcon::Default);
                        }
                    }
                }
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
                                s.toggle_maximize();
                            }
                            WindowControlAction::Help => {
                                s.toggle_help();
                                return;
                            }
                            WindowControlAction::Settings => {
                                let config_file = if s.config_path.exists() {
                                    s.config_path.to_string_lossy().to_string()
                                } else {
                                    "lunula.lua".to_string()
                                };
                                let _ = std::process::Command::new("cmd")
                                    .args(["/c", "start", "", &config_file])
                                    .spawn();
                                return;
                            }
                            WindowControlAction::Drag => {
                                if !s.is_maximized {
                                    s.dragging = true;
                                    s.drag_start_cursor = screen_cursor();
                                    s.drag_start_win = s.window.outer_position().unwrap_or_default();
                                    s.last_win_pos = s.drag_start_win;
                                    s.last_cursor_pos = s.drag_start_cursor;
                                    s.last_cursor_time = Instant::now();
                                }
                            }
                            WindowControlAction::None => {
                                if let Some([cx, cy]) = s
                                    .wobble
                                    .screen_to_content_pos(s.mouse_pos[0], s.mouse_pos[1])
                                {
                                    let content_w = s.wobble.content_size[0] as f32;
                                    let content_h = s.wobble.content_size[1] as f32;

                                    // Check if Help modal is active
                                    if s.help_visible {
                                        let card_w = 660.0f32.min(content_w - 24.0);
                                        let card_h = 580.0f32.min(content_h - 24.0);
                                        let card_x = (content_w - card_w) * 0.5;
                                        let card_y = (content_h - card_h) * 0.5;
                                        let close_x = card_x + card_w - 32.0;
                                        let close_y = card_y + 8.0;

                                        let in_close = cx >= close_x && cx <= close_x + 24.0 && cy >= close_y && cy <= close_y + 24.0;
                                        let outside_card = cx < card_x || cx > card_x + card_w || cy < card_y || cy > card_y + card_h;
                                        if in_close || outside_card {
                                            s.help_visible = false;
                                            s.help_dragging_scrollbar = false;
                                            s.window.request_redraw();
                                            return;
                                        }

                                        // Check if clicked scrollbar track
                                        let visible_h = card_h - 32.0;
                                        let max_scroll = (s.help_total_height - visible_h).max(0.0);
                                        if max_scroll > 0.0 && cx >= card_x + card_w - 20.0 && cx <= card_x + card_w {
                                            let track_y = card_y + 40.0;
                                            let track_h = card_h - 52.0;
                                            let thumb_ratio = (visible_h / s.help_total_height).clamp(0.1, 0.9);
                                            let thumb_h = (track_h * thumb_ratio).max(28.0);

                                            s.help_dragging_scrollbar = true;
                                            let pct = ((cy - track_y - thumb_h * 0.5) / (track_h - thumb_h).max(1.0)).clamp(0.0, 1.0);
                                            s.help_scroll_y = pct * max_scroll;
                                            s.window.request_redraw();
                                        }
                                        return;
                                    }

                                    // 1. Check Tab Bar click (below titlebar at 42.0..74.0)
                                    if s.tab_manager.tab_bar_visible(&s.config)
                                        && cy >= 42.0 && cy < 42.0 + s.config.tabs.height
                                    {
                                        match s.tab_manager.handle_tab_bar_click(
                                            cx,
                                            cy,
                                            content_w,
                                            content_h,
                                            &mut s.text_engine,
                                            &s.config,
                                        ) {
                                            pane::TabBarClickResult::Handled => {
                                                s.window.request_redraw();
                                                return;
                                            }
                                            pane::TabBarClickResult::None => {}
                                        }
                                    }

                                    // 2. Pane click: close button [✕] or focus & selection start
                                    if let Some(action) = s.tab_manager.handle_pane_click_action(cx, cy) {
                                        match action {
                                            pane::PaneClickResult::ClosePane(pane_id) => {
                                                let alive = s.tab_manager.close_pane_by_id(
                                                    pane_id,
                                                    content_w,
                                                    content_h,
                                                    &mut s.text_engine,
                                                    &s.config,
                                                );
                                                if !alive {
                                                    el.exit();
                                                } else {
                                                    s.window.request_redraw();
                                                }
                                                return;
                                            }
                                            pane::PaneClickResult::Focused(clicked_id) => {
                                                if let Some(pane) = s.tab_manager.panes.get_mut(&clicked_id) {
                                                    let rel_x = cx - pane.rect[0] - pane::PANE_PAD;
                                                    let rel_y = cy - pane.rect[1] - pane::PANE_PAD;
                                                    let col = (rel_x / s.text_engine.cell_width).max(0.0) as usize;
                                                    let screen_row = (rel_y / s.text_engine.cell_height).max(0.0) as usize;
                                                    let col = col.min(pane.terminal.cols.saturating_sub(1));
                                                    let screen_row = screen_row.min(pane.terminal.rows.saturating_sub(1));
                                                    let abs_row = pane.terminal.screen_to_abs_row(screen_row);

                                                    // SGR Mouse forwarding to PTY if reporting is enabled and shift not pressed
                                                    if pane.terminal.mouse_reporting && !s.shift_pressed {
                                                        let seq = format!("\x1b[<0;{};{}M", col + 1, screen_row + 1);
                                                        pane.pty.write_bytes(seq.as_bytes());
                                                        s.drain_pty();
                                                        s.window.request_redraw();
                                                        return;
                                                    }

                                                    // Ctrl+Click: Open URL if clicked
                                                    if s.ctrl_pressed {
                                                        if let Some(url) = pane.terminal.get_hyperlink_at(col, abs_row) {
                                                            let _ = std::process::Command::new("cmd")
                                                                .args(["/c", "start", "", &url])
                                                                .spawn();
                                                            return;
                                                        }
                                                    }

                                                    let now = Instant::now();
                                                    let same_cell = s.last_click_cell == (col, screen_row);
                                                    if same_cell && (now - s.last_click_time).as_millis() < 400 {
                                                        s.click_streak = (s.click_streak % 3) + 1;
                                                    } else {
                                                        s.click_streak = 1;
                                                    }
                                                    s.last_click_time = now;
                                                    s.last_click_cell = (col, screen_row);

                                                    if s.click_streak == 2 {
                                                        pane.terminal.select_word_at(col, abs_row);
                                                        s.selecting = false;
                                                        s.window.request_redraw();
                                                        return;
                                                    } else if s.click_streak == 3 {
                                                        pane.terminal.select_line_at(abs_row);
                                                        s.selecting = false;
                                                        s.window.request_redraw();
                                                        return;
                                                    }

                                                    if let Some(last_row) = pane.terminal.last_content_abs_row() {
                                                        if abs_row <= last_row {
                                                            let content_len = pane.terminal.abs_row_content_len(abs_row);
                                                            let clamped_col = if content_len > 0 {
                                                                col.min(content_len.saturating_sub(1))
                                                            } else {
                                                                0
                                                            };
                                                            s.selecting = true;
                                                            pane.terminal.selection = Some(terminal::Selection {
                                                                anchor: terminal::SelectionPoint {
                                                                    col: clamped_col,
                                                                    abs_row,
                                                                },
                                                                active: terminal::SelectionPoint {
                                                                    col: clamped_col,
                                                                    abs_row,
                                                                },
                                                            });
                                                            pane.terminal.dirty = true;
                                                        } else {
                                                            s.selecting = false;
                                                            pane.terminal.clear_selection();
                                                        }
                                                    } else {
                                                        s.selecting = false;
                                                        pane.terminal.clear_selection();
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    if let Some(pane) = s.tab_manager.active_pane_mut() {
                                        pane.terminal.clear_selection();
                                    }
                                }
                            }
                        }
                    }
                    ElementState::Released => {
                        s.dragging = false;
                        s.help_dragging_scrollbar = false;
                        s.wobble.handle_mouse_up();

                        if let Some([cx, cy]) = s.wobble.screen_to_content_pos(s.mouse_pos[0], s.mouse_pos[1]) {
                            if let Some(pane) = s.tab_manager.active_pane_mut() {
                                if pane.terminal.mouse_reporting && !s.shift_pressed {
                                    let rel_x = cx - pane.rect[0] - pane::PANE_PAD;
                                    let rel_y = cy - pane.rect[1] - pane::PANE_PAD;
                                    let col = ((rel_x / s.text_engine.cell_width).max(0.0) as usize)
                                        .min(pane.terminal.cols.saturating_sub(1));
                                    let screen_row = ((rel_y / s.text_engine.cell_height).max(0.0) as usize)
                                        .min(pane.terminal.rows.saturating_sub(1));
                                    let seq = format!("\x1b[<0;{};{}m", col + 1, screen_row + 1);
                                    pane.pty.write_bytes(seq.as_bytes());
                                    s.drain_pty();
                                    s.window.request_redraw();
                                }
                            }
                        }

                        if s.selecting {
                            s.selecting = false;
                            if let Some(pane) = s.tab_manager.active_pane_mut() {
                                if let Some(sel) = &pane.terminal.selection {
                                    if sel.is_empty() {
                                        pane.terminal.selection = None;
                                        pane.terminal.dirty = true;
                                    }
                                }
                            }
                        }
                    }
                }
                s.window.request_redraw();
            }

            WindowEvent::MouseInput {
                button: MouseButton::Right,
                state: ElementState::Pressed,
                ..
            } => {
                if let Some(pane) = s.tab_manager.active_pane_mut() {
                    if pane.terminal.mouse_reporting && !s.shift_pressed {
                        if let Some([cx, cy]) = s.wobble.screen_to_content_pos(s.mouse_pos[0], s.mouse_pos[1]) {
                            let rel_x = cx - pane.rect[0] - pane::PANE_PAD;
                            let rel_y = cy - pane.rect[1] - pane::PANE_PAD;
                            let col = ((rel_x / s.text_engine.cell_width).max(0.0) as usize)
                                .min(pane.terminal.cols.saturating_sub(1));
                            let screen_row = ((rel_y / s.text_engine.cell_height).max(0.0) as usize)
                                .min(pane.terminal.rows.saturating_sub(1));
                            let seq = format!("\x1b[<2;{};{}M", col + 1, screen_row + 1);
                            pane.pty.write_bytes(seq.as_bytes());
                            s.drain_pty();
                            s.window.request_redraw();
                            return;
                        }
                    }

                    if pane.terminal.has_selection() {
                        if let Some(text) = pane.terminal.get_selected_text() {
                            clipboard::set_text(&text);
                        }
                        pane.terminal.clear_selection();
                        s.window.request_redraw();
                    } else {
                        // Paste from clipboard!
                        if let Some(text) = clipboard::get_text() {
                            let normalized = text.replace("\r\n", "\r").replace('\n', "\r");
                            if pane.terminal.bracketed_paste {
                                pane.pty.write_bytes(b"\x1b[200~");
                                pane.pty.write_bytes(normalized.as_bytes());
                                pane.pty.write_bytes(b"\x1b[201~");
                            } else {
                                pane.pty.write_bytes(normalized.as_bytes());
                            }
                            pane.terminal.scroll_to_bottom();
                            s.drain_pty();
                            s.window.request_redraw();
                        }
                    }
                }
            }

            WindowEvent::RedrawRequested => {
                s.render();
            }
            _ => {}
        }
    }
}

fn main() {
    env_logger::init();
    let config = config::LunulaConfig::load();
    let el = EventLoop::new().expect("event loop");
    let proxy = el.create_proxy();
    el.set_control_flow(ControlFlow::Wait);
    let mut app = LunulaApp {
        config,
        proxy,
        state: None,
    };
    el.run_app(&mut app).expect("run");
}
