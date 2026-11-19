//! Core `TerminalGrid` state machine with `O(1)` `VecDeque` scrollback and buffer recycling.

pub mod cell;
pub mod graphics;
pub mod parser;
pub mod procedural;
pub mod renderer;
pub mod selection;

pub use cell::{BgRect, Cell, CellStyle, ColorRgb, CursorConfig, CursorShape};
pub use selection::{Selection, SelectionPoint};

use std::collections::VecDeque;
use vte::Parser;

pub struct TerminalGrid {
    pub cols: usize,
    pub rows: usize,
    pub cell_height: f32,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub saved_cursor_x: usize,
    pub saved_cursor_y: usize,
    pub cursor_visible: bool,
    pub cursor_config: CursorConfig,
    pub title: String,
    pub cwd: Option<std::path::PathBuf>,
    pub app_cursor_keys: bool,
    pub cells: Vec<Vec<Cell>>,
    pub current_style: CellStyle,
    pub scrollback: VecDeque<Vec<Cell>>,
    pub max_scrollback: usize,
    pub scroll_offset: usize,
    pub target_scroll_y: f32,
    pub current_scroll_y: f32,
    pub selection: Option<Selection>,
    pub selection_color: ColorRgb,
    pub parser: Parser,
    pub dirty: bool,
    pub responses: Vec<Vec<u8>>,
    pub is_alt_screen: bool,
    pub alt_screen: Option<Vec<Vec<Cell>>>,
    pub scroll_region_top: usize,
    pub scroll_region_bottom: usize,
    pub bracketed_paste: bool,
    pub mouse_reporting: bool,
    pub sgr_mouse: bool,
    pub hyperlinks: Vec<String>,
    pub current_hyperlink_idx: u16,
    pub in_sync_update: bool,
    pub sync_update_start: Option<std::time::Instant>,
    pub dirty_rows: Vec<u64>,
    /// Active Kitty Keyboard Protocol flags bitmask (CSI > flags u)
    pub kitty_keyboard_flags: u8,
    /// Stack of previous Kitty Keyboard Protocol flags (up to 32 levels)
    pub kitty_keyboard_stack: Vec<u8>,
    /// Kitty Graphics Protocol manager (APC / DCS G)
    pub graphics: graphics::KittyGraphicsManager,
    /// Pending APC buffer for multi-byte streaming sequences
    pub apc_buffer: Option<Vec<u8>>,
    /// Pending DCS graphics buffer
    pub dcs_graphics_buffer: Vec<u8>,
}

impl TerminalGrid {
    pub fn new(cols: usize, rows: usize) -> Self {
        let cols = cols.max(10);
        let rows = rows.max(5);
        let cells = vec![vec![Cell::default(); cols]; rows];
        let scroll_region_bottom = rows.saturating_sub(1);
        let num_words = (rows + 63) / 64;
        let dirty_rows = vec![!0u64; num_words];

        Self {
            cols,
            rows,
            cell_height: 20.0,
            cursor_x: 0,
            cursor_y: 0,
            saved_cursor_x: 0,
            saved_cursor_y: 0,
            cursor_visible: true,
            cursor_config: CursorConfig::default(),
            title: "Lunula Terminal".to_string(),
            cwd: None,
            app_cursor_keys: false,
            cells,
            current_style: CellStyle::default(),
            scrollback: VecDeque::new(),
            max_scrollback: 5000,
            scroll_offset: 0,
            target_scroll_y: 0.0,
            current_scroll_y: 0.0,
            selection: None,
            selection_color: ColorRgb {
                r: 45,
                g: 79,
                b: 103,
            }, // Kanagawa waveBlue2 (#2D4F67)
            parser: Parser::new(),
            dirty: true,
            responses: Vec::new(),
            is_alt_screen: false,
            alt_screen: None,
            scroll_region_top: 0,
            scroll_region_bottom,
            bracketed_paste: false,
            mouse_reporting: false,
            sgr_mouse: false,
            hyperlinks: Vec::new(),
            current_hyperlink_idx: 0,
            in_sync_update: false,
            sync_update_start: None,
            dirty_rows,
            kitty_keyboard_flags: 0,
            kitty_keyboard_stack: Vec::new(),
            graphics: graphics::KittyGraphicsManager::new(),
            apc_buffer: None,
            dcs_graphics_buffer: Vec::new(),
        }
    }

    #[inline]
    pub fn mark_row_dirty(&mut self, row: usize) {
        if row < self.rows {
            let word = row / 64;
            let bit = 1u64 << (row % 64);
            if word < self.dirty_rows.len() {
                self.dirty_rows[word] |= bit;
            }
            self.dirty = true;
        }
    }

    #[inline]
    pub fn mark_all_rows_dirty(&mut self) {
        for word in &mut self.dirty_rows {
            *word = !0u64;
        }
        self.dirty = true;
    }

    #[inline]
    pub fn is_row_dirty(&self, row: usize) -> bool {
        if row < self.rows {
            let word = row / 64;
            let bit = 1u64 << (row % 64);
            if let Some(&w) = self.dirty_rows.get(word) {
                return (w & bit) != 0;
            }
        }
        false
    }

