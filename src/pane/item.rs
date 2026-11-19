//! Individual terminal `Pane` state, elastic cursor trail physics, and row-cached text shaping.
//!
//! Optimization highlights:
//! - Separates `drain_pty_bytes()` (which runs on all panes including background tabs to prevent
//!   unbounded channel memory growth) from `sync_render_cache()` (which shapes glyphs only for
//!   currently visible panes).
//! - Caches `last_shaped_base_offset: Option<usize>` so sub-pixel smooth scrolling shifts
//!   `scroll_subpixel_y` on the GPU without re-running HarfBuzz shaping on every frame.

use super::vi_mode::ViMode;
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
    pub cached_row_bg_rects: Vec<Vec<BgRect>>,
    pub vi_mode: Option<ViMode>,
    pub rect: [f32; 4],
    pub title: String,
    pub cursor_head: [f32; 2],
    pub cursor_tail: [f32; 2],
    pub cursor_head_vel: [f32; 2],
    pub cursor_tail_vel: [f32; 2],
    pub cursor_blink_time: f32,
    pub cursor_last_moved: Instant,
    pub prev_cursor_grid: (usize, usize),
    pub prev_cursor_visible: bool,
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

        let pty = PtyManager::new(cols, rows, proxy, &config.shell)?;
        let buffer = text_engine.create_buffer();

        Ok(Self {
            id,
            pty,
            terminal,
            buffer,
            cached_bg_rects: Vec::new(),
            cached_row_bg_rects: vec![Vec::new(); rows as usize],
            vi_mode: None,
            rect: [0.0, 0.0, 100.0, 100.0],
            title: "pwsh".to_string(),
            cursor_head: [PANE_PAD, PANE_PAD],
            cursor_tail: [PANE_PAD, PANE_PAD],
            cursor_head_vel: [0.0, 0.0],
            cursor_tail_vel: [0.0, 0.0],
            cursor_blink_time: 0.0,
            cursor_last_moved: Instant::now(),
            prev_cursor_grid: (0, 0),
            prev_cursor_visible: true,
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
                self.cached_row_bg_rects = vec![Vec::new(); new_rows as usize];
                self.last_shaped_base_offset = None;
            }
        }
    }

    pub fn update_physics(&mut self, dt: f32, config: &LunulaConfig, cell_w: f32, cell_h: f32) {
        let is_visible = self.terminal.cursor_visible;
        let cur_grid = (self.terminal.cursor_x, self.terminal.cursor_y);
        let now = Instant::now();

        let target_x = self.rect[0] + PANE_PAD + self.terminal.cursor_x as f32 * cell_w;
        let target_y = self.rect[1] + PANE_PAD + self.terminal.cursor_y as f32 * cell_h;

        // When terminal cursor is hidden (e.g. during TUI full-frame redraws in luna/vim/nano),
        // snap instantly so intermediate pen coordinates never pull the physics spring.
        if !is_visible {
            self.cursor_head = [target_x, target_y];
            self.cursor_tail = [target_x, target_y];
            self.cursor_head_vel = [0.0, 0.0];
            self.cursor_tail_vel = [0.0, 0.0];
            self.prev_cursor_grid = cur_grid;
            self.prev_cursor_visible = false;
            return;
        }

        let just_became_visible = !self.prev_cursor_visible && is_visible;
        self.prev_cursor_visible = true;

        let grid_dx = (cur_grid.0 as isize - self.prev_cursor_grid.0 as isize).abs();
        let grid_dy = (cur_grid.1 as isize - self.prev_cursor_grid.1 as isize).abs();

        if cur_grid != self.prev_cursor_grid {
            self.cursor_last_moved = now;
            self.cursor_blink_time = 0.0;
            self.terminal.dirty = true;

            // When moving between lines, snap the horizontal tail immediately to head so
            // no giant multi-line bounding box smear flashes across the screen.
            if grid_dy > 0 {
                self.cursor_tail[0] = self.cursor_head[0];
                self.cursor_tail_vel[0] = 0.0;
            }

            // Snap immediately on large jumps (e.g. PageDown, buffer jump, or unhiding far away)
            if grid_dy > 3 || grid_dx > 40 || (just_became_visible && (grid_dy > 1 || grid_dx > 10)) {
                self.cursor_head = [target_x, target_y];
                self.cursor_tail = [target_x, target_y];
                self.cursor_head_vel = [0.0, 0.0];
                self.cursor_tail_vel = [0.0, 0.0];
                self.prev_cursor_grid = cur_grid;
                return;
            }

            self.prev_cursor_grid = cur_grid;
        }
        self.cursor_blink_time += dt;

        let dx = target_x - self.cursor_head[0];
        let dy = target_y - self.cursor_head[1];
        let tdx = self.cursor_head[0] - self.cursor_tail[0];

        // Settle cleanly when close to target to eliminate micro-vibrations
        if dx.abs() < 0.2
            && dy.abs() < 0.2
            && tdx.abs() < 0.2
            && self.cursor_head_vel[0].abs() < 1.0
            && self.cursor_head_vel[1].abs() < 1.0
        {
            self.cursor_head = [target_x, target_y];
            self.cursor_tail = [target_x, target_y];
            self.cursor_head_vel = [0.0, 0.0];
            self.cursor_tail_vel = [0.0, 0.0];
            return;
        }

        // Substepping semi-implicit Euler integration for unconditional stability and zero oscillation.
        let k_head_x = config.cursor.trail_stiffness.clamp(100.0, 1000.0);
        let d_head_x = config.cursor.trail_damping.max(2.0 * k_head_x.sqrt() * 0.95);

        // Vertical movement is overdamped (d_y >= 2*sqrt(k_y)) to completely prevent row bounce/overshoot
        let k_head_y = 650.0;
        let d_head_y = 55.0; // 2 * sqrt(650) ~ 51.0, 55.0 is critically damped/overdamped

        let k_tail_x = k_head_x * 0.45;
        let d_tail_x = d_head_x * 0.85;

        // Substep physics to ensure numerical stability at any frame rate (e.g. 30 FPS, 60 FPS, 144 FPS)
        let mut remaining = dt.clamp(0.001, 0.05);
        let max_substep = 0.008; // 125 Hz substeps

        while remaining > 0.0 {
            let sub_dt = remaining.min(max_substep);
            remaining -= sub_dt;

            // Head X (Semi-implicit Euler with division damping factor)
            let damp_x = 1.0 / (1.0 + d_head_x * sub_dt);
            let cur_dx = target_x - self.cursor_head[0];
            self.cursor_head_vel[0] = (self.cursor_head_vel[0] + cur_dx * k_head_x * sub_dt) * damp_x;
            self.cursor_head[0] += self.cursor_head_vel[0] * sub_dt;

            // Head Y (Overdamped for smooth, non-oscillating line movement)
            let damp_y = 1.0 / (1.0 + d_head_y * sub_dt);
            let cur_dy = target_y - self.cursor_head[1];
            self.cursor_head_vel[1] = (self.cursor_head_vel[1] + cur_dy * k_head_y * sub_dt) * damp_y;
            self.cursor_head[1] += self.cursor_head_vel[1] * sub_dt;

            // Tail X
            let cur_tdx = self.cursor_head[0] - self.cursor_tail[0];
            let damp_tail_x = 1.0 / (1.0 + d_tail_x * sub_dt);
            self.cursor_tail_vel[0] = (self.cursor_tail_vel[0] + cur_tdx * k_tail_x * sub_dt) * damp_tail_x;
            self.cursor_tail[0] += self.cursor_tail_vel[0] * sub_dt;

            // Tail Y follows Head Y directly so the trail smear is strictly horizontal
            self.cursor_tail[1] = self.cursor_head[1];
            self.cursor_tail_vel[1] = self.cursor_head_vel[1];
        }
    }

    /// Drains incoming PTY bytes into `TerminalGrid` without running HarfBuzz shaping.
    pub fn drain_pty_bytes(&mut self) -> bool {
        self.pty.clear_wake_flag();
        let mut received = false;
        while let Ok(bytes) = self.pty.rx.try_recv() {
            self.terminal.advance_bytes(&bytes);
            received = true;
        }

        // Process binary-safe IPC packets (e.g. Kitty Graphics from limg) strictly after
        // pending ConPTY output has updated cursor_x/cursor_y, then ACK the sender.
        while let Ok((ipc_bytes, ack_tx)) = self.pty.ipc_rx.try_recv() {
            self.terminal.handle_ipc_bytes(&ipc_bytes);
            let _ = ack_tx.send(());
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
        // DEC Private Mode ?2026: Synchronized Update (BSU/ESU)
        if self.terminal.is_sync_update_active() {
            return false;
        }

        let cell_h = text_engine.cell_height.max(1.0);
        let cell_w = text_engine.cell_width;
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

        if self.last_shaped_base_offset != Some(base_offset) {
            self.terminal.mark_all_rows_dirty();
            self.last_shaped_base_offset = Some(base_offset);
        }

        if self.terminal.dirty {
            let rows = self.terminal.rows;
            if self.cached_row_bg_rects.len() != rows {
                self.cached_row_bg_rects = vec![Vec::new(); rows];
                self.terminal.mark_all_rows_dirty();
            }

            let family = glyphon::Family::Name(&text_engine.font_family);
            let default_attrs = glyphon::Attrs::new()
                .family(family)
                .color(glyphon::Color::rgb(220, 215, 186));

            while self.buffer.lines.len() < rows {
                let idx = self.buffer.lines.len();
                let is_last = idx == rows - 1;
                let ending = if is_last {
                    glyphon::cosmic_text::LineEnding::None
                } else {
                    glyphon::cosmic_text::LineEnding::Lf
                };
                self.buffer.lines.push(glyphon::cosmic_text::BufferLine::new(
                    "",
                    ending,
                    glyphon::cosmic_text::AttrsList::new(&default_attrs),
                    glyphon::Shaping::Advanced,
                ));
            }
            if self.buffer.lines.len() > rows {
                self.buffer.lines.truncate(rows);
            }

            // Update only rows marked dirty in TerminalGrid's row dirty bitmask
            for r in 0..rows {
                if self.terminal.is_row_dirty(r) {
                    let (row_spans, row_bg) = self.terminal.render_row(
                        r,
                        base_offset,
                        cell_w,
                        cell_h,
                        PANE_PAD,
                        PANE_PAD,
                    );
                    let is_last = r == rows - 1;
                    text_engine.update_buffer_line(&mut self.buffer, r, &row_spans, is_last);
                    self.cached_row_bg_rects[r] = row_bg;
                }
            }

            // HarfBuzz will only shape lines that were marked dirty and updated!
            self.buffer.shape_until_scroll(&mut text_engine.font_system, false);

            let total_bg = self.cached_row_bg_rects.iter().map(|v| v.len()).sum();
            let mut bg_rects = Vec::with_capacity(total_bg);
            for row_bg in &self.cached_row_bg_rects {
                bg_rects.extend_from_slice(row_bg);
            }
            self.cached_bg_rects = bg_rects;

            self.terminal.clear_dirty_rows();
            true
        } else {
            false
        }
    }
}
