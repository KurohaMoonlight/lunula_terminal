//! Individual terminal `Pane` state, elastic cursor trail physics, and row-cached text shaping.
//!
//! Optimization highlights:
//! - Separates `drain_pty_bytes()` (which runs on all panes including background tabs to prevent
//!   unbounded channel memory growth) from `sync_render_cache()` (which shapes glyphs only for
//!   currently visible panes).
//! - Caches `last_shaped_base_offset: Option<usize>` so sub-pixel smooth scrolling shifts
//!   `scroll_subpixel_y` on the GPU without re-running HarfBuzz shaping on every frame.

use crate::config::LunulaConfig;
use crate::pty::PtyManager;
use crate::render::TextEngine;
use crate::terminal::{self, BgRect, TerminalGrid};
use glyphon::Buffer;
use std::time::Instant;
use winit::event_loop::EventLoopProxy;

pub const PANE_PAD: f32 = 14.0;

pub struct Pane {
    #[allow(dead_code)]
    pub id: usize,
    pub pty: PtyManager,
    pub terminal: TerminalGrid,
    pub buffer: Buffer,
    pub cached_bg_rects: Vec<BgRect>,
    pub rect: [f32; 4],
    pub title: String,
    pub cursor_head: [f32; 2],
    pub cursor_tail: [f32; 2],
    pub cursor_head_vel: [f32; 2],
    pub cursor_tail_vel: [f32; 2],
    pub cursor_blink_time: f32,
    pub cursor_last_moved: Instant,
    pub prev_cursor_grid: (usize, usize),
    pub last_shaped_base_offset: Option<usize>,
    pub scroll_subpixel_y: f32,
}

impl Pane {
    pub fn new(
        id: usize,
        cols: u16,
        rows: u16,
        text_engine: &mut TextEngine,
        proxy: EventLoopProxy<()>,
        config: &LunulaConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut terminal = TerminalGrid::new(cols as usize, rows as usize);
        terminal.cell_height = text_engine.cell_height;
        terminal.max_scrollback = config.terminal.max_scrollback;
        terminal.selection_color = terminal::ColorRgb {
            r: config.colors.selection[0],
            g: config.colors.selection[1],
            b: config.colors.selection[2],
        };
        terminal.cursor_config.shape = match config.cursor.shape.as_str() {
            "beam" => terminal::CursorShape::Beam,
            "underline" => terminal::CursorShape::Underline,
            _ => terminal::CursorShape::Block,
        };
        terminal.cursor_config.blinking = config.cursor.blinking;
        terminal.cursor_config.color = terminal::ColorRgb {
            r: config.cursor.color[0],
            g: config.cursor.color[1],
            b: config.cursor.color[2],
        };

        let pty = PtyManager::new(cols, rows, proxy)?;
        let buffer = text_engine.create_buffer();

        Ok(Self {
            id,
            pty,
            terminal,
            buffer,
            cached_bg_rects: Vec::new(),
            rect: [0.0, 0.0, 100.0, 100.0],
            title: "pwsh".to_string(),
            cursor_head: [PANE_PAD, PANE_PAD],
            cursor_tail: [PANE_PAD, PANE_PAD],
            cursor_head_vel: [0.0, 0.0],
            cursor_tail_vel: [0.0, 0.0],
            cursor_blink_time: 0.0,
            cursor_last_moved: Instant::now(),
            prev_cursor_grid: (0, 0),
            last_shaped_base_offset: None,
            scroll_subpixel_y: 0.0,
        })
    }

    pub fn resize(&mut self, new_cols: u16, new_rows: u16, cell_h: f32) {
        self.terminal.cell_height = cell_h.max(1.0);
        if new_cols > 0 && new_rows > 0 {
            let changed = new_cols as usize != self.terminal.cols
                || new_rows as usize != self.terminal.rows;
            if changed {
                self.terminal.resize(new_cols as usize, new_rows as usize);
                self.pty.resize(new_cols, new_rows);
                self.last_shaped_base_offset = None;
            }
        }
    }