    #[inline]
    pub fn clear_dirty_rows(&mut self) {
        for word in &mut self.dirty_rows {
            *word = 0;
        }
        self.dirty = false;
    }

    /// Checks if DEC 2026 Synchronized Update is active and within the 16ms frame timeout.
    pub fn is_sync_update_active(&mut self) -> bool {
        if self.in_sync_update {
            if let Some(start) = self.sync_update_start {
                if start.elapsed() >= std::time::Duration::from_millis(16) {
                    self.in_sync_update = false;
                    self.sync_update_start = None;
                    return false;
                }
            }
            return true;
        }
        false
    }

    pub fn resize(&mut self, new_cols: usize, new_rows: usize) {
        let new_cols = new_cols.max(10);
        let new_rows = new_rows.max(5);
        if new_cols == self.cols && new_rows == self.rows {
            return;
        }

        let mut new_cells = vec![vec![Cell::default(); new_cols]; new_rows];
        for r in 0..self.rows.min(new_rows) {
            let copy_len = self.cols.min(new_cols);
            new_cells[r][..copy_len].copy_from_slice(&self.cells[r][..copy_len]);
        }

        if let Some(alt) = &mut self.alt_screen {
            let mut new_alt = vec![vec![Cell::default(); new_cols]; new_rows];
            for r in 0..self.rows.min(new_rows) {
                let copy_len = self.cols.min(new_cols);
                new_alt[r][..copy_len].copy_from_slice(&alt[r][..copy_len]);
            }
            *alt = new_alt;
        }

        self.cols = new_cols;
        self.rows = new_rows;
        self.scroll_region_top = 0;
        self.scroll_region_bottom = new_rows.saturating_sub(1);
        self.cursor_x = self.cursor_x.min(new_cols - 1);
        self.cursor_y = self.cursor_y.min(new_rows - 1);
        self.cells = new_cells;
        self.scroll_offset = 0;
        self.target_scroll_y = 0.0;
        self.current_scroll_y = 0.0;
        let num_words = (new_rows + 63) / 64;
        self.dirty_rows = vec![!0u64; num_words];
        self.mark_all_rows_dirty();
    }

    pub fn advance_bytes(&mut self, bytes: &[u8]) {
        let mut idx = 0;
        let mut normal_start = 0;

        while idx < bytes.len() {
            if let Some(ref mut apc) = self.apc_buffer {
                // Currently buffering an APC sequence. Look for ST (\x1b\ or \x07)
                // Check if ST was split across buffer chunks (previous chunk ended with 0x1b, this chunk starts with b'\\')
                if apc.last() == Some(&0x1b) && bytes[idx] == b'\\' {
                    apc.pop();
                    let full_apc = std::mem::take(apc);
                    self.apc_buffer = None;
                    self.handle_apc_kitty(&full_apc);
                    idx += 1;
                    normal_start = idx;
                    continue;
                }

                let mut found_st = false;
                let mut scan = idx;
                while scan < bytes.len() {
                    if bytes[scan] == 0x07 {
                        apc.extend_from_slice(&bytes[idx..scan]);
                        idx = scan + 1;
                        found_st = true;
                        break;
                    } else if bytes[scan] == 0x1b && scan + 1 < bytes.len() && bytes[scan + 1] == b'\\' {
                        apc.extend_from_slice(&bytes[idx..scan]);
                        idx = scan + 2;
                        found_st = true;
                        break;
                    }
                    scan += 1;
                }

                if found_st {
                    let full_apc = std::mem::take(apc);
                    self.apc_buffer = None;
                    self.handle_apc_kitty(&full_apc);
                    normal_start = idx;
                } else {
                    apc.extend_from_slice(&bytes[idx..]);
                    return;
                }
            } else if bytes[idx] == 0x1b
                && idx + 2 < bytes.len()
                && bytes[idx + 1] == b'_'
                && bytes[idx + 2] == b'G'
            {
                // Found APC Kitty Graphics start: \x1b_G
                if idx > normal_start {
                    let mut parser = std::mem::replace(&mut self.parser, Parser::new());
                    parser.advance(self, &bytes[normal_start..idx]);
                    self.parser = parser;
                }

                idx += 3; // Skip \x1b_G
                let mut apc = Vec::new();
                let mut found_st = false;
                let mut scan = idx;

                while scan < bytes.len() {
                    if bytes[scan] == 0x07 {
                        apc.extend_from_slice(&bytes[idx..scan]);
                        idx = scan + 1;
                        found_st = true;
                        break;
                    } else if bytes[scan] == 0x1b && scan + 1 < bytes.len() && bytes[scan + 1] == b'\\' {
                        apc.extend_from_slice(&bytes[idx..scan]);
                        idx = scan + 2;
                        found_st = true;
                        break;
                    }
                    scan += 1;
                }

                if found_st {
                    self.handle_apc_kitty(&apc);
                    normal_start = idx;
                } else {
                    apc.extend_from_slice(&bytes[idx..]);
                    self.apc_buffer = Some(apc);
                    return;
                }
            } else {
                idx += 1;
            }
        }

        if normal_start < bytes.len() {
            let mut parser = std::mem::replace(&mut self.parser, Parser::new());
            parser.advance(self, &bytes[normal_start..]);
            self.parser = parser;
        }
    }

