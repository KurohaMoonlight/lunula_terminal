//! Mouse wheel scrolling, cursor hover/drag physics, text selection, and hyperlink clicking.

use super::state::GpuState;
use crate::pane;
use crate::platform::{clipboard, screen_cursor};
use crate::terminal;
use crate::wobble::WindowControlAction;
use std::time::Instant;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseScrollDelta};
use winit::event_loop::ActiveEventLoop;

impl GpuState {
    pub fn handle_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        let cell_h = self.text_engine.cell_height;
        let delta_y = match delta {
            MouseScrollDelta::LineDelta(_x, y) => y * cell_h * 3.0,
            MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
        };

        if self.help_visible {
            let content_h = self.wobble.content_size[1] as f32;
            let card_h = super::HELP_MODAL_HEIGHT.min(content_h - 24.0);
            let visible_h = card_h - 32.0;
            let max_scroll = (self.help_total_height - visible_h).max(0.0);
            self.help_scroll_y = (self.help_scroll_y - delta_y).clamp(0.0, max_scroll);
            self.window.request_redraw();
            return;
        }

        if let Some(pane) = self.tab_manager.active_pane_mut() {
            if pane.terminal.mouse_reporting && !self.shift_pressed {
                if let Some([cx, cy]) = self
                    .wobble
                    .screen_to_content_pos(self.mouse_pos[0], self.mouse_pos[1])
                {
                    let rel_x = cx - pane.rect[0] - pane::PANE_PAD;
                    let rel_y = cy - pane.rect[1] - pane::PANE_PAD;
                    let col = ((rel_x / self.text_engine.cell_width).max(0.0) as usize)
                        .min(pane.terminal.cols.saturating_sub(1));
                    let screen_row = ((rel_y / self.text_engine.cell_height).max(0.0) as usize)
                        .min(pane.terminal.rows.saturating_sub(1));
                    let btn = if delta_y > 0.0 { 64 } else { 65 };
                    let seq = format!("\x1b[<{};{};{}M", btn, col + 1, screen_row + 1);
                    pane.pty.write_bytes(seq.as_bytes());
                    self.drain_pty();
                    self.window.request_redraw();
                    return;
                }
            }

            if self.config.scroll.smooth {
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
        self.window.request_redraw();
    }

    pub fn handle_cursor_moved(&mut self, position: PhysicalPosition<f64>) {
        self.mouse_pos = [position.x as f32, position.y as f32];

        let hover = self
            .wobble
            .get_hover_button(self.mouse_pos[0], self.mouse_pos[1]);
        if (self.wobble.hover_button - hover).abs() > 0.1 {
            self.wobble.hover_button = hover;
            self.window.request_redraw();
        }

        if self.dragging {
            let (cx, cy) = screen_cursor();
            let dx = cx - self.drag_start_cursor.0;
            let dy = cy - self.drag_start_cursor.1;
            let new_pos =
                PhysicalPosition::new(self.drag_start_win.x + dx, self.drag_start_win.y + dy);
            self.window.set_outer_position(new_pos);

            let now = Instant::now();
            let dt = (now - self.last_cursor_time).as_secs_f32().max(0.001);
            let vel_x = (cx - self.last_cursor_pos.0) as f32 / dt;
            let vel_y = (cy - self.last_cursor_pos.1) as f32 / dt;

            self.last_cursor_pos = (cx, cy);
            self.last_cursor_time = now;

            self.wobble.inject_desktop_drag_velocity(vel_x, vel_y);
            self.last_win_pos = new_pos;
            self.window.request_redraw();
        } else if self.help_visible && self.help_dragging_scrollbar {
            let content_h = self.wobble.content_size[1] as f32;
            let card_h = super::HELP_MODAL_HEIGHT.min(content_h - 24.0);
            let card_y = (content_h - card_h) * 0.5;
            let visible_h = card_h - 32.0;
            let max_scroll = (self.help_total_height - visible_h).max(0.0);
            let track_y = card_y + 40.0;
            let track_h = card_h - 52.0;
            let thumb_ratio = (visible_h / self.help_total_height).clamp(0.1, 0.9);
            let thumb_h = (track_h * thumb_ratio).max(28.0);

            if let Some([_cx, cy]) = self
                .wobble
                .screen_to_content_pos(self.mouse_pos[0], self.mouse_pos[1])
            {
                let pct = ((cy - track_y - thumb_h * 0.5) / (track_h - thumb_h).max(1.0))
                    .clamp(0.0, 1.0);
                self.help_scroll_y = pct * max_scroll;
                self.window.request_redraw();
            }
        } else if self.selecting {
            if let Some([cx, cy]) = self
                .wobble
                .screen_to_content_pos(self.mouse_pos[0], self.mouse_pos[1])
            {
                if let Some(pane) = self.tab_manager.active_pane_mut() {
                    let rel_x = cx - pane.rect[0] - pane::PANE_PAD;
                    let rel_y = cy - pane.rect[1] - pane::PANE_PAD;
                    let col = (rel_x / self.text_engine.cell_width).max(0.0) as usize;
                    let screen_row = (rel_y / self.text_engine.cell_height).max(0.0) as usize;
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
                                self.window.request_redraw();
                            }
                        }
                    }
                }
            }
        }

        if self.mouse_left_down {
            if let Some([cx, cy]) = self
                .wobble
                .screen_to_content_pos(self.mouse_pos[0], self.mouse_pos[1])
            {
                if let Some(pane) = self.tab_manager.active_pane_mut() {
                    if pane.terminal.mouse_reporting && !self.shift_pressed {
                        let rel_x = cx - pane.rect[0] - pane::PANE_PAD;
                        let rel_y = cy - pane.rect[1] - pane::PANE_PAD;
                        if rel_x >= 0.0 && rel_y >= 0.0 {
                            let col = ((rel_x / self.text_engine.cell_width).max(0.0) as usize)
                                .min(pane.terminal.cols.saturating_sub(1));
                            let screen_row =
                                ((rel_y / self.text_engine.cell_height).max(0.0) as usize)
                                    .min(pane.terminal.rows.saturating_sub(1));
                            if self.last_drag_cell != Some((col, screen_row)) {
                                self.last_drag_cell = Some((col, screen_row));
                                let seq = format!("\x1b[<32;{};{}M", col + 1, screen_row + 1);
                                pane.pty.write_bytes(seq.as_bytes());
                                self.drain_pty();
                                self.window.request_redraw();
                            }
                        }
                    }
                }
            }
        }

        if self.ctrl_pressed {
            if let Some([cx, cy]) = self
                .wobble
                .screen_to_content_pos(self.mouse_pos[0], self.mouse_pos[1])
            {
                if let Some(pane) = self.tab_manager.active_pane() {
                    let rel_x = cx - pane.rect[0] - pane::PANE_PAD;
                    let rel_y = cy - pane.rect[1] - pane::PANE_PAD;
                    let col = (rel_x / self.text_engine.cell_width).max(0.0) as usize;
                    let screen_row = (rel_y / self.text_engine.cell_height).max(0.0) as usize;
                    if col < pane.terminal.cols && screen_row < pane.terminal.rows {
                        let abs_row = pane.terminal.screen_to_abs_row(screen_row);
                        if let Some(url) = pane.terminal.get_hyperlink_at(col, abs_row) {
                            if crate::platform::is_safe_url(&url) {
                                self.window.set_cursor(winit::window::CursorIcon::Pointer);
                            } else {
                                self.window.set_cursor(winit::window::CursorIcon::Default);
                            }
                        } else {
                            self.window.set_cursor(winit::window::CursorIcon::Default);
                        }
                    }
                }
            }
        } else {
            let hover_btn = self
                .wobble
                .get_hover_button(self.mouse_pos[0], self.mouse_pos[1]);
            if hover_btn > 0.5 {
                self.window.set_cursor(winit::window::CursorIcon::Pointer);
            } else if let Some([_cx, cy]) = self
                .wobble
                .screen_to_content_pos(self.mouse_pos[0], self.mouse_pos[1])
            {
                if self.tab_manager.tab_bar_visible(&self.config)
                    && cy >= 48.0
                    && cy <= 48.0 + self.config.tabs.height
                {
                    self.window.set_cursor(winit::window::CursorIcon::Pointer);
                } else {
                    self.window.set_cursor(winit::window::CursorIcon::Default);
                }
            }
        }
    }

    pub fn handle_mouse_left(&mut self, el: &ActiveEventLoop, bs: ElementState) {
        self.check_background_metadata();

        match bs {
            ElementState::Pressed => {
                self.mouse_left_down = true;
                let action = self
                    .wobble
                    .handle_mouse_click(self.mouse_pos[0], self.mouse_pos[1]);
                match action {
                    WindowControlAction::Close => el.exit(),
                    WindowControlAction::Minimize => self.window.set_minimized(true),
                    WindowControlAction::Maximize => {
                        self.toggle_maximize();
                    }
                    WindowControlAction::Help => {
                        self.toggle_help();
                        return;
                    }
                    WindowControlAction::Settings => {
                        self.toggle_settings();
                        return;
                    }
                    WindowControlAction::Drag => {
                        if !self.is_maximized {
                            self.dragging = true;
                            self.drag_start_cursor = screen_cursor();
                            self.drag_start_win = self.window.outer_position().unwrap_or_default();
                            self.last_win_pos = self.drag_start_win;
                            self.last_cursor_pos = self.drag_start_cursor;
                            self.last_cursor_time = Instant::now();
                        }
                    }
                    WindowControlAction::None => {
                        if let Some([cx, cy]) = self
                            .wobble
                            .screen_to_content_pos(self.mouse_pos[0], self.mouse_pos[1])
                        {
                            let content_w = self.wobble.content_size[0] as f32;
                            let content_h = self.wobble.content_size[1] as f32;

                            if self.settings_visible {
                                let card_w = super::SETTINGS_MODAL_WIDTH.min(content_w - 24.0);
                                let card_h = super::SETTINGS_MODAL_HEIGHT.min(content_h - 24.0);
                                let card_x = (content_w - card_w) * 0.5;
                                let card_y = (content_h - card_h) * 0.5;
                                let close_x = card_x + card_w - 32.0;
                                let close_y = card_y + 8.0;

                                let in_close = cx >= close_x
                                    && cx <= close_x + 24.0
                                    && cy >= close_y
                                    && cy <= close_y + 24.0;
                                let outside_card = cx < card_x
                                    || cx > card_x + card_w
                                    || cy < card_y
                                    || cy > card_y + card_h;

                                if in_close || outside_card {
                                    self.settings_visible = false;
                                    self.window.request_redraw();
                                    return;
                                }

                                let line_h = self.text_engine.line_height;
                                let item_start_y = card_y + 16.0 + 4.0 * line_h;
                                if cx >= card_x + 10.0
                                    && cx <= card_x + card_w - 10.0
                                    && cy >= item_start_y - 4.0
                                    && cy < item_start_y + 6.0 * (2.0 * line_h)
                                {
                                    let rel_y = cy - (item_start_y - 4.0);
                                    let item_idx = (rel_y / (2.0 * line_h)) as usize;
                                    if item_idx < 6 {
                                        self.settings_selected_index = item_idx;
                                        self.execute_settings_action(item_idx);
                                        return;
                                    }
                                }
                                return;
                            }

                            if self.help_visible {
                                let card_w = super::HELP_MODAL_WIDTH.min(content_w - 24.0);
                                let card_h = super::HELP_MODAL_HEIGHT.min(content_h - 24.0);
                                let card_x = (content_w - card_w) * 0.5;
                                let card_y = (content_h - card_h) * 0.5;
                                let close_x = card_x + card_w - 32.0;
                                let close_y = card_y + 8.0;

                                let in_close = cx >= close_x
                                    && cx <= close_x + 24.0
                                    && cy >= close_y
                                    && cy <= close_y + 24.0;
                                let outside_card = cx < card_x
                                    || cx > card_x + card_w
                                    || cy < card_y
                                    || cy > card_y + card_h;
                                if in_close || outside_card {
                                    self.help_visible = false;
                                    self.help_dragging_scrollbar = false;
                                    self.window.request_redraw();
                                    return;
                                }

                                let visible_h = card_h - 32.0;
                                let max_scroll = (self.help_total_height - visible_h).max(0.0);
                                if max_scroll > 0.0
                                    && cx >= card_x + card_w - 20.0
                                    && cx <= card_x + card_w
                                {
                                    let track_y = card_y + 40.0;
                                    let track_h = card_h - 52.0;
                                    let thumb_ratio =
                                        (visible_h / self.help_total_height).clamp(0.1, 0.9);
                                    let thumb_h = (track_h * thumb_ratio).max(28.0);

                                    self.help_dragging_scrollbar = true;
                                    let pct = ((cy - track_y - thumb_h * 0.5)
                                        / (track_h - thumb_h).max(1.0))
                                    .clamp(0.0, 1.0);
                                    self.help_scroll_y = pct * max_scroll;
                                    self.window.request_redraw();
                                }
                                return;
                            }

                            if self.tab_manager.tab_bar_visible(&self.config)
                                && cy >= 48.0
                                && cy < 48.0 + self.config.tabs.height
                            {
                                match self.tab_manager.handle_tab_bar_click(
                                    cx,
                                    cy,
                                    content_w,
                                    content_h,
                                    &mut self.text_engine,
                                    &self.config,
                                ) {
                                    pane::TabBarClickResult::Handled => {
                                        self.window.request_redraw();
                                        return;
                                    }
                                    pane::TabBarClickResult::None => {}
                                }
                            }

                            if let Some(action) = self.tab_manager.handle_pane_click_action(cx, cy)
                            {
                                match action {
                                    pane::PaneClickResult::ClosePane(pane_id) => {
                                        let alive = self.tab_manager.close_pane_by_id(
                                            pane_id,
                                            content_w,
                                            content_h,
                                            &mut self.text_engine,
                                            &self.config,
                                        );
                                        if !alive {
                                            el.exit();
                                        } else {
                                            self.window.request_redraw();
                                        }
                                        return;
                                    }
                                    pane::PaneClickResult::Focused(clicked_id) => {
                                        if let Some(pane) =
                                            self.tab_manager.panes.get_mut(&clicked_id)
                                        {
                                            let rel_x = cx - pane.rect[0] - pane::PANE_PAD;
                                            let rel_y = cy - pane.rect[1] - pane::PANE_PAD;
                                            let col = (rel_x / self.text_engine.cell_width)
                                                .max(0.0)
                                                as usize;
                                            let screen_row = (rel_y / self.text_engine.cell_height)
                                                .max(0.0)
                                                as usize;
                                            let col =
                                                col.min(pane.terminal.cols.saturating_sub(1));
                                            let screen_row = screen_row
                                                .min(pane.terminal.rows.saturating_sub(1));
                                            let abs_row =
                                                pane.terminal.screen_to_abs_row(screen_row);

                                            if pane.terminal.mouse_reporting && !self.shift_pressed
                                            {
                                                let seq = format!(
                                                    "\x1b[<0;{};{}M",
                                                    col + 1,
                                                    screen_row + 1
                                                );
                                                pane.pty.write_bytes(seq.as_bytes());
                                                self.drain_pty();
                                                self.window.request_redraw();
                                                return;
                                            }

                                            if self.ctrl_pressed {
                                                if let Some(url) =
                                                    pane.terminal.get_hyperlink_at(col, abs_row)
                                                {
                                                    crate::platform::open_url_safely(&url);
                                                    return;
                                                }
                                            }

                                            let now = Instant::now();
                                            let same_cell =
                                                self.last_click_cell == (col, screen_row);
                                            if same_cell
                                                && (now - self.last_click_time).as_millis() < 400
                                            {
                                                self.click_streak = (self.click_streak % 3) + 1;
                                            } else {
                                                self.click_streak = 1;
                                            }
                                            self.last_click_time = now;
                                            self.last_click_cell = (col, screen_row);

                                            if self.click_streak == 2 {
                                                pane.terminal.select_word_at(col, abs_row);
                                                self.selecting = false;
                                                self.window.request_redraw();
                                                return;
                                            } else if self.click_streak == 3 {
                                                pane.terminal.select_line_at(abs_row);
                                                self.selecting = false;
                                                self.window.request_redraw();
                                                return;
                                            }

                                            if let Some(last_row) =
                                                pane.terminal.last_content_abs_row()
                                            {
                                                if abs_row <= last_row {
                                                    let content_len = pane
                                                        .terminal
                                                        .abs_row_content_len(abs_row);
                                                    let clamped_col = if content_len > 0 {
                                                        col.min(content_len.saturating_sub(1))
                                                    } else {
                                                        0
                                                    };
                                                    self.selecting = true;
                                                    pane.terminal.selection =
                                                        Some(terminal::Selection {
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
                                                    self.selecting = false;
                                                    pane.terminal.clear_selection();
                                                }
                                            } else {
                                                self.selecting = false;
                                                pane.terminal.clear_selection();
                                            }
                                        }
                                    }
                                }
                            }
                        } else if let Some(pane) = self.tab_manager.active_pane_mut() {
                            pane.terminal.clear_selection();
                        }
                    }
                }
            }
            ElementState::Released => {
                self.mouse_left_down = false;
                self.last_drag_cell = None;
                self.dragging = false;
                self.help_dragging_scrollbar = false;
                self.wobble.handle_mouse_up();

                if let Some([cx, cy]) = self
                    .wobble
                    .screen_to_content_pos(self.mouse_pos[0], self.mouse_pos[1])
                {
                    if let Some(pane) = self.tab_manager.active_pane_mut() {
                        if pane.terminal.mouse_reporting && !self.shift_pressed {
                            let rel_x = cx - pane.rect[0] - pane::PANE_PAD;
                            let rel_y = cy - pane.rect[1] - pane::PANE_PAD;
                            let col = ((rel_x / self.text_engine.cell_width).max(0.0) as usize)
                                .min(pane.terminal.cols.saturating_sub(1));
                            let screen_row =
                                ((rel_y / self.text_engine.cell_height).max(0.0) as usize)
                                    .min(pane.terminal.rows.saturating_sub(1));
                            let seq = format!("\x1b[<0;{};{}m", col + 1, screen_row + 1);
                            pane.pty.write_bytes(seq.as_bytes());
                            self.drain_pty();
                            self.window.request_redraw();
                        }
                    }
                }

                if self.selecting {
                    self.selecting = false;
                    if let Some(pane) = self.tab_manager.active_pane_mut() {
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
        self.window.request_redraw();
    }

    pub fn handle_mouse_right(&mut self) {
        self.check_background_metadata();

        if let Some(pane) = self.tab_manager.active_pane_mut() {
            if pane.terminal.mouse_reporting && !self.shift_pressed {
                if let Some([cx, cy]) = self
                    .wobble
                    .screen_to_content_pos(self.mouse_pos[0], self.mouse_pos[1])
                {
                    let rel_x = cx - pane.rect[0] - pane::PANE_PAD;
                    let rel_y = cy - pane.rect[1] - pane::PANE_PAD;
                    let col = ((rel_x / self.text_engine.cell_width).max(0.0) as usize)
                        .min(pane.terminal.cols.saturating_sub(1));
                    let screen_row = ((rel_y / self.text_engine.cell_height).max(0.0) as usize)
                        .min(pane.terminal.rows.saturating_sub(1));
                    let seq = format!("\x1b[<2;{};{}M", col + 1, screen_row + 1);
                    pane.pty.write_bytes(seq.as_bytes());
                    self.drain_pty();
                    self.window.request_redraw();
                    return;
                }
            }

            if pane.terminal.has_selection() {
                if let Some(text) = pane.terminal.get_selected_text() {
                    clipboard::set_text(&text);
                }
                pane.terminal.clear_selection();
                self.window.request_redraw();
            } else if let Some(text) = clipboard::get_text() {
                let normalized = text.replace("\r\n", "\r").replace('\n', "\r");
                if pane.terminal.bracketed_paste {
                    pane.pty.write_bytes(b"\x1b[200~");
                    pane.pty.write_bytes(normalized.as_bytes());
                    pane.pty.write_bytes(b"\x1b[201~");
                } else {
                    pane.pty.write_bytes(normalized.as_bytes());
                }
                pane.terminal.scroll_to_bottom();
                self.drain_pty();
                self.window.request_redraw();
            }
        }
    }
}
