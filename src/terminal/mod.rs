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
    pub last_hyperlink_prune_len: usize,
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
    /// 16 ANSI Palette colors
    pub palette: [ColorRgb; 16],
    /// Whether OSC 52 clipboard reading (`OSC 52 ; Pc ; ? ST`) is permitted (default: false for security)
    pub allow_clipboard_read: bool,
    /// Combining diacritical marks and zero-width characters attached to (abs_row, col)
    pub combining_chars: std::collections::HashMap<(usize, usize), Vec<char>>,
    /// Dynamic foreground color for OSC 10 queries
    pub fg_color: ColorRgb,
    /// Dynamic background color for OSC 11 queries
    pub bg_color: ColorRgb,
    /// Whether DEC private mode ?1004 (focus reporting) is active
    pub focus_reporting: bool,
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
            last_hyperlink_prune_len: 0,
            in_sync_update: false,
            sync_update_start: None,
            dirty_rows,
            kitty_keyboard_flags: 0,
            kitty_keyboard_stack: Vec::new(),
            graphics: graphics::KittyGraphicsManager::new(),
            apc_buffer: None,
            dcs_graphics_buffer: Vec::new(),
            palette: cell::DEFAULT_ANSI_16,
            allow_clipboard_read: false,
            combining_chars: std::collections::HashMap::new(),
            fg_color: ColorRgb { r: 220, g: 215, b: 186 }, // Kanagawa fujiWhite (#DCD7BA)
            bg_color: ColorRgb { r: 31, g: 31, b: 40 },    // Kanagawa sumiInk1 (#1F1F28)
            focus_reporting: false,
        }
    }

    pub fn set_palette(&mut self, palette: [ColorRgb; 16]) {
        self.palette = palette;
        self.mark_all_rows_dirty();
    }

    pub fn set_theme_colors(&mut self, fg: ColorRgb, bg: ColorRgb) {
        self.fg_color = fg;
        self.bg_color = bg;
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

        if self.is_alt_screen {
            // Full-screen TUI apps (e.g. vim, htop) manage their own screen layouts on resize
            let mut new_cells = vec![vec![Cell::default(); new_cols]; new_rows];
            for r in 0..self.rows.min(new_rows) {
                let copy_len = self.cols.min(new_cols);
                new_cells[r][..copy_len].copy_from_slice(&self.cells[r][..copy_len]);
            }
            self.cells = new_cells;
        } else {
            // Standard primary screen: perform smart line reflow across scrollback and visible rows
            let mut physical_lines: Vec<Vec<Cell>> = self.scrollback.drain(..).collect();
            physical_lines.extend(self.cells.drain(..));

            // Trim trailing completely empty lines from the bottom of visible screen so content stays in view
            while physical_lines.len() > 1
                && physical_lines.last().map_or(false, |row| {
                    !row.last().map_or(false, |c| c.style.wrapped())
                        && !row.iter().any(|c| c.c != ' ' || c.style.bg.is_some() || c.hyperlink_idx > 0)
                })
            {
                physical_lines.pop();
            }

            // 1. Group physical lines into continuous logical lines based on soft-wrapping
            let mut logical_lines: Vec<Vec<Cell>> = Vec::new();
            let mut current_logical: Vec<Cell> = Vec::new();

            for row in physical_lines {
                let was_wrapped = row.last().map_or(false, |c| c.style.wrapped());
                let content_len = row
                    .iter()
                    .rposition(|c| c.c != ' ' || c.style.bg.is_some() || c.hyperlink_idx > 0)
                    .map_or(0, |i| i + 1);
                let take_len = if was_wrapped { row.len() } else { content_len };

                current_logical.extend_from_slice(&row[..take_len]);

                if !was_wrapped {
                    logical_lines.push(std::mem::take(&mut current_logical));
                }
            }
            if !current_logical.is_empty() {
                logical_lines.push(current_logical);
            }

            // 2. Re-wrap logical lines to new_cols
            let mut new_physical_lines: Vec<Vec<Cell>> = Vec::new();
            for mut logical in logical_lines {
                if logical.is_empty() {
                    new_physical_lines.push(vec![Cell::default(); new_cols]);
                    continue;
                }
                while !logical.is_empty() {
                    if logical.len() > new_cols {
                        let mut row: Vec<Cell> = logical.drain(..new_cols).collect();
                        if let Some(last) = row.last_mut() {
                            last.style.set_wrapped(true);
                        }
                        new_physical_lines.push(row);
                    } else {
                        let mut row = std::mem::take(&mut logical);
                        row.resize(new_cols, Cell::default());
                        new_physical_lines.push(row);
                    }
                }
            }

            // 3. Ensure minimum height
            while new_physical_lines.len() < new_rows {
                new_physical_lines.push(vec![Cell::default(); new_cols]);
            }

            // 4. Distribute into visible cells (last new_rows) and scrollback (preceding)
            let split_idx = new_physical_lines.len().saturating_sub(new_rows);
            let visible_part = new_physical_lines.split_off(split_idx);
            self.cells = visible_part;

            self.scrollback.clear();
            for line in new_physical_lines {
                if self.scrollback.len() >= self.max_scrollback {
                    self.scrollback.pop_front();
                }
                self.scrollback.push_back(line);
            }
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
        self.cursor_x = self.cursor_x.min(new_cols.saturating_sub(1));
        self.cursor_y = self.cursor_y.min(new_rows.saturating_sub(1));
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

    /// Prunes unreferenced hyperlinks from `self.hyperlinks` and remaps all active cell indices.
    ///
    /// This prevents unbounded memory growth from evicted scrollback lines and avoids exhausting
    /// the 16-bit `u16::MAX` index space during long-running terminal sessions.
    pub fn prune_hyperlinks(&mut self) {
        if self.hyperlinks.is_empty() {
            self.current_hyperlink_idx = 0;
            self.last_hyperlink_prune_len = 0;
            return;
        }

        let old_count = self.hyperlinks.len();
        let mut used = vec![false; old_count + 1];

        // 1. Mark currently active OSC 8 hyperlink if open
        let cur = self.current_hyperlink_idx as usize;
        if cur > 0 && cur <= old_count {
            used[cur] = true;
        }

        // 2. Mark visible screen cells
        for row in &self.cells {
            for cell in row {
                let idx = cell.hyperlink_idx as usize;
                if idx > 0 && idx <= old_count {
                    used[idx] = true;
                }
            }
        }

        // 3. Mark scrollback cells
        for row in &self.scrollback {
            for cell in row {
                let idx = cell.hyperlink_idx as usize;
                if idx > 0 && idx <= old_count {
                    used[idx] = true;
                }
            }
        }

        // 4. Mark alternate screen cells if present
        if let Some(ref alt) = self.alt_screen {
            for row in alt {
                for cell in row {
                    let idx = cell.hyperlink_idx as usize;
                    if idx > 0 && idx <= old_count {
                        used[idx] = true;
                    }
                }
            }
        }

        // Count how many URLs are actively referenced
        let used_count = used[1..].iter().filter(|&&u| u).count();
        if used_count == old_count {
            self.last_hyperlink_prune_len = old_count;
            return;
        }

        // Build remapping table: old 1-based index -> new 1-based index (0 if dead)
        let mut remap = vec![0u16; old_count + 1];
        let mut new_hyperlinks = Vec::with_capacity(used_count);

        for old_idx in 1..=old_count {
            if used[old_idx] {
                new_hyperlinks.push(std::mem::take(&mut self.hyperlinks[old_idx - 1]));
                remap[old_idx] = new_hyperlinks.len() as u16;
            }
        }

        self.hyperlinks = new_hyperlinks;
        self.last_hyperlink_prune_len = self.hyperlinks.len();

        // Remap current active hyperlink index
        if cur > 0 && cur <= old_count {
            self.current_hyperlink_idx = remap[cur];
        } else {
            self.current_hyperlink_idx = 0;
        }

        // Remap visible screen cells
        for row in &mut self.cells {
            for cell in row {
                let idx = cell.hyperlink_idx as usize;
                if idx > 0 && idx <= old_count {
                    cell.hyperlink_idx = remap[idx];
                } else if idx > old_count {
                    cell.hyperlink_idx = 0;
                }
            }
        }

        // Remap scrollback cells
        for row in &mut self.scrollback {
            for cell in row {
                let idx = cell.hyperlink_idx as usize;
                if idx > 0 && idx <= old_count {
                    cell.hyperlink_idx = remap[idx];
                } else if idx > old_count {
                    cell.hyperlink_idx = 0;
                }
            }
        }

        // Remap alternate screen cells if present
        if let Some(ref mut alt) = self.alt_screen {
            for row in alt {
                for cell in row {
                    let idx = cell.hyperlink_idx as usize;
                    if idx > 0 && idx <= old_count {
                        cell.hyperlink_idx = remap[idx];
                    } else if idx > old_count {
                        cell.hyperlink_idx = 0;
                    }
                }
            }
        }
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
    fn test_hyperlinks_pruning_and_scrollback_eviction() {
        let mut grid = TerminalGrid::new(30, 2);
        grid.max_scrollback = 2;

        // Line 1: link 1 (destined to be evicted)
        grid.advance_bytes(b"\x1b]8;;https://dead.com\x1b\\dead\x1b]8;;\x1b\\\r\n");
        // Line 2: link 2 (destined to be kept)
        grid.advance_bytes(b"\x1b]8;;https://alive.com\x1b\\alive\x1b]8;;\x1b\\\r\n");

        assert_eq!(grid.hyperlinks.len(), 2);

        // Scroll 5 more blank lines to completely evict line 1 from scrollback (max_scrollback = 2)
        for _ in 0..5 {
            grid.advance_bytes(b"line\r\n");
        }

        // Prune unreferenced links
        grid.prune_hyperlinks();

        // dead.com was evicted from scrollback and screen, so only alive.com or neither remains
        assert!(!grid.hyperlinks.iter().any(|u| u == "https://dead.com"));

        // Test CSI 3 J scrollback pruning:
        // Add a link that stays in scrollback
        let mut grid2 = TerminalGrid::new(30, 2);
        grid2.max_scrollback = 5;
        grid2.advance_bytes(b"\x1b]8;;https://only-in-scrollback.com\x1b\\sb\x1b]8;;\x1b\\\r\n\r\n\r\n");
        assert_eq!(grid2.hyperlinks.len(), 1);

        // Test CSI 3 J scrollback pruning:
        // Add a link that gets pushed into scrollback
        let mut grid2 = TerminalGrid::new(30, 5);
        grid2.max_scrollback = 10;
        grid2.advance_bytes(b"\x1b]8;;https://only-in-scrollback.com\x1b\\sb\x1b]8;;\x1b\\\r\n");
        // Print 7 lines so "sb" is pushed up into scrollback and completely leaves the 5 visible screen rows
        for _ in 0..7 {
            grid2.advance_bytes(b"content\r\n");
        }
        assert!(!grid2.scrollback.is_empty());
        assert_eq!(grid2.hyperlinks.len(), 1);

        // Clear scrollback with CSI 3 J
        grid2.advance_bytes(b"\x1b[3J");
        assert_eq!(grid2.scrollback.len(), 0);
        // Scrollback cleared -> prune_hyperlinks called automatically -> hyperlinks table emptied
        assert_eq!(grid2.hyperlinks.len(), 0);
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

    #[test]
    fn test_csi_2j_and_3j_standards() {
        let mut grid = TerminalGrid::new(20, 5);
        grid.advance_bytes(b"line 1\r\nline 2\r\nline 3\r\nline 4\r\nline 5\r\nline 6");
        assert!(!grid.scrollback.is_empty());

        // Position cursor at (5, 2)
        grid.advance_bytes(b"\x1b[3;6H");
        assert_eq!(grid.cursor_x, 5);
        assert_eq!(grid.cursor_y, 2);

        // CSI 2 J clears screen cells but preserves cursor position per ECMA-48
        grid.advance_bytes(b"\x1b[2J");
        assert_eq!(grid.cursor_x, 5);
        assert_eq!(grid.cursor_y, 2);
        assert_eq!(grid.cells[0][0].c, ' ');

        // Type new content
        grid.advance_bytes(b"hello");
        assert_eq!(grid.cursor_x, 10);
        assert_eq!(grid.cells[2][5].c, 'h');

        // CSI 3 J clears scrollback only, keeping visible screen and cursor
        grid.advance_bytes(b"\x1b[3J");
        assert_eq!(grid.scrollback.len(), 0);
        assert_eq!(grid.cursor_x, 10);
        assert_eq!(grid.cells[2][5].c, 'h');
    }

    #[test]
    fn test_osc_color_queries_and_osc52() {
        let mut grid = TerminalGrid::new(20, 5);

        // OSC 10 query -> returns foreground rgb response
        grid.advance_bytes(b"\x1b]10;?\x1b\\");
        assert_eq!(grid.responses.pop().unwrap(), b"\x1b]10;rgb:dc00/d700/ba00\x1b\\");

        // OSC 11 query -> returns background rgb response
        grid.advance_bytes(b"\x1b]11;?\x1b\\");
        assert_eq!(grid.responses.pop().unwrap(), b"\x1b]11;rgb:1f00/1f00/2800\x1b\\");

        // OSC 52 set clipboard: base64 of "lunula" is "bHVudWxh"
        grid.advance_bytes(b"\x1b]52;c;bHVudWxh\x1b\\");
        #[cfg(windows)]
        {
            if let Some(text) = crate::platform::clipboard::get_text() {
                assert_eq!(text, "lunula");
            }
        }

        // OSC 52 query clipboard: disabled by default (security protection against exfiltration)
        assert!(!grid.allow_clipboard_read);
        grid.advance_bytes(b"\x1b]52;c;?\x1b\\");
        assert!(grid.responses.is_empty(), "OSC 52 read must be blocked when allow_clipboard_read is false");

        // OSC 52 query clipboard: when explicitly allowed
        #[cfg(windows)]
        {
            grid.allow_clipboard_read = true;
            grid.advance_bytes(b"\x1b]52;c;?\x1b\\");
            let resp = grid.responses.pop().expect("response when allow_clipboard_read is true");
            assert!(resp.starts_with(b"\x1b]52;c;"));
            assert!(resp.ends_with(b"\x1b\\"));
        }
    }

    #[test]
    fn test_terminal_line_reflow_on_resize() {
        let mut grid = TerminalGrid::new(15, 5);
        // Print 25 characters (exceeds width 15, wraps 15 on row 0, 10 on row 1)
        grid.advance_bytes(b"abcdefghijklmnopqrstuvwxy");
        assert_eq!(grid.cells[0][0].c, 'a');
        assert_eq!(grid.cells[0][14].c, 'o');
        assert!(grid.cells[0][14].style.wrapped());
        assert_eq!(grid.cells[1][0].c, 'p');
        assert_eq!(grid.cells[1][9].c, 'y');

        // Expand width to 30: lines should reflow and unwrap into row 0!
        grid.resize(30, 5);
        assert_eq!(grid.cols, 30);
        let row0: String = grid.cells[0].iter().map(|c| c.c).collect();
        assert!(row0.starts_with("abcdefghijklmnopqrstuvwxy"));

        // Shrink width back down to 15: lines should re-wrap without losing characters!
        grid.resize(15, 5);
        assert_eq!(grid.cols, 15);
        assert_eq!(grid.cells[0][0].c, 'a');
        assert_eq!(grid.cells[0][14].c, 'o');
        assert_eq!(grid.cells[1][0].c, 'p');
        assert_eq!(grid.cells[1][9].c, 'y');
    }

    #[test]
    fn test_combining_diacritics_and_zero_width() {
        let mut grid = TerminalGrid::new(20, 5);
        // 'e' followed by combining acute '\u{0301}'
        grid.advance_bytes("e\u{0301}".as_bytes());
        assert_eq!(grid.cells[0][0].c, 'e');
        let extras = grid.combining_chars.get(&(0, 0)).expect("combining char attached");
        assert_eq!(extras, &vec!['\u{0301}']);

        // Selection should preserve the combining character
        let pt1 = crate::terminal::selection::SelectionPoint { col: 0, abs_row: 0 };
        let pt2 = crate::terminal::selection::SelectionPoint { col: 1, abs_row: 0 };
        grid.selection = Some(crate::terminal::selection::Selection {
            anchor: pt1,
            active: pt2,
        });
        let text = grid.get_selected_text().unwrap();
        assert_eq!(text, "e\u{0301}");
    }

    #[test]
    fn test_dynamic_osc_theme_colors() {
        let mut grid = TerminalGrid::new(20, 5);
        grid.set_theme_colors(
            ColorRgb { r: 10, g: 20, b: 30 },
            ColorRgb { r: 40, g: 50, b: 60 },
        );

        grid.advance_bytes(b"\x1b]10;?\x1b\\");
        assert_eq!(grid.responses.pop().unwrap(), b"\x1b]10;rgb:0a00/1400/1e00\x1b\\");

        grid.advance_bytes(b"\x1b]11;?\x1b\\");
        assert_eq!(grid.responses.pop().unwrap(), b"\x1b]11;rgb:2800/3200/3c00\x1b\\");
    }

    #[test]
    fn test_dec_1004_focus_reporting() {
        let mut grid = TerminalGrid::new(20, 5);
        assert!(!grid.focus_reporting);

        // Enable focus reporting
        grid.advance_bytes(b"\x1b[?1004h");
        assert!(grid.focus_reporting);

        // Query status via DECRQM
        grid.advance_bytes(b"\x1b[?1004$p");
        assert_eq!(grid.responses.pop().unwrap(), b"\x1b[?1004;1$y");

        // Disable focus reporting
        grid.advance_bytes(b"\x1b[?1004l");
        assert!(!grid.focus_reporting);
    }
}