    pub fn handle_apc_kitty(&mut self, payload: &[u8]) {
        let resp = self.graphics.handle_kitty_sequence(
            payload,
            self.cursor_x,
            self.cursor_y,
            self.scrollback.len(),
        );
        if let Some(r) = resp {
            self.responses.push(r.into_bytes());
        }
        self.mark_all_rows_dirty();
        self.dirty = true;
    }

    /// Processes a complete IPC packet containing one or more `_G...\` Kitty Graphics
    /// sequences without echoing responses back into ConPTY stdin.
    pub fn handle_ipc_bytes(&mut self, bytes: &[u8]) {
        let mut idx = 0;
        while idx + 3 < bytes.len() {
            if bytes[idx] == 0x1b && bytes[idx + 1] == b'_' && bytes[idx + 2] == b'G' {
                idx += 3;
                let start = idx;
                let mut end = bytes.len();
                let mut next_idx = bytes.len();
                while idx < bytes.len() {
                    if bytes[idx] == 0x07 {
                        end = idx;
                        next_idx = idx + 1;
                        break;
                    } else if bytes[idx] == 0x1b && idx + 1 < bytes.len() && bytes[idx + 1] == b'\\' {
                        end = idx;
                        next_idx = idx + 2;
                        break;
                    }
                    idx += 1;
                }
                let _ = self.graphics.handle_kitty_sequence(
                    &bytes[start..end],
                    self.cursor_x,
                    self.cursor_y,
                    self.scrollback.len(),
                );
                self.mark_all_rows_dirty();
                self.dirty = true;
                idx = next_idx;
            } else {
                idx += 1;
            }
        }
    }

    #[inline]
    pub fn content_lines_in_scrollback(&self) -> usize {
        self.scrollback.len()
    }

    /// Scrolls the active region upward by 1 row in `O(1)` using `VecDeque` and buffer recycling.
    pub fn scroll_region_up(&mut self) {
        if self.rows == 0
            || self.scroll_region_top >= self.scroll_region_bottom
            || self.scroll_region_bottom >= self.rows
        {
            return;
        }

        if self.scroll_region_top == 0
            && self.scroll_region_bottom == self.rows - 1
            && !self.is_alt_screen
        {
            let mut line = self.cells.remove(0);
            let has_content = line.iter().any(|c| c.c != ' ' || c.style.bg.is_some());

            let push_to_sb = if has_content {
                true
            } else if !self.scrollback.is_empty() {
                let last_empty = self.scrollback.back().map_or(false, |l| {
                    !l.iter().any(|c| c.c != ' ' || c.style.bg.is_some())
                });
                !last_empty
            } else {
                false
            };

            if push_to_sb {
                if self.scrollback.len() >= self.max_scrollback {
                    // Recycle the oldest scrollback row buffer directly into the new bottom row (0 heap alloc!)
                    let mut oldest = self
                        .scrollback
                        .pop_front()
                        .unwrap_or_else(|| vec![Cell::default(); self.cols]);
                    oldest.resize(self.cols, Cell::default());
                    oldest.fill(Cell::default());
                    self.scrollback.push_back(line);
                    self.cells.push(oldest);

                    self.graphics.shift_rows_up(1);
                    if let Some(sel) = &mut self.selection {
                        if sel.anchor.abs_row > 0 {
                            sel.anchor.abs_row -= 1;
                        }
                        if sel.active.abs_row > 0 {
                            sel.active.abs_row -= 1;
                        }
                    }
                } else {
                    self.scrollback.push_back(line);
                    self.cells.push(vec![Cell::default(); self.cols]);
                }
            } else {
                // Recycle the blank top line directly as the new blank bottom line (0 heap alloc!)
                // Since scrollback.len() did not grow while visible rows moved up by 1, shift images up too.
                self.graphics.shift_rows_up(1);
                line.fill(Cell::default());
                self.cells.push(line);
            }
        } else {
            let mut recycled = self.cells.remove(self.scroll_region_top);
            recycled.fill(Cell::default());
            self.cells.insert(self.scroll_region_bottom, recycled);
        }
        self.dirty = true;
    }

    pub fn scroll_region_down(&mut self) {
        if self.rows == 0
            || self.scroll_region_top >= self.scroll_region_bottom
            || self.scroll_region_bottom >= self.rows
        {
            return;
        }
        let mut recycled = self.cells.remove(self.scroll_region_bottom);
        recycled.fill(Cell::default());
        self.cells.insert(self.scroll_region_top, recycled);
        self.dirty = true;
    }

    #[allow(dead_code)]
    pub fn scroll_up(&mut self) {
        self.scroll_region_up();
    }

    pub fn scroll_up_view(&mut self, lines: usize) {
        if self.is_alt_screen {
            return;
        }
        let max_scroll = self.content_lines_in_scrollback();
        if max_scroll == 0 {
            return;
        }
        self.scroll_offset = (self.scroll_offset + lines).min(max_scroll);
        self.dirty = true;
    }