    pub fn update_physics(&mut self, dt: f32, config: &LunulaConfig, cell_w: f32, cell_h: f32) {
        let cur_grid = (self.terminal.cursor_x, self.terminal.cursor_y);
        let now = Instant::now();
        if cur_grid != self.prev_cursor_grid {
            self.cursor_last_moved = now;
            self.cursor_blink_time = 0.0;
            self.prev_cursor_grid = cur_grid;
            self.terminal.dirty = true;
        }
        self.cursor_blink_time += dt;

        let target_x = self.rect[0] + PANE_PAD + self.terminal.cursor_x as f32 * cell_w;
        let target_y = self.rect[1] + PANE_PAD + self.terminal.cursor_y as f32 * cell_h;

        let dx = target_x - self.cursor_head[0];
        let dy = target_y - self.cursor_head[1];
        let tdx = self.cursor_head[0] - self.cursor_tail[0];
        let tdy = self.cursor_head[1] - self.cursor_tail[1];

        // Snap directly to target when already settled to avoid micro-oscillations
        if dx.abs() < 0.05
            && dy.abs() < 0.05
            && tdx.abs() < 0.05
            && tdy.abs() < 0.05
            && self.cursor_head_vel[0].abs() < 0.1
            && self.cursor_head_vel[1].abs() < 0.1
        {
            self.cursor_head = [target_x, target_y];
            self.cursor_tail = [target_x, target_y];
            self.cursor_head_vel = [0.0, 0.0];
            self.cursor_tail_vel = [0.0, 0.0];
            return;
        }

        let k_head = config.cursor.trail_stiffness;
        let d_head = config.cursor.trail_damping;
        self.cursor_head_vel[0] =
            (self.cursor_head_vel[0] + dx * k_head * dt) * (1.0 - d_head * dt).max(0.0);
        self.cursor_head_vel[1] =
            (self.cursor_head_vel[1] + dy * k_head * dt) * (1.0 - d_head * dt).max(0.0);
        self.cursor_head[0] += self.cursor_head_vel[0] * dt;
        self.cursor_head[1] += self.cursor_head_vel[1] * dt;

        let k_tail = k_head * 0.45;
        let d_tail = d_head * 0.70;
        let tdx = self.cursor_head[0] - self.cursor_tail[0];
        let tdy = self.cursor_head[1] - self.cursor_tail[1];
        self.cursor_tail_vel[0] =
            (self.cursor_tail_vel[0] + tdx * k_tail * dt) * (1.0 - d_tail * dt).max(0.0);
        self.cursor_tail_vel[1] =
            (self.cursor_tail_vel[1] + tdy * k_tail * dt) * (1.0 - d_tail * dt).max(0.0);
        self.cursor_tail[0] += self.cursor_tail_vel[0] * dt;
        self.cursor_tail[1] += self.cursor_tail_vel[1] * dt;
    }

    /// Drains incoming PTY bytes into `TerminalGrid` without running HarfBuzz shaping.
    pub fn drain_pty_bytes(&mut self) -> bool {
        self.pty.clear_wake_flag();
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
        received
    }

    /// Shapes `glyphon::Buffer` and builds `cached_bg_rects` only when content or row offset changed.
    pub fn sync_render_cache(&mut self, text_engine: &mut TextEngine) -> bool {
        let cell_h = text_engine.cell_height.max(1.0);
        self.terminal.cell_height = cell_h;

        let base_offset = if self.terminal.current_scroll_y > 0.0 {
            (self.terminal.current_scroll_y / cell_h).floor() as usize
        } else {
            self.terminal.scroll_offset
        };

        self.scroll_subpixel_y = if self.terminal.current_scroll_y > 0.0 {
            self.terminal.current_scroll_y % cell_h
        } else {
            0.0
        };

        if self.terminal.dirty || self.last_shaped_base_offset != Some(base_offset) {
            let (spans, bg_rects) = self.terminal.render_spans_and_backgrounds(
                text_engine.cell_width,
                text_engine.cell_height,
                PANE_PAD,
                PANE_PAD,
            );
            text_engine.shape_buffer(&mut self.buffer, &spans);
            self.cached_bg_rects = bg_rects;
            self.terminal.dirty = false;
            self.last_shaped_base_offset = Some(base_offset);
            true
        } else {
            false
        }
    }
}