    pub fn scroll_down_view(&mut self, lines: usize) {
        if self.is_alt_screen {
            return;
        }
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
        self.dirty = true;
    }

    pub fn scroll_up_pixels(&mut self, pixels: f32, cell_h: f32) {
        if self.is_alt_screen {
            return;
        }
        self.cell_height = cell_h.max(1.0);
        let max_pixels = self.content_lines_in_scrollback() as f32 * self.cell_height;
        if max_pixels <= 0.0 {
            self.target_scroll_y = 0.0;
            self.current_scroll_y = 0.0;
            return;
        }
        self.target_scroll_y = (self.target_scroll_y + pixels).clamp(0.0, max_pixels);
    }

    pub fn scroll_down_pixels(&mut self, pixels: f32, cell_h: f32) {
        if self.is_alt_screen {
            return;
        }
        self.cell_height = cell_h.max(1.0);
        let max_pixels = self.content_lines_in_scrollback() as f32 * self.cell_height;
        if max_pixels <= 0.0 {
            self.target_scroll_y = 0.0;
            self.current_scroll_y = 0.0;
            return;
        }
        self.target_scroll_y = (self.target_scroll_y - pixels).clamp(0.0, max_pixels);
    }

    /// Interpolates `current_scroll_y` toward `target_scroll_y` without forcing full text re-shaping
    /// unless the integer row offset crosses a row boundary.
    pub fn update_smooth_scroll(&mut self, dt: f32) -> bool {
        let diff = self.target_scroll_y - self.current_scroll_y;
        if diff.abs() > 0.05 {
            let factor = 1.0 - (-24.0 * dt).exp();
            self.current_scroll_y += diff * factor;
            true
        } else if self.current_scroll_y != self.target_scroll_y {
            self.current_scroll_y = self.target_scroll_y;
            true
        } else {
            false
        }
    }

    pub fn scroll_to_bottom(&mut self) {
        if self.scroll_offset != 0 || self.target_scroll_y != 0.0 || self.current_scroll_y != 0.0 {
            self.scroll_offset = 0;
            self.target_scroll_y = 0.0;
            self.current_scroll_y = 0.0;
            self.dirty = true;
        }
    }

    #[allow(dead_code)]
    pub fn visible_row(&self, screen_r: usize) -> &[Cell] {
        let effective_offset = if self.current_scroll_y > 0.0 {
            (self.current_scroll_y / self.cell_height.max(1.0)).floor() as usize
        } else {
            self.scroll_offset
        };
        self.visible_row_at(screen_r, effective_offset)
    }

    pub fn visible_row_at(&self, screen_r: usize, base_offset: usize) -> &[Cell] {
        if base_offset == 0 {
            if screen_r < self.cells.len() {
                &self.cells[screen_r]
            } else {
                &[]
            }
        } else {
            let sb_len = self.scrollback.len();
            if base_offset > sb_len {
                return &[];
            }
            let total_rows = sb_len + self.rows;
            let start_idx = total_rows.saturating_sub(base_offset + self.rows);
            let idx = start_idx + screen_r;
            if idx < sb_len {
                &self.scrollback[idx]
            } else {
                let cell_idx = idx - sb_len;
                if cell_idx < self.cells.len() {
                    &self.cells[cell_idx]
                } else {
                    &[]
                }
            }
        }
    }

    pub fn screen_to_abs_row(&self, screen_row: usize) -> usize {
        let sb_len = self.scrollback.len();
        let total_rows = sb_len + self.rows;
        let base_offset = if self.current_scroll_y > 0.0 {
            (self.current_scroll_y / self.cell_height.max(1.0)).floor() as usize
        } else {
            self.scroll_offset
        };
        let start_idx = total_rows.saturating_sub(base_offset + self.rows);
        start_idx + screen_row
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::cell::ansi_16_color;

    #[test]
    fn test_cell_memory_compaction() {
        assert_eq!(std::mem::size_of::<CellStyle>(), 13);
        assert_eq!(std::mem::size_of::<Cell>(), 20);
    }

    #[test]
    fn test_print_and_backspace() {
        let mut grid = TerminalGrid::new(20, 5);
        grid.advance_bytes(b"hello");
        assert_eq!(grid.cursor_x, 5);
        assert_eq!(grid.cells[0][0].c, 'h');
        assert_eq!(grid.cells[0][4].c, 'o');

        grid.advance_bytes(b"\x08");
        assert_eq!(grid.cursor_x, 4);

        grid.advance_bytes(b"\x1b[K");
        assert_eq!(grid.cells[0][4].c, ' ');
    }

    #[test]
    fn test_typing_spans_visibility() {
        let mut grid = TerminalGrid::new(40, 5);
        grid.advance_bytes(b"PS C:\\> ");
        assert_eq!(grid.cursor_x, 8);

        grid.advance_bytes(b"\x1b[?25l\x1b[93mgit \x1b[37mstatus\x1b[?25h");
        assert_eq!(grid.cursor_x, 18);
        assert_eq!(grid.cells[0][8].c, 'g');
        assert_eq!(grid.cells[0][17].c, 's');

        let (spans, _bg) = grid.render_spans_and_backgrounds(10.0, 20.0, 10.0, 10.0);
        let joined: String = spans.iter().map(|(s, _)| s.as_str()).collect();
        assert!(joined.starts_with("PS C:\\> git status"));
    }

    #[test]
    fn test_sgr_colors() {
        let mut grid = TerminalGrid::new(40, 5);
        grid.advance_bytes(b"\x1b[31mhi");
        assert_eq!(grid.cells[0][0].c, 'h');
        assert_eq!(grid.cells[0][0].style.fg, Some(ansi_16_color(1)));

        grid.advance_bytes(b"\x1b[48;2;138;173;244m\x1b[38;2;255;255;255m!");
        assert_eq!(grid.cells[0][2].c, '!');
        assert_eq!(grid.cells[0][2].style.bg, Some(ColorRgb { r: 138, g: 173, b: 244 }));
        assert_eq!(grid.cells[0][2].style.fg, Some(ColorRgb { r: 255, g: 255, b: 255 }));

        grid.advance_bytes(b"\x1b[m?");
        assert_eq!(grid.cells[0][3].c, '?');
        assert_eq!(grid.cells[0][3].style.fg, None);
        assert_eq!(grid.cells[0][3].style.bg, None);
    }

    #[test]
    fn test_scrollback_and_recycling() {
        let mut grid = TerminalGrid::new(10, 5);
        grid.max_scrollback = 3;
        grid.advance_bytes(b"line 1\r\nline 2\r\nline 3\r\nline 4\r\nline 5\r\nline 6\r\nline 7");
        assert_eq!(grid.scrollback.len(), 2);
        assert_eq!(grid.scrollback[0][0].c, 'l');
        assert_eq!(grid.scrollback[0][5].c, '1');

        grid.scroll_up_view(1);
        assert_eq!(grid.scroll_offset, 1);
        let top_row = grid.visible_row(0);
        assert_eq!(top_row[5].c, '2');

        grid.scroll_to_bottom();
        assert_eq!(grid.scroll_offset, 0);

        let mut fresh = TerminalGrid::new(10, 5);
        fresh.scroll_up_view(5);
        assert_eq!(fresh.scroll_offset, 0);
    }

    #[test]
    fn test_selection_and_run_length_merging() {
        let mut grid = TerminalGrid::new(20, 5);
        grid.advance_bytes(b"hello world\r\nsecond line");

        grid.selection = Some(Selection {
            anchor: SelectionPoint { col: 6, abs_row: 0 },
            active: SelectionPoint { col: 10, abs_row: 0 },
        });
        assert!(grid.has_selection());
        assert_eq!(grid.get_selected_text(), Some("world".to_string()));

        grid.selection = Some(Selection {
            anchor: SelectionPoint { col: 10, abs_row: 0 },
            active: SelectionPoint { col: 6, abs_row: 0 },
        });
        assert_eq!(grid.get_selected_text(), Some("world".to_string()));

        grid.selection = Some(Selection {
            anchor: SelectionPoint { col: 6, abs_row: 0 },
            active: SelectionPoint { col: 5, abs_row: 1 },
        });
        let selected = grid.get_selected_text().unwrap();
        assert_eq!(selected, "world\r\nsecond");

        let (_, bg_rects) = grid.render_spans_and_backgrounds(8.0, 16.0, 16.0, 16.0);
        assert!(!bg_rects.is_empty());
        assert_eq!(bg_rects[0].color, grid.selection_color);

        grid.clear_selection();
        assert!(!grid.has_selection());
        assert_eq!(grid.get_selected_text(), None);

        let mut empty_grid = TerminalGrid::new(20, 5);
        empty_grid.advance_bytes(b"prompt>");
        assert_eq!(empty_grid.last_content_abs_row(), Some(0));
        assert_eq!(empty_grid.abs_row_content_len(0), 7);
        assert_eq!(empty_grid.abs_row_content_len(1), 0);

        empty_grid.selection = Some(Selection {
            anchor: SelectionPoint { col: 0, abs_row: 0 },
            active: SelectionPoint { col: 19, abs_row: 4 },
        });

        assert_eq!(empty_grid.get_selected_text(), Some("prompt>".to_string()));

        // Run-Length Merging coalesces the 7 contiguous selected cells into 1 BgRect of width 56.0
        let cell_w = 8.0;
        let cell_h = 16.0;
        let pad_x = 10.0;
        let pad_y = 10.0;
        let (_, bg_rects) = empty_grid.render_spans_and_backgrounds(cell_w, cell_h, pad_x, pad_y);
        assert_eq!(bg_rects.len(), 1);
        assert_eq!(bg_rects[0].y, pad_y);
        assert_eq!(bg_rects[0].w, 7.0 * cell_w);
        assert_eq!(bg_rects[0].color, empty_grid.selection_color);
    }

    #[test]
    fn test_osc_window_title() {
        let mut grid = TerminalGrid::new(20, 5);
        grid.advance_bytes(b"\x1b]0;My Custom PowerShell\x07");
        assert_eq!(grid.title, "My Custom PowerShell");

        grid.advance_bytes(b"\x1b]2;nvim main.rs\x07");
        assert_eq!(grid.title, "nvim main.rs");
    }

    #[test]
    fn test_decscusr_cursor_shapes() {
        let mut grid = TerminalGrid::new(20, 5);
        grid.advance_bytes(b"\x1b[5 q");
        assert_eq!(grid.cursor_config.shape, CursorShape::Beam);
        assert!(grid.cursor_config.blinking);

        grid.advance_bytes(b"\x1b[2 q");
        assert_eq!(grid.cursor_config.shape, CursorShape::Block);
        assert!(!grid.cursor_config.blinking);

        grid.advance_bytes(b"\x1b[3 q");
        assert_eq!(grid.cursor_config.shape, CursorShape::Underline);
        assert!(grid.cursor_config.blinking);
    }

    #[test]
    fn test_decorations_underline_strikethrough_undercurl() {
        let mut grid = TerminalGrid::new(20, 5);
        grid.advance_bytes(b"\x1b[9mtest\x1b[29m");
        assert!(grid.cells[0][0].style.strikethrough());
        assert!(!grid.cells[0][4].style.strikethrough());

        grid.advance_bytes(b"\x1b[4m\x1b[58;2;255;0;0merror");
        assert!(grid.cells[0][4].style.underline());
        assert_eq!(
            grid.cells[0][4].style.underline_color,
            Some(ColorRgb { r: 255, g: 0, b: 0 })
        );
    }

    #[test]
    fn test_alternate_screen_buffer() {
        let mut grid = TerminalGrid::new(10, 5);
        grid.advance_bytes(b"primary");
        assert_eq!(grid.cells[0][0].c, 'p');

        grid.advance_bytes(b"\x1b[?1049h");
        assert!(grid.is_alt_screen);
        assert_eq!(grid.cells[0][0].c, ' ');

        grid.advance_bytes(b"nvim");
        assert_eq!(grid.cells[0][0].c, 'n');

        grid.advance_bytes(b"\x1b[?1049l");
        assert!(!grid.is_alt_screen);
        assert_eq!(grid.cells[0][0].c, 'p');
    }

    #[test]
    fn test_win32_input_mode_ignored() {
        let mut grid = TerminalGrid::new(20, 5);
        grid.advance_bytes(b"\x1b[67;0;99;1;0;1_");
        assert_eq!(grid.cells[0][0].c, ' ');
    }

    #[test]
    fn test_procedural_box_and_powerline() {
        let mut grid = TerminalGrid::new(10, 5);
        grid.advance_bytes("─\u{E0B0}".as_bytes());
        let (spans, bg_rects) = grid.render_spans_and_backgrounds(10.0, 20.0, 0.0, 0.0);
        assert_eq!(spans[0].0, "  ");
        assert!(!bg_rects.is_empty());
    }

    #[test]
    fn test_scroll_region_and_bounded_scrolling() {
        let mut grid = TerminalGrid::new(10, 5);
        grid.advance_bytes(b"HEADER\r\n");
        grid.advance_bytes(b"row1\r\nrow2\r\nrow3\r\n");
        grid.advance_bytes(b"STATUS");

        assert_eq!(grid.cells[0][0].c, 'H');
        assert_eq!(grid.cells[4][0].c, 'S');

        grid.advance_bytes(b"\x1b[2;4r");
        assert_eq!(grid.scroll_region_top, 1);
        assert_eq!(grid.scroll_region_bottom, 3);
        assert_eq!(grid.cursor_x, 0);
        assert_eq!(grid.cursor_y, 0);

        grid.advance_bytes(b"\x1b[4;1H");
        assert_eq!(grid.cursor_y, 3);
        grid.advance_bytes(b"new4\r\n");

        assert_eq!(grid.cells[0][0].c, 'H');
        assert_eq!(grid.cells[4][0].c, 'S');
        assert_eq!(grid.scrollback.len(), 0);
    }

    #[test]
    fn test_sgr_mouse_and_bracketed_paste_modes() {
        let mut grid = TerminalGrid::new(20, 5);

        grid.advance_bytes(b"\x1b[?2004h");
        assert!(grid.bracketed_paste);
        grid.advance_bytes(b"\x1b[?2004l");
        assert!(!grid.bracketed_paste);

        grid.advance_bytes(b"\x1b[?1000h");
        assert!(grid.mouse_reporting);
        grid.advance_bytes(b"\x1b[?1000l");
        assert!(!grid.mouse_reporting);

        grid.advance_bytes(b"\x1b[?1006h");
        assert!(grid.sgr_mouse);
        grid.advance_bytes(b"\x1b[?1006l");
        assert!(!grid.sgr_mouse);
    }

    #[test]
    fn test_word_and_line_selection() {
        let mut grid = TerminalGrid::new(30, 5);
        grid.advance_bytes(b"git checkout -b feature-foo");

        grid.select_word_at(6, 0);
        assert_eq!(grid.get_selected_text(), Some("checkout".to_string()));

        grid.select_word_at(18, 0);
        assert_eq!(grid.get_selected_text(), Some("feature-foo".to_string()));

        grid.select_line_at(0);
        assert_eq!(
            grid.get_selected_text(),
            Some("git checkout -b feature-foo".to_string())
        );
    }

    #[test]
    fn test_procedural_rounded_corners_and_blocks() {
        let mut grid = TerminalGrid::new(20, 5);
        grid.advance_bytes("╭╮╯╰█═║░".as_bytes());
        let (spans, bg_rects) = grid.render_spans_and_backgrounds(10.0, 20.0, 0.0, 0.0);
        assert_eq!(spans[0].0, "        ");
        assert!(!bg_rects.is_empty());
    }

    #[test]
    fn test_osc_8_hyperlinks() {
        let mut grid = TerminalGrid::new(30, 5);
        grid.advance_bytes(b"\x1b]8;;https://github.com\x1b\\GitHub Repo\x1b]8;;\x1b\\");

        assert!(grid.cells[0][0].hyperlink_idx > 0);
        assert_eq!(
            grid.get_hyperlink_at(3, 0),
            Some("https://github.com".to_string())
        );

        let mut plain_grid = TerminalGrid::new(40, 5);
        plain_grid.advance_bytes(b"visit https://example.com for info");
        assert_eq!(
            plain_grid.get_hyperlink_at(10, 0),
            Some("https://example.com".to_string())
        );
    }

    #[test]
    fn test_wide_characters_no_duplicate_spaces() {
        let mut grid = TerminalGrid::new(20, 5);
        grid.advance_bytes("中文".as_bytes());
        assert_eq!(grid.cells[0][0].c, '中');
        assert_eq!(grid.cells[0][1].c, '\0');
        assert_eq!(grid.cells[0][2].c, '文');
        assert_eq!(grid.cells[0][3].c, '\0');

        let (spans, _) = grid.render_spans_and_backgrounds(10.0, 20.0, 0.0, 0.0);
        assert_eq!(spans[0].0, "中文");
    }

    #[test]
    fn test_osc_cwd_handling() {
        let mut grid = TerminalGrid::new(20, 5);
        let current_dir = std::env::current_dir().unwrap();
        let path_str = current_dir.to_str().unwrap().replace('\\', "/");
        let osc7 = format!("\x1b]7;file://localhost/{}\x07", path_str);
        grid.advance_bytes(osc7.as_bytes());
        assert_eq!(grid.cwd, Some(current_dir.clone()));

        let mut grid2 = TerminalGrid::new(20, 5);
        let osc9 = format!("\x1b]9;9;\"{}\"\x07", current_dir.display());
        grid2.advance_bytes(osc9.as_bytes());
        assert_eq!(grid2.cwd, Some(current_dir));
    }

    #[test]
    fn test_app_cursor_keys_mode() {
        let mut grid = TerminalGrid::new(20, 5);
        assert!(!grid.app_cursor_keys);
        grid.advance_bytes(b"\x1b[?1h");
        assert!(grid.app_cursor_keys);
        grid.advance_bytes(b"\x1b[?1l");
        assert!(!grid.app_cursor_keys);
    }

    #[test]
    fn test_dec_mode_2026_synchronized_update() {
        let mut grid = TerminalGrid::new(20, 5);
        assert!(!grid.in_sync_update);
        assert!(!grid.is_sync_update_active());

        // Enable mode ?2026h
        grid.advance_bytes(b"\x1b[?2026h");
        assert!(grid.in_sync_update);
        assert!(grid.is_sync_update_active());

        // Query DECRQM ?2026$p
        grid.advance_bytes(b"\x1b[?2026$p");
        assert_eq!(grid.responses.len(), 1);
        assert_eq!(grid.responses[0], b"\x1b[?2026;1$y");
        grid.responses.clear();

        // Disable mode ?2026l
        grid.advance_bytes(b"\x1b[?2026l");
        assert!(!grid.in_sync_update);
        assert!(!grid.is_sync_update_active());

        // Query DECRQM when reset
        grid.advance_bytes(b"\x1b[?2026$p");
        assert_eq!(grid.responses.len(), 1);
        assert_eq!(grid.responses[0], b"\x1b[?2026;2$y");
    }

    #[test]
    fn test_row_level_dirty_bitmask() {
        let mut grid = TerminalGrid::new(20, 10);
        grid.clear_dirty_rows();
        assert!(!grid.dirty);
        for r in 0..10 {
            assert!(!grid.is_row_dirty(r));
        }

        // Typing characters on row 0
        grid.advance_bytes(b"prompt>");
        assert!(grid.dirty);
        assert!(grid.is_row_dirty(0));
        // All other rows remain clean
        for r in 1..10 {
            assert!(!grid.is_row_dirty(r));
        }

        // Clear dirty rows and move cursor to row 3
        grid.clear_dirty_rows();
        grid.advance_bytes(b"\x1b[4;1Hhello");
        assert!(grid.is_row_dirty(3));
        assert!(!grid.is_row_dirty(0));
        assert!(!grid.is_row_dirty(1));
        assert!(!grid.is_row_dirty(2));
        assert!(!grid.is_row_dirty(4));
    }

    #[test]
    fn test_kitty_keyboard_protocol_stack_and_query() {
        let mut grid = TerminalGrid::new(80, 24);
        assert_eq!(grid.kitty_keyboard_flags, 0);

        // 1. Query initial flags
        grid.advance_bytes(b"\x1b[?u");
        assert_eq!(grid.responses.pop().unwrap(), b"\x1b[?0u");

        // 2. Push flags = 1 (disambiguate escape codes)
        grid.advance_bytes(b"\x1b[>1u");
        assert_eq!(grid.kitty_keyboard_flags, 1);
        assert_eq!(grid.kitty_keyboard_stack, vec![0]);

        // 3. Query flags -> should be 1
        grid.advance_bytes(b"\x1b[?u");
        assert_eq!(grid.responses.pop().unwrap(), b"\x1b[?1u");

        // 4. Push flags = 3
        grid.advance_bytes(b"\x1b[>3u");
        assert_eq!(grid.kitty_keyboard_flags, 3);
        assert_eq!(grid.kitty_keyboard_stack, vec![0, 1]);

        // 5. Pop flags -> restores 1
        grid.advance_bytes(b"\x1b[<1u");
        assert_eq!(grid.kitty_keyboard_flags, 1);
        assert_eq!(grid.kitty_keyboard_stack, vec![0]);

        // 6. Pop flags again -> restores 0
        grid.advance_bytes(b"\x1b[<u");
        assert_eq!(grid.kitty_keyboard_flags, 0);
        assert!(grid.kitty_keyboard_stack.is_empty());

        // 7. Modify mode with CSI = flags ; mode u
        grid.advance_bytes(b"\x1b[=5;1u"); // set = 5
        assert_eq!(grid.kitty_keyboard_flags, 5);
        grid.advance_bytes(b"\x1b[=2;2u"); // OR with 2 -> 5 | 2 = 7
        assert_eq!(grid.kitty_keyboard_flags, 7);
        grid.advance_bytes(b"\x1b[=4;3u"); // AND NOT 4 -> 7 & !4 = 3
        assert_eq!(grid.kitty_keyboard_flags, 3);
    }

    #[test]
    fn test_kitty_graphics_apc_and_dcs_handling() {
        let mut grid = TerminalGrid::new(40, 10);
        assert_eq!(grid.graphics.images.len(), 0);

        // 1. Send 2x2 RGBA image via APC sequence: \x1b_G...;\x1b\
        let apc_seq = b"\x1b_Ga=t,f=32,s=2,v=2,c=4,r=2,i=100;/wAA//8AAP//AAD//wAA/w==\x1b\\";
        grid.advance_bytes(apc_seq);
        assert_eq!(grid.graphics.images.len(), 1);
        assert_eq!(grid.graphics.images[0].id, 100);
        assert_eq!(grid.graphics.images[0].width_cells, 4);
        assert_eq!(grid.graphics.images[0].height_cells, 2);
        assert_eq!(grid.responses.pop().unwrap(), b"\x1b_Gi=100;OK\x1b\\");

        // Cursor remains at position (image is placed at cursor cell)
        assert_eq!(grid.cursor_y, 0);

        // 2. Clear screen with \x1b[2J -> should clear images
        grid.advance_bytes(b"\x1b[2J");
        assert_eq!(grid.graphics.images.len(), 0);

        // 3. Send via DCS sequence: \x1bPG...;\x1b\
        let dcs_seq = b"\x1bPGa=t,f=32,s=2,v=2,c=2,r=1,i=101;/wAA//8AAP//AAD//wAA/w==\x1b\\";
        grid.advance_bytes(dcs_seq);
        assert_eq!(grid.graphics.images.len(), 1);
        assert_eq!(grid.graphics.images[0].id, 101);
        assert_eq!(grid.responses.pop().unwrap(), b"\x1b_Gi=101;OK\x1b\\");

        // 4. Delete image via Kitty APC delete sequence: \x1b_Ga=d,d=i,i=101;\x1b\
        grid.advance_bytes(b"\x1b_Ga=d,d=i,i=101;\x1b\\");
        assert_eq!(grid.graphics.images.len(), 0);

        // 5. Test split sequence across multiple buffer chunks (e.g. streaming over TCP)
        let chunk1 = b"\x1b_Ga=t,f=32,s=2,v=2,c=4,r=2,i=102;/wAA";
        let chunk2 = b"//8AAP//AAD//wAA/w";
        let chunk3 = b"==\x1b\\";
        grid.advance_bytes(chunk1);
        assert_eq!(grid.graphics.images.len(), 0); // Not completed yet
        grid.advance_bytes(chunk2);
        assert_eq!(grid.graphics.images.len(), 0); // Not completed yet
        grid.advance_bytes(chunk3);
        assert_eq!(grid.graphics.images.len(), 1); // Completed upon ST!
        assert_eq!(grid.graphics.images[0].id, 102);
    }
}
