use unicode_width::UnicodeWidthChar;
use vte::{Params, Parser, Perform};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub fn ansi_16_color(idx: usize) -> ColorRgb {
    // Catppuccin Mocha 16-color palette
    const PALETTE: [ColorRgb; 16] = [
        ColorRgb { r: 69, g: 71, b: 90 },    // 0: Black (Surface 1)
        ColorRgb { r: 243, g: 139, b: 168 }, // 1: Red
        ColorRgb { r: 166, g: 227, b: 161 }, // 2: Green
        ColorRgb { r: 249, g: 226, b: 175 }, // 3: Yellow
        ColorRgb { r: 137, g: 180, b: 250 }, // 4: Blue
        ColorRgb { r: 245, g: 194, b: 231 }, // 5: Magenta (Pink)
        ColorRgb { r: 148, g: 226, b: 213 }, // 6: Cyan (Teal)
        ColorRgb { r: 186, g: 194, b: 222 }, // 7: White (Subtext 1)
        ColorRgb { r: 88, g: 91, b: 112 },   // 8: Bright Black (Surface 2)
        ColorRgb { r: 235, g: 160, b: 172 }, // 9: Bright Red (Maroon)
        ColorRgb { r: 166, g: 227, b: 161 }, // 10: Bright Green
        ColorRgb { r: 249, g: 226, b: 175 }, // 11: Bright Yellow
        ColorRgb { r: 137, g: 220, b: 235 }, // 12: Bright Blue (Sky)
        ColorRgb { r: 203, g: 166, b: 247 }, // 13: Bright Magenta (Mauve)
        ColorRgb { r: 137, g: 220, b: 235 }, // 14: Bright Cyan (Sky)
        ColorRgb { r: 205, g: 214, b: 244 }, // 15: Bright White (Text)
    ];
    PALETTE[idx.min(15)]
}

pub fn color_256(idx: u8) -> ColorRgb {
    if idx < 16 {
        return ansi_16_color(idx as usize);
    }
    if idx >= 232 {
        let gray = 8 + (idx - 232) * 10;
        return ColorRgb { r: gray, g: gray, b: gray };
    }
    let idx = idx - 16;
    let b_idx = idx % 6;
    let g_idx = (idx / 6) % 6;
    let r_idx = idx / 36;
    let to_val = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
    ColorRgb {
        r: to_val(r_idx),
        g: to_val(g_idx),
        b: to_val(b_idx),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorShape {
    Block,
    Underline,
    Beam,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorConfig {
    pub shape: CursorShape,
    pub blinking: bool,
    pub color: ColorRgb,
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self {
            shape: CursorShape::Block,
            blinking: true,
            color: ColorRgb { r: 180, g: 190, b: 254 }, // Catppuccin Lavender
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellStyle {
    pub fg: Option<ColorRgb>,
    pub bg: Option<ColorRgb>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub undercurl: bool,
    pub strikethrough: bool,
    pub underline_color: Option<ColorRgb>,
    pub reverse: bool,
}

impl Default for CellStyle {
    fn default() -> Self {
        Self {
            fg: None,
            bg: None,
            bold: false,
            dim: false,
            italic: false,
            underline: false,
            undercurl: false,
            strikethrough: false,
            underline_color: None,
            reverse: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub c: char,
    pub style: CellStyle,
    pub hyperlink_idx: u16,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            c: ' ',
            style: CellStyle::default(),
            hyperlink_idx: 0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BgRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub color: ColorRgb,
    pub alpha: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectionPoint {
    pub col: usize,
    pub abs_row: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub anchor: SelectionPoint,
    pub active: SelectionPoint,
}

impl Selection {
    pub fn normalized(&self) -> (SelectionPoint, SelectionPoint) {
        if self.anchor.abs_row < self.active.abs_row
            || (self.anchor.abs_row == self.active.abs_row && self.anchor.col <= self.active.col)
        {
            (self.anchor, self.active)
        } else {
            (self.active, self.anchor)
        }
    }

    pub fn contains(&self, col: usize, abs_row: usize) -> bool {
        let (start, end) = self.normalized();
        if abs_row < start.abs_row || abs_row > end.abs_row {
            return false;
        }
        if start.abs_row == end.abs_row {
            col >= start.col && col <= end.col
        } else if abs_row == start.abs_row {
            col >= start.col
        } else if abs_row == end.abs_row {
            col <= end.col
        } else {
            true
        }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.active
    }
}

pub struct TerminalGrid {
    pub cols: usize,
    pub rows: usize,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub saved_cursor_x: usize,
    pub saved_cursor_y: usize,
    pub cursor_visible: bool,
    pub cursor_config: CursorConfig,
    pub title: String,
    pub app_cursor_keys: bool,
    pub cells: Vec<Vec<Cell>>,
    pub current_style: CellStyle,
    pub scrollback: Vec<Vec<Cell>>,
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
}

impl TerminalGrid {
    pub fn new(cols: usize, rows: usize) -> Self {
        let cols = cols.max(10);
        let rows = rows.max(5);
        let cells = vec![vec![Cell::default(); cols]; rows];
        let scroll_region_bottom = rows.saturating_sub(1);

        Self {
            cols,
            rows,
            cursor_x: 0,
            cursor_y: 0,
            saved_cursor_x: 0,
            saved_cursor_y: 0,
            cursor_visible: true,
            cursor_config: CursorConfig::default(),
            title: "Lunula Terminal".to_string(),
            app_cursor_keys: false,
            cells,
            current_style: CellStyle::default(),
            scrollback: Vec::new(),
            max_scrollback: 5000,
            scroll_offset: 0,
            target_scroll_y: 0.0,
            current_scroll_y: 0.0,
            selection: None,
            selection_color: ColorRgb {
                r: 88,
                g: 91,
                b: 112,
            },
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
        }
    }

    pub fn resize(&mut self, new_cols: usize, new_rows: usize) {
        let new_cols = new_cols.max(10);
        let new_rows = new_rows.max(5);

        let mut new_cells = vec![vec![Cell::default(); new_cols]; new_rows];
        for r in 0..self.rows.min(new_rows) {
            for c in 0..self.cols.min(new_cols) {
                new_cells[r][c] = self.cells[r][c];
            }
        }

        if let Some(alt) = &mut self.alt_screen {
            let mut new_alt = vec![vec![Cell::default(); new_cols]; new_rows];
            for r in 0..self.rows.min(new_rows) {
                for c in 0..self.cols.min(new_cols) {
                    new_alt[r][c] = alt[r][c];
                }
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
        self.dirty = true;
    }

    pub fn advance_bytes(&mut self, bytes: &[u8]) {
        let mut parser = std::mem::replace(&mut self.parser, Parser::new());
        parser.advance(self, bytes);
        self.parser = parser;
    }

    pub fn content_lines_in_scrollback(&self) -> usize {
        self.scrollback
            .iter()
            .rposition(|line| line.iter().any(|c| c.c != ' ' || c.style.bg.is_some()))
            .map(|idx| idx + 1)
            .unwrap_or(0)
    }

    pub fn scroll_region_up(&mut self) {
        if self.rows == 0 || self.scroll_region_top >= self.scroll_region_bottom || self.scroll_region_bottom >= self.rows {
            return;
        }

        if self.scroll_region_top == 0 && self.scroll_region_bottom == self.rows - 1 && !self.is_alt_screen {
            let line = self.cells.remove(0);
            let has_content = line.iter().any(|c| c.c != ' ' || c.style.bg.is_some());
            if has_content {
                self.scrollback.push(line);
                if self.scrollback.len() > self.max_scrollback {
                    self.scrollback.remove(0);
                    if let Some(sel) = &mut self.selection {
                        if sel.anchor.abs_row > 0 {
                            sel.anchor.abs_row -= 1;
                        }
                        if sel.active.abs_row > 0 {
                            sel.active.abs_row -= 1;
                        }
                    }
                }
            } else if !self.scrollback.is_empty() {
                let last_empty = self.scrollback.last().map_or(false, |l| {
                    !l.iter().any(|c| c.c != ' ' || c.style.bg.is_some())
                });
                if !last_empty && self.content_lines_in_scrollback() > 0 {
                    self.scrollback.push(line);
                }
            }
            self.cells.push(vec![Cell::default(); self.cols]);
        } else {
            self.cells.remove(self.scroll_region_top);
            self.cells.insert(self.scroll_region_bottom, vec![Cell::default(); self.cols]);
        }
        self.dirty = true;
    }

    pub fn scroll_region_down(&mut self) {
        if self.rows == 0 || self.scroll_region_top >= self.scroll_region_bottom || self.scroll_region_bottom >= self.rows {
            return;
        }
        self.cells.remove(self.scroll_region_bottom);
        self.cells.insert(self.scroll_region_top, vec![Cell::default(); self.cols]);
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
        let max_pixels = self.content_lines_in_scrollback() as f32 * cell_h;
        if max_pixels <= 0.0 {
            self.target_scroll_y = 0.0;
            self.current_scroll_y = 0.0;
            return;
        }
        self.target_scroll_y = (self.target_scroll_y + pixels).clamp(0.0, max_pixels);
        self.dirty = true;
    }

    pub fn scroll_down_pixels(&mut self, pixels: f32, cell_h: f32) {
        if self.is_alt_screen {
            return;
        }
        let max_pixels = self.content_lines_in_scrollback() as f32 * cell_h;
        if max_pixels <= 0.0 {
            self.target_scroll_y = 0.0;
            self.current_scroll_y = 0.0;
            return;
        }
        self.target_scroll_y = (self.target_scroll_y - pixels).clamp(0.0, max_pixels);
        self.dirty = true;
    }

    pub fn update_smooth_scroll(&mut self, dt: f32) -> bool {
        let diff = self.target_scroll_y - self.current_scroll_y;
        if diff.abs() > 0.05 {
            let factor = 1.0 - (-24.0 * dt).exp();
            self.current_scroll_y += diff * factor;
            self.dirty = true;
            true
        } else if self.current_scroll_y != self.target_scroll_y {
            self.current_scroll_y = self.target_scroll_y;
            self.dirty = true;
            true
        } else {
            false
        }
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
        self.target_scroll_y = 0.0;
        self.current_scroll_y = 0.0;
        self.dirty = true;
    }

    #[allow(dead_code)]
    pub fn visible_row(&self, screen_r: usize) -> &[Cell] {
        let effective_offset = if self.current_scroll_y > 0.0 {
            (self.current_scroll_y / 20.0).round() as usize
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
            (self.current_scroll_y / 20.0).round() as usize
        } else {
            self.scroll_offset
        };
        let start_idx = total_rows.saturating_sub(base_offset + self.rows);
        start_idx + screen_row
    }

    pub fn has_selection(&self) -> bool {
        self.selection.as_ref().map_or(false, |s| !s.is_empty())
    }

    pub fn clear_selection(&mut self) {
        if self.selection.is_some() {
            self.selection = None;
            self.dirty = true;
        }
    }

    pub fn select_word_at(&mut self, col: usize, abs_row: usize) {
        if let Some(row) = self.get_row(abs_row) {
            let content_len = self.abs_row_content_len(abs_row);
            if content_len == 0 || col >= row.len() {
                self.selection = None;
                self.dirty = true;
                return;
            }
            let col = col.min(content_len.saturating_sub(1));
            let target_char = row[col].c;
            let is_word_char = |c: char| c.is_alphanumeric() || c == '_' || c == '-';

            if target_char == ' ' {
                let mut start = col;
                while start > 0 && row[start - 1].c == ' ' {
                    start -= 1;
                }
                let mut end = col;
                while end + 1 < content_len && row[end + 1].c == ' ' {
                    end += 1;
                }
                self.selection = Some(Selection {
                    anchor: SelectionPoint { col: start, abs_row },
                    active: SelectionPoint { col: end, abs_row },
                });
            } else if is_word_char(target_char) {
                let mut start = col;
                while start > 0 && is_word_char(row[start - 1].c) {
                    start -= 1;
                }
                let mut end = col;
                while end + 1 < content_len && is_word_char(row[end + 1].c) {
                    end += 1;
                }
                self.selection = Some(Selection {
                    anchor: SelectionPoint { col: start, abs_row },
                    active: SelectionPoint { col: end, abs_row },
                });
            } else {
                self.selection = Some(Selection {
                    anchor: SelectionPoint { col, abs_row },
                    active: SelectionPoint { col, abs_row },
                });
            }
            self.dirty = true;
        }
    }

    pub fn select_line_at(&mut self, abs_row: usize) {
        let content_len = self.abs_row_content_len(abs_row);
        if content_len > 0 {
            self.selection = Some(Selection {
                anchor: SelectionPoint { col: 0, abs_row },
                active: SelectionPoint { col: content_len.saturating_sub(1), abs_row },
            });
            self.dirty = true;
        }
    }

    pub fn get_hyperlink_at(&self, col: usize, abs_row: usize) -> Option<String> {
        let row = self.get_row(abs_row)?;
        let cell = row.get(col)?;
        if cell.hyperlink_idx > 0 {
            if let Some(url) = self.hyperlinks.get(cell.hyperlink_idx as usize - 1) {
                return Some(url.clone());
            }
        }

        // Fallback: detect plaintext URL (http:// or https://) on this row
        let text: String = row.iter().map(|c| c.c).collect();
        for (start_idx, _) in text.match_indices("http://").chain(text.match_indices("https://")) {
            let end_idx = text[start_idx..]
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ')' || c == '>')
                .map(|len| start_idx + len)
                .unwrap_or(text.len());
            if col >= start_idx && col < end_idx {
                return Some(text[start_idx..end_idx].to_string());
            }
        }
        None
    }

    pub fn get_row(&self, abs_row: usize) -> Option<&Vec<Cell>> {
        let sb_len = self.scrollback.len();
        if abs_row < sb_len {
            self.scrollback.get(abs_row)
        } else {
            self.cells.get(abs_row - sb_len)
        }
    }

    pub fn abs_row_content_len(&self, abs_row: usize) -> usize {
        match self.get_row(abs_row) {
            Some(row) => row
                .iter()
                .rposition(|cell| cell.c != ' ' || cell.style.bg.is_some())
                .map(|i| i + 1)
                .unwrap_or(0),
            None => 0,
        }
    }

    pub fn last_content_abs_row(&self) -> Option<usize> {
        let sb_len = self.scrollback.len();
        for (r_idx, row) in self.cells.iter().enumerate().rev() {
            if row.iter().any(|c| c.c != ' ' || c.style.bg.is_some()) {
                return Some(sb_len + r_idx);
            }
        }
        for (r_idx, row) in self.scrollback.iter().enumerate().rev() {
            if row.iter().any(|c| c.c != ' ' || c.style.bg.is_some()) {
                return Some(r_idx);
            }
        }
        None
    }

    pub fn first_content_abs_row(&self) -> Option<usize> {
        let sb_len = self.scrollback.len();
        for (r_idx, row) in self.scrollback.iter().enumerate() {
            if row.iter().any(|c| c.c != ' ' || c.style.bg.is_some()) {
                return Some(r_idx);
            }
        }
        for (r_idx, row) in self.cells.iter().enumerate() {
            if row.iter().any(|c| c.c != ' ' || c.style.bg.is_some()) {
                return Some(sb_len + r_idx);
            }
        }
        None
    }

    pub fn get_selected_text(&self) -> Option<String> {
        let sel = self.selection.as_ref()?;
        if sel.is_empty() {
            return None;
        }

        let (start, end) = sel.normalized();
        let mut result = String::new();

        for r in start.abs_row..=end.abs_row {
            if let Some(row) = self.get_row(r) {
                let content_len = row
                    .iter()
                    .rposition(|cell| cell.c != ' ' || cell.style.bg.is_some())
                    .map(|i| i + 1)
                    .unwrap_or(0);

                if content_len == 0 {
                    if r != end.abs_row {
                        result.push_str("\r\n");
                    }
                    continue;
                }

                let col_start = if r == start.abs_row { start.col } else { 0 };
                let col_end = if r == end.abs_row {
                    end.col.min(content_len.saturating_sub(1))
                } else {
                    content_len.saturating_sub(1)
                };

                if col_start <= col_end {
                    let mut line = String::new();
                    for c in col_start..=col_end {
                        if c < row.len() {
                            line.push(row[c].c);
                        }
                    }
                    let trimmed = line.trim_end_matches(' ');
                    result.push_str(trimmed);
                }

                if r != end.abs_row {
                    result.push_str("\r\n");
                }
            }
        }

        let trimmed_result = result.trim_end_matches(|c| c == '\r' || c == '\n').to_string();
        if trimmed_result.is_empty() {
            None
        } else {
            Some(trimmed_result)
        }
    }
}

fn render_procedural_glyph(
    c: char,
    x0: f32,
    x1: f32,
    y0: f32,
    y1: f32,
    fg: ColorRgb,
    bg_rects: &mut Vec<BgRect>,
) -> bool {
    let w = x1 - x0;
    let h = y1 - y0;
    let mid_x = ((x0 + x1) * 0.5).round();
    let mid_y = ((y0 + y1) * 0.5).round();
    let line_thick = (h * 0.12).max(1.5).round();
    let half_thick = (line_thick * 0.5).round();

    match c {
        // Horizontal box lines
        '─' | '━' => {
            bg_rects.push(BgRect {
                x: x0,
                y: mid_y - half_thick,
                w,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        // Vertical box lines
        '│' | '┃' => {
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: y0,
                w: line_thick,
                h,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        // Corners
        '┌' | '┏' => {
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: mid_y - half_thick,
                w: x1 - mid_x + half_thick,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: mid_y - half_thick,
                w: line_thick,
                h: y1 - mid_y + half_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        '┐' | '┓' => {
            bg_rects.push(BgRect {
                x: x0,
                y: mid_y - half_thick,
                w: mid_x - x0 + half_thick,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: mid_y - half_thick,
                w: line_thick,
                h: y1 - mid_y + half_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        '└' | '┗' => {
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: mid_y - half_thick,
                w: x1 - mid_x + half_thick,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: y0,
                w: line_thick,
                h: mid_y - y0 + half_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        '┘' | '┛' => {
            bg_rects.push(BgRect {
                x: x0,
                y: mid_y - half_thick,
                w: mid_x - x0 + half_thick,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: y0,
                w: line_thick,
                h: mid_y - y0 + half_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        // Intersections
        '├' | '┣' => {
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: y0,
                w: line_thick,
                h,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: mid_y - half_thick,
                w: x1 - mid_x + half_thick,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        '┤' | '┫' => {
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: y0,
                w: line_thick,
                h,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: x0,
                y: mid_y - half_thick,
                w: mid_x - x0 + half_thick,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        '┬' | '┳' => {
            bg_rects.push(BgRect {
                x: x0,
                y: mid_y - half_thick,
                w,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: mid_y - half_thick,
                w: line_thick,
                h: y1 - mid_y + half_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        '┴' | '┻' => {
            bg_rects.push(BgRect {
                x: x0,
                y: mid_y - half_thick,
                w,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: y0,
                w: line_thick,
                h: mid_y - y0 + half_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        '┼' | '╋' => {
            bg_rects.push(BgRect {
                x: x0,
                y: mid_y - half_thick,
                w,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: y0,
                w: line_thick,
                h,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        // Powerline right arrow solid (\u{E0B0})
        '\u{E0B0}' => {
            let steps = 12;
            for k in 0..steps {
                let t = k as f32 / steps as f32;
                let sx0 = x0 + t * w;
                let sx1 = x0 + (k + 1) as f32 / steps as f32 * w;
                let half_h = (1.0 - t) * (h * 0.5);
                bg_rects.push(BgRect {
                    x: sx0,
                    y: mid_y - half_h,
                    w: (sx1 - sx0 + 0.5).max(1.0),
                    h: (half_h * 2.0).max(1.0),
                    color: fg,
                    alpha: 1.0,
                });
            }
            true
        }
        // Powerline left arrow solid (\u{E0B2})
        '\u{E0B2}' => {
            let steps = 12;
            for k in 0..steps {
                let t = k as f32 / steps as f32;
                let sx0 = x0 + t * w;
                let sx1 = x0 + (k + 1) as f32 / steps as f32 * w;
                let half_h = t * (h * 0.5);
                bg_rects.push(BgRect {
                    x: sx0,
                    y: mid_y - half_h,
                    w: (sx1 - sx0 + 0.5).max(1.0),
                    h: (half_h * 2.0).max(1.0),
                    color: fg,
                    alpha: 1.0,
                });
            }
            true
        }
        // Powerline left bubble round cap (\u{E0B6})
        '\u{E0B6}' => {
            let steps = 12;
            for k in 0..steps {
                let t = k as f32 / steps as f32;
                let sx0 = x0 + t * w;
                let sx1 = x0 + (k + 1) as f32 / steps as f32 * w;
                let arc = (t * (2.0 - t)).max(0.0).sqrt();
                let half_h = arc * (h * 0.5);
                bg_rects.push(BgRect {
                    x: sx0,
                    y: mid_y - half_h,
                    w: (sx1 - sx0 + 0.5).max(1.0),
                    h: (half_h * 2.0).max(1.0),
                    color: fg,
                    alpha: 1.0,
                });
            }
            true
        }
        // Powerline right bubble round cap (\u{E0B4})
        '\u{E0B4}' => {
            let steps = 12;
            for k in 0..steps {
                let t = 1.0 - (k as f32 / steps as f32);
                let sx0 = x0 + (k as f32 / steps as f32) * w;
                let sx1 = x0 + ((k + 1) as f32 / steps as f32) * w;
                let arc = (t * (2.0 - t)).max(0.0).sqrt();
                let half_h = arc * (h * 0.5);
                bg_rects.push(BgRect {
                    x: sx0,
                    y: mid_y - half_h,
                    w: (sx1 - sx0 + 0.5).max(1.0),
                    h: (half_h * 2.0).max(1.0),
                    color: fg,
                    alpha: 1.0,
                });
            }
            true
        }
        // Rounded corners
        '╭' => {
            bg_rects.push(BgRect {
                x: mid_x + half_thick,
                y: mid_y - half_thick,
                w: x1 - (mid_x + half_thick),
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: mid_y + half_thick,
                w: line_thick,
                h: y1 - (mid_y + half_thick),
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick * 0.5,
                y: mid_y - half_thick * 0.5,
                w: line_thick,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        '╮' => {
            bg_rects.push(BgRect {
                x: x0,
                y: mid_y - half_thick,
                w: (mid_x - half_thick - x0).max(1.0),
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: mid_y + half_thick,
                w: line_thick,
                h: y1 - (mid_y + half_thick),
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick * 0.5,
                y: mid_y - half_thick * 0.5,
                w: line_thick,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        '╯' => {
            bg_rects.push(BgRect {
                x: x0,
                y: mid_y - half_thick,
                w: (mid_x - half_thick - x0).max(1.0),
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: y0,
                w: line_thick,
                h: (mid_y - half_thick - y0).max(1.0),
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick * 0.5,
                y: mid_y - half_thick * 0.5,
                w: line_thick,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        '╰' => {
            bg_rects.push(BgRect {
                x: mid_x + half_thick,
                y: mid_y - half_thick,
                w: x1 - (mid_x + half_thick),
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: y0,
                w: line_thick,
                h: (mid_y - half_thick - y0).max(1.0),
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick * 0.5,
                y: mid_y - half_thick * 0.5,
                w: line_thick,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        // Double lines
        '═' => {
            let offset = (line_thick * 0.9).max(2.0);
            let d_thick = (line_thick * 0.7).max(1.0);
            bg_rects.push(BgRect { x: x0, y: mid_y - offset, w, h: d_thick, color: fg, alpha: 1.0 });
            bg_rects.push(BgRect { x: x0, y: mid_y + offset - d_thick, w, h: d_thick, color: fg, alpha: 1.0 });
            true
        }
        '║' => {
            let offset = (line_thick * 0.9).max(2.0);
            let d_thick = (line_thick * 0.7).max(1.0);
            bg_rects.push(BgRect { x: mid_x - offset, y: y0, w: d_thick, h, color: fg, alpha: 1.0 });
            bg_rects.push(BgRect { x: mid_x + offset - d_thick, y: y0, w: d_thick, h, color: fg, alpha: 1.0 });
            true
        }
        // Solid block elements
        '█' => {
            bg_rects.push(BgRect { x: x0, y: y0, w, h, color: fg, alpha: 1.0 });
            true
        }
        '▀' => {
            bg_rects.push(BgRect { x: x0, y: y0, w, h: (h * 0.5).round(), color: fg, alpha: 1.0 });
            true
        }
        '▄' => {
            bg_rects.push(BgRect { x: x0, y: mid_y, w, h: y1 - mid_y, color: fg, alpha: 1.0 });
            true
        }
        '▌' => {
            bg_rects.push(BgRect { x: x0, y: y0, w: (w * 0.5).round(), h, color: fg, alpha: 1.0 });
            true
        }
        '▐' => {
            bg_rects.push(BgRect { x: mid_x, y: y0, w: x1 - mid_x, h, color: fg, alpha: 1.0 });
            true
        }
        '░' => {
            bg_rects.push(BgRect { x: x0, y: y0, w, h, color: fg, alpha: 0.25 });
            true
        }
        '▒' => {
            bg_rects.push(BgRect { x: x0, y: y0, w, h, color: fg, alpha: 0.50 });
            true
        }
        '▓' => {
            bg_rects.push(BgRect { x: x0, y: y0, w, h, color: fg, alpha: 0.75 });
            true
        }
        // Thin powerlines
        '\u{E0B1}' => {
            let line_w = (line_thick * 0.8).max(1.0);
            let steps = 8;
            for k in 0..steps {
                let t = k as f32 / steps as f32;
                let sx0 = x0 + t * w;
                let half_h = (1.0 - t) * (h * 0.5);
                bg_rects.push(BgRect { x: sx0, y: mid_y - half_h, w: line_w, h: line_w, color: fg, alpha: 1.0 });
                bg_rects.push(BgRect { x: sx0, y: mid_y + half_h - line_w, w: line_w, h: line_w, color: fg, alpha: 1.0 });
            }
            true
        }
        '\u{E0B3}' => {
            let line_w = (line_thick * 0.8).max(1.0);
            let steps = 8;
            for k in 0..steps {
                let t = k as f32 / steps as f32;
                let sx0 = x0 + t * w;
                let half_h = t * (h * 0.5);
                bg_rects.push(BgRect { x: sx0, y: mid_y - half_h, w: line_w, h: line_w, color: fg, alpha: 1.0 });
                bg_rects.push(BgRect { x: sx0, y: mid_y + half_h - line_w, w: line_w, h: line_w, color: fg, alpha: 1.0 });
            }
            true
        }
        _ => false,
    }
}

impl TerminalGrid {
    pub fn render_spans_and_backgrounds(
        &self,
        cell_w: f32,
        cell_h: f32,
        pad_x: f32,
        pad_y: f32,
    ) -> (Vec<(String, CellStyle)>, Vec<BgRect>) {
        let mut spans = Vec::new();
        let mut bg_rects = Vec::new();

        let base_offset = (self.current_scroll_y / cell_h).floor() as usize;
        let effective_base = if self.current_scroll_y > 0.0 {
            base_offset
        } else {
            self.scroll_offset
        };
        let show_cursor = self.cursor_visible && effective_base == 0;

        for r in 0..self.rows {
            if r > 0 {
                spans.push(("\n".to_string(), CellStyle::default()));
            }

            let abs_r = self.screen_to_abs_row(r);
            let row = self.visible_row_at(r, effective_base);
            let content_len = row
                .iter()
                .rposition(|cell| cell.c != ' ' || cell.style.bg.is_some())
                .map(|i| i + 1)
                .unwrap_or(0);
            let mut end = content_len;
            if show_cursor && r == self.cursor_y && self.cursor_x >= end {
                end = self.cursor_x + 1;
            }

            let mut cur_text = String::new();
            let mut cur_style = CellStyle::default();

            let row_y = pad_y + r as f32 * cell_h;

            for c in 0..end {
                let cell = if c < row.len() { row[c] } else { Cell::default() };
                let is_cursor = show_cursor && r == self.cursor_y && c == self.cursor_x;
                let is_selected = c < content_len
                    && self
                        .selection
                        .as_ref()
                        .map_or(false, |s| !s.is_empty() && s.contains(c, abs_r));

                let mut eff_style = cell.style;
                if is_selected {
                    eff_style.bg = Some(self.selection_color);
                    eff_style.fg = Some(ColorRgb { r: 245, g: 245, b: 255 });
                } else if eff_style.reverse {
                    std::mem::swap(&mut eff_style.fg, &mut eff_style.bg);
                }

                // Pixel snap to eliminate 1px gap between Oh My Posh bubble/powerline segments
                let x0 = (pad_x + c as f32 * cell_w).round();
                let x1 = (pad_x + (c + 1) as f32 * cell_w).round();
                let y0 = row_y.round();
                let y1 = (row_y + cell_h).round();

                // 1. Cell background
                if let Some(bg) = eff_style.bg {
                    bg_rects.push(BgRect {
                        x: x0,
                        y: y0,
                        w: x1 - x0,
                        h: y1 - y0,
                        color: bg,
                        alpha: 1.0,
                    });
                }

                // 2. Block cursor cell styling (invert text to dark for readability)
                if is_cursor && self.cursor_config.shape == CursorShape::Block {
                    eff_style.fg = Some(ColorRgb { r: 17, g: 17, b: 27 });
                }

                // 3. Underline decoration
                if eff_style.underline {
                    let u_color = eff_style
                        .underline_color
                        .or(eff_style.fg)
                        .unwrap_or(ColorRgb { r: 205, g: 214, b: 244 });
                    bg_rects.push(BgRect {
                        x: x0,
                        y: y1 - 2.0,
                        w: x1 - x0,
                        h: 1.5,
                        color: u_color,
                        alpha: 1.0,
                    });
                }

                // 4. Strikethrough decoration
                if eff_style.strikethrough {
                    let s_color = eff_style.fg.unwrap_or(ColorRgb { r: 205, g: 214, b: 244 });
                    bg_rects.push(BgRect {
                        x: x0,
                        y: y0 + (y1 - y0) * 0.5 - 0.75,
                        w: x1 - x0,
                        h: 1.5,
                        color: s_color,
                        alpha: 1.0,
                    });
                }

                // 5. Undercurl decoration (Neovim LSP diagnostic wavy underline)
                if eff_style.undercurl {
                    let c_color = eff_style
                        .underline_color
                        .unwrap_or(ColorRgb { r: 243, g: 139, b: 168 });
                    let w = x1 - x0;
                    let mid_x = x0 + w * 0.5;
                    bg_rects.push(BgRect {
                        x: x0,
                        y: y1 - 3.5,
                        w: w * 0.5,
                        h: 1.5,
                        color: c_color,
                        alpha: 1.0,
                    });
                    bg_rects.push(BgRect {
                        x: mid_x,
                        y: y1 - 1.5,
                        w: w * 0.5,
                        h: 1.5,
                        color: c_color,
                        alpha: 1.0,
                    });
                }

                // Text glyph character is 100% pure (no fake cursor characters)
                let mut ch = cell.c;
                let glyph_color = eff_style.fg.unwrap_or(ColorRgb { r: 205, g: 214, b: 244 });
                if render_procedural_glyph(ch, x0, x1, y0, y1, glyph_color, &mut bg_rects) {
                    ch = ' ';
                }

                // Group consecutive matching styles
                if cur_text.is_empty() {
                    cur_style = eff_style;
                    cur_text.push(ch);
                } else if cur_style == eff_style {
                    cur_text.push(ch);
                } else {
                    spans.push((cur_text, cur_style));
                    cur_style = eff_style;
                    cur_text = String::new();
                    cur_text.push(ch);
                }
            }

            if !cur_text.is_empty() {
                spans.push((cur_text, cur_style));
            }
        }

        (spans, bg_rects)
    }

    fn handle_sgr(&mut self, params: &[u16]) {
        if params.is_empty() {
            self.current_style = CellStyle::default();
            return;
        }

        let mut i = 0;
        while i < params.len() {
            match params[i] {
                0 => {
                    self.current_style = CellStyle::default();
                    i += 1;
                }
                1 => {
                    self.current_style.bold = true;
                    i += 1;
                }
                2 => {
                    self.current_style.dim = true;
                    i += 1;
                }
                3 => {
                    self.current_style.italic = true;
                    i += 1;
                }
                4 => {
                    self.current_style.underline = true;
                    i += 1;
                }
                7 => {
                    self.current_style.reverse = true;
                    i += 1;
                }
                9 => {
                    self.current_style.strikethrough = true;
                    i += 1;
                }
                22 => {
                    self.current_style.bold = false;
                    self.current_style.dim = false;
                    i += 1;
                }
                23 => {
                    self.current_style.italic = false;
                    i += 1;
                }
                24 => {
                    self.current_style.underline = false;
                    self.current_style.undercurl = false;
                    i += 1;
                }
                27 => {
                    self.current_style.reverse = false;
                    i += 1;
                }
                29 => {
                    self.current_style.strikethrough = false;
                    i += 1;
                }
                30..=37 => {
                    self.current_style.fg = Some(ansi_16_color((params[i] - 30) as usize));
                    i += 1;
                }
                39 => {
                    self.current_style.fg = None;
                    i += 1;
                }
                40..=47 => {
                    self.current_style.bg = Some(ansi_16_color((params[i] - 40) as usize));
                    i += 1;
                }
                49 => {
                    self.current_style.bg = None;
                    i += 1;
                }
                90..=97 => {
                    self.current_style.fg = Some(ansi_16_color((params[i] - 90 + 8) as usize));
                    i += 1;
                }
                100..=107 => {
                    self.current_style.bg = Some(ansi_16_color((params[i] - 100 + 8) as usize));
                    i += 1;
                }
                38 => {
                    if i + 1 < params.len() {
                        if params[i + 1] == 2 && i + 4 < params.len() {
                            self.current_style.fg = Some(ColorRgb {
                                r: params[i + 2] as u8,
                                g: params[i + 3] as u8,
                                b: params[i + 4] as u8,
                            });
                            i += 5;
                        } else if params[i + 1] == 5 && i + 2 < params.len() {
                            self.current_style.fg = Some(color_256(params[i + 2] as u8));
                            i += 3;
                        } else {
                            i += 1;
                        }
                    } else {
                        i += 1;
                    }
                }
                48 => {
                    if i + 1 < params.len() {
                        if params[i + 1] == 2 && i + 4 < params.len() {
                            self.current_style.bg = Some(ColorRgb {
                                r: params[i + 2] as u8,
                                g: params[i + 3] as u8,
                                b: params[i + 4] as u8,
                            });
                            i += 5;
                        } else if params[i + 1] == 5 && i + 2 < params.len() {
                            self.current_style.bg = Some(color_256(params[i + 2] as u8));
                            i += 3;
                        } else {
                            i += 1;
                        }
                    } else {
                        i += 1;
                    }
                }
                58 => {
                    // Underline color (58;2;r;g;b or 58;5;idx)
                    if i + 1 < params.len() {
                        if params[i + 1] == 2 && i + 4 < params.len() {
                            self.current_style.underline_color = Some(ColorRgb {
                                r: params[i + 2] as u8,
                                g: params[i + 3] as u8,
                                b: params[i + 4] as u8,
                            });
                            i += 5;
                        } else if params[i + 1] == 5 && i + 2 < params.len() {
                            self.current_style.underline_color =
                                Some(color_256(params[i + 2] as u8));
                            i += 3;
                        } else {
                            i += 1;
                        }
                    } else {
                        i += 1;
                    }
                }
                59 => {
                    self.current_style.underline_color = None;
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }
    }
}

impl Perform for TerminalGrid {
    fn print(&mut self, c: char) {
        let w = c.width().unwrap_or(1);
        if w == 0 {
            return;
        }

        if self.cursor_x + w > self.cols {
            self.cursor_x = 0;
            if self.cursor_y == self.scroll_region_bottom {
                self.scroll_region_up();
            } else if self.cursor_y + 1 < self.rows {
                self.cursor_y += 1;
            }
        }

        if self.cursor_y < self.rows && self.cursor_x < self.cols {
            self.cells[self.cursor_y][self.cursor_x] = Cell {
                c,
                style: self.current_style,
                hyperlink_idx: self.current_hyperlink_idx,
            };
            for extra in 1..w {
                if self.cursor_x + extra < self.cols {
                    self.cells[self.cursor_y][self.cursor_x + extra] = Cell {
                        c: ' ',
                        style: self.current_style,
                        hyperlink_idx: self.current_hyperlink_idx,
                    };
                }
            }
            self.cursor_x += w;
            self.dirty = true;
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x0A | 0x0B | 0x0C => {
                if self.cursor_y == self.scroll_region_bottom {
                    self.scroll_region_up();
                } else if self.cursor_y + 1 < self.rows {
                    self.cursor_y += 1;
                }
                self.dirty = true;
            }
            0x0D => {
                self.cursor_x = 0;
                self.dirty = true;
            }
            0x08 => {
                if self.cursor_x > 0 {
                    self.cursor_x -= 1;
                    self.dirty = true;
                }
            }
            0x09 => {
                self.cursor_x = ((self.cursor_x / 8) + 1) * 8;
                if self.cursor_x >= self.cols {
                    self.cursor_x = self.cols.saturating_sub(1);
                }
                self.dirty = true;
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        match byte {
            b'7' => {
                self.saved_cursor_x = self.cursor_x;
                self.saved_cursor_y = self.cursor_y;
            }
            b'8' => {
                self.cursor_x = self.saved_cursor_x.min(self.cols.saturating_sub(1));
                self.cursor_y = self.saved_cursor_y.min(self.rows.saturating_sub(1));
                self.dirty = true;
            }
            b'M' => {
                if self.cursor_y == self.scroll_region_top {
                    self.scroll_region_down();
                } else if self.cursor_y > 0 {
                    self.cursor_y -= 1;
                }
                self.dirty = true;
            }
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if params.is_empty() {
            return;
        }
        if params[0] == b"0" || params[0] == b"2" {
            if params.len() >= 2 {
                if let Ok(title) = std::str::from_utf8(params[1]) {
                    self.title = title.to_string();
                    self.dirty = true;
                }
            }
        } else if params[0] == b"8" {
            // OSC 8 ; params ; url ST
            if params.len() >= 3 {
                let url = std::str::from_utf8(params[2]).unwrap_or("");
                if url.is_empty() {
                    self.current_hyperlink_idx = 0;
                } else {
                    let idx = (self.hyperlinks.len() + 1) as u16;
                    self.hyperlinks.push(url.to_string());
                    self.current_hyperlink_idx = idx;
                }
            } else {
                self.current_hyperlink_idx = 0;
            }
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        let is_private = intermediates.contains(&b'?');
        let is_secondary = intermediates.contains(&b'>');
        let mut it = params.iter();
        let p0 = it.next().and_then(|p| p.first()).copied().unwrap_or(0) as usize;
        let p1 = it.next().and_then(|p| p.first()).copied().unwrap_or(0) as usize;

        // Ignore Windows ConPTY win32-input-mode key reports if received
        if action == '_' {
            return;
        }

        // DECSCUSR: Set Cursor Style (\x1b[ <n> q)
        if action == 'q' {
            match p0 {
                0 | 1 => {
                    self.cursor_config.shape = CursorShape::Block;
                    self.cursor_config.blinking = true;
                }
                2 => {
                    self.cursor_config.shape = CursorShape::Block;
                    self.cursor_config.blinking = false;
                }
                3 => {
                    self.cursor_config.shape = CursorShape::Underline;
                    self.cursor_config.blinking = true;
                }
                4 => {
                    self.cursor_config.shape = CursorShape::Underline;
                    self.cursor_config.blinking = false;
                }
                5 => {
                    self.cursor_config.shape = CursorShape::Beam;
                    self.cursor_config.blinking = true;
                }
                6 => {
                    self.cursor_config.shape = CursorShape::Beam;
                    self.cursor_config.blinking = false;
                }
                _ => {}
            }
            self.dirty = true;
            return;
        }

        // DSR: Device Status Report
        if action == 'n' {
            if p0 == 6 {
                let resp = if is_private {
                    format!("\x1b[?{};{}R", self.cursor_y + 1, self.cursor_x + 1)
                } else {
                    format!("\x1b[{};{}R", self.cursor_y + 1, self.cursor_x + 1)
                };
                self.responses.push(resp.into_bytes());
                return;
            } else if p0 == 5 {
                self.responses.push(b"\x1b[0n".to_vec());
                return;
            }
        }

        // DA: Primary and Secondary Device Attributes
        if action == 'c' {
            if is_secondary {
                self.responses.push(b"\x1b[>0;10;1c".to_vec());
            } else {
                self.responses.push(b"\x1b[?1;2c".to_vec());
            }
            return;
        }

        // Window Size query
        if action == 't' && p0 == 18 {
            let resp = format!("\x1b[8;{};{}t", self.rows, self.cols);
            self.responses.push(resp.into_bytes());
            return;
        }

        if is_private {
            match action {
                'h' => match p0 {
                    25 => {
                        self.cursor_visible = true;
                        self.dirty = true;
                    }
                    1 => self.app_cursor_keys = true,
                    47 | 1047 | 1049 => {
                        if !self.is_alt_screen {
                            self.is_alt_screen = true;
                            self.saved_cursor_x = self.cursor_x;
                            self.saved_cursor_y = self.cursor_y;
                            let alt = vec![vec![Cell::default(); self.cols]; self.rows];
                            let primary = std::mem::replace(&mut self.cells, alt);
                            self.alt_screen = Some(primary);
                            self.cursor_x = 0;
                            self.cursor_y = 0;
                            self.scroll_region_top = 0;
                            self.scroll_region_bottom = self.rows.saturating_sub(1);
                            self.scroll_offset = 0;
                            self.target_scroll_y = 0.0;
                            self.current_scroll_y = 0.0;
                            self.dirty = true;
                        }
                    }
                    2004 => self.bracketed_paste = true,
                    1000 | 1002 | 1003 => self.mouse_reporting = true,
                    1006 => self.sgr_mouse = true,
                    _ => {}
                },
                'l' => match p0 {
                    25 => {
                        self.cursor_visible = false;
                        self.dirty = true;
                    }
                    1 => self.app_cursor_keys = false,
                    47 | 1047 | 1049 => {
                        if self.is_alt_screen {
                            self.is_alt_screen = false;
                            if let Some(primary) = self.alt_screen.take() {
                                self.cells = primary;
                            }
                            self.cursor_x = self.saved_cursor_x.min(self.cols.saturating_sub(1));
                            self.cursor_y = self.saved_cursor_y.min(self.rows.saturating_sub(1));
                            self.scroll_region_top = 0;
                            self.scroll_region_bottom = self.rows.saturating_sub(1);
                            self.scroll_offset = 0;
                            self.target_scroll_y = 0.0;
                            self.current_scroll_y = 0.0;
                            self.dirty = true;
                        }
                    }
                    2004 => self.bracketed_paste = false,
                    1000 | 1002 | 1003 => self.mouse_reporting = false,
                    1006 => self.sgr_mouse = false,
                    _ => {}
                },
                _ => {}
            }
            return;
        }

        match action {
            // SGR: Select Graphic Rendition (Colors & Styles)
            'm' => {
                // Check for undercurl 4:3
                for slice in params.iter() {
                    if slice.first() == Some(&4) && slice.get(1) == Some(&3) {
                        self.current_style.undercurl = true;
                    }
                }
                let flat: Vec<u16> =
                    params.iter().flat_map(|slice| slice.iter().copied()).collect();
                self.handle_sgr(&flat);
            }

            // Cursor Position
            'H' | 'f' => {
                let row = if p0 == 0 { 1 } else { p0 };
                let col = if p1 == 0 { 1 } else { p1 };
                self.cursor_y = (row - 1).min(self.rows.saturating_sub(1));
                self.cursor_x = (col - 1).min(self.cols.saturating_sub(1));
                self.dirty = true;
            }

            // Cursor Horizontal Absolute
            'G' | '`' => {
                let col = if p0 == 0 { 1 } else { p0 };
                self.cursor_x = (col - 1).min(self.cols.saturating_sub(1));
                self.dirty = true;
            }

            // Line Position Absolute
            'd' => {
                let row = if p0 == 0 { 1 } else { p0 };
                self.cursor_y = (row - 1).min(self.rows.saturating_sub(1));
                self.dirty = true;
            }

            // Cursor Next Line
            'E' => {
                let n = if p0 == 0 { 1 } else { p0 };
                self.cursor_x = 0;
                self.cursor_y = (self.cursor_y + n).min(self.rows.saturating_sub(1));
                self.dirty = true;
            }

            // Cursor Previous Line
            'F' => {
                let n = if p0 == 0 { 1 } else { p0 };
                self.cursor_x = 0;
                self.cursor_y = self.cursor_y.saturating_sub(n);
                self.dirty = true;
            }

            // Erase in Display
            'J' => {
                match p0 {
                    0 => {
                        if self.cursor_y < self.rows {
                            for c in self.cursor_x..self.cols {
                                self.cells[self.cursor_y][c] = Cell::default();
                            }
                            for r in (self.cursor_y + 1)..self.rows {
                                for c in 0..self.cols {
                                    self.cells[r][c] = Cell::default();
                                }
                            }
                        }
                    }
                    1 => {
                        for r in 0..self.cursor_y {
                            for c in 0..self.cols {
                                self.cells[r][c] = Cell::default();
                            }
                        }
                        if self.cursor_y < self.rows {
                            for c in 0..=self.cursor_x.min(self.cols - 1) {
                                self.cells[self.cursor_y][c] = Cell::default();
                            }
                        }
                    }
                    2 => {
                        for r in 0..self.rows {
                            for c in 0..self.cols {
                                self.cells[r][c] = Cell::default();
                            }
                        }
                        self.cursor_x = 0;
                        self.cursor_y = 0;
                    }
                    3 => {
                        for r in 0..self.rows {
                            for c in 0..self.cols {
                                self.cells[r][c] = Cell::default();
                            }
                        }
                        self.scrollback.clear();
                        self.scroll_offset = 0;
                        self.target_scroll_y = 0.0;
                        self.current_scroll_y = 0.0;
                        self.cursor_x = 0;
                        self.cursor_y = 0;
                    }
                    _ => {}
                }
                self.dirty = true;
            }

            // Erase in Line
            'K' => {
                if self.cursor_y < self.rows {
                    match p0 {
                        0 => {
                            for c in self.cursor_x..self.cols {
                                self.cells[self.cursor_y][c] = Cell {
                                    c: ' ',
                                    style: self.current_style,
                                    hyperlink_idx: 0,
                                };
                            }
                        }
                        1 => {
                            for c in 0..=self.cursor_x.min(self.cols - 1) {
                                self.cells[self.cursor_y][c] = Cell {
                                    c: ' ',
                                    style: self.current_style,
                                    hyperlink_idx: 0,
                                };
                            }
                        }
                        2 => {
                            for c in 0..self.cols {
                                self.cells[self.cursor_y][c] = Cell {
                                    c: ' ',
                                    style: self.current_style,
                                    hyperlink_idx: 0,
                                };
                            }
                        }
                        _ => {}
                    }
                    self.dirty = true;
                }
            }

            // Erase Characters
            'X' => {
                let n = if p0 == 0 { 1 } else { p0 };
                if self.cursor_y < self.rows {
                    let end = (self.cursor_x + n).min(self.cols);
                    for c in self.cursor_x..end {
                        self.cells[self.cursor_y][c] = Cell::default();
                    }
                    self.dirty = true;
                }
            }

            // Delete Characters
            'P' => {
                let n = if p0 == 0 { 1 } else { p0 };
                if self.cursor_y < self.rows && self.cursor_x < self.cols {
                    let count = n.min(self.cols - self.cursor_x);
                    let row = &mut self.cells[self.cursor_y];
                    for i in self.cursor_x..(self.cols - count) {
                        row[i] = row[i + count];
                    }
                    for i in (self.cols - count)..self.cols {
                        row[i] = Cell::default();
                    }
                    self.dirty = true;
                }
            }

            // Insert Characters
            '@' => {
                let n = if p0 == 0 { 1 } else { p0 };
                if self.cursor_y < self.rows && self.cursor_x < self.cols {
                    let count = n.min(self.cols - self.cursor_x);
                    let row = &mut self.cells[self.cursor_y];
                    for i in (self.cursor_x..self.cols).rev() {
                        if i >= self.cursor_x + count {
                            row[i] = row[i - count];
                        } else {
                            row[i] = Cell::default();
                        }
                    }
                    self.dirty = true;
                }
            }

            // Set Top and Bottom Margins (Scroll Region) - DECSTBM
            'r' if intermediates.is_empty() => {
                let top = if p0 == 0 { 1 } else { p0 };
                let bottom = if p1 == 0 { self.rows } else { p1 };
                let new_top = if top < bottom && bottom <= self.rows {
                    (top - 1).min(self.rows.saturating_sub(1))
                } else {
                    0
                };
                let new_bottom = if top < bottom && bottom <= self.rows {
                    (bottom - 1).min(self.rows.saturating_sub(1))
                } else {
                    self.rows.saturating_sub(1)
                };
                if new_top != self.scroll_region_top || new_bottom != self.scroll_region_bottom {
                    self.scroll_region_top = new_top;
                    self.scroll_region_bottom = new_bottom;
                    self.cursor_x = 0;
                    self.cursor_y = 0;
                    self.dirty = true;
                }
            }

            // Scroll Up (SU)
            'S' => {
                let n = (if p0 == 0 { 1 } else { p0 }).min(self.rows);
                for _ in 0..n {
                    self.scroll_region_up();
                }
                self.dirty = true;
            }

            // Scroll Down (SD)
            'T' => {
                let n = (if p0 == 0 { 1 } else { p0 }).min(self.rows);
                for _ in 0..n {
                    self.scroll_region_down();
                }
                self.dirty = true;
            }

            // Insert Lines
            'L' => {
                if self.cursor_y >= self.scroll_region_top && self.cursor_y <= self.scroll_region_bottom {
                    let n = (if p0 == 0 { 1 } else { p0 })
                        .min(self.scroll_region_bottom.saturating_sub(self.cursor_y) + 1);
                    for _ in 0..n {
                        self.cells.insert(self.cursor_y, vec![Cell::default(); self.cols]);
                        self.cells.remove(self.scroll_region_bottom + 1);
                    }
                    self.dirty = true;
                }
            }

            // Delete Lines
            'M' => {
                if self.cursor_y >= self.scroll_region_top && self.cursor_y <= self.scroll_region_bottom {
                    let n = (if p0 == 0 { 1 } else { p0 })
                        .min(self.scroll_region_bottom.saturating_sub(self.cursor_y) + 1);
                    for _ in 0..n {
                        self.cells.remove(self.cursor_y);
                        self.cells.insert(self.scroll_region_bottom, vec![Cell::default(); self.cols]);
                    }
                    self.dirty = true;
                }
            }

            // Cursor Navigation
            'A' => {
                let n = if p0 == 0 { 1 } else { p0 };
                self.cursor_y = self.cursor_y.saturating_sub(n);
                self.dirty = true;
            }
            'B' => {
                let n = if p0 == 0 { 1 } else { p0 };
                self.cursor_y = (self.cursor_y + n).min(self.rows.saturating_sub(1));
                self.dirty = true;
            }
            'C' => {
                let n = if p0 == 0 { 1 } else { p0 };
                self.cursor_x = (self.cursor_x + n).min(self.cols.saturating_sub(1));
                self.dirty = true;
            }
            'D' => {
                let n = if p0 == 0 { 1 } else { p0 };
                self.cursor_x = self.cursor_x.saturating_sub(n);
                self.dirty = true;
            }

            // Save / Restore Cursor
            's' => {
                self.saved_cursor_x = self.cursor_x;
                self.saved_cursor_y = self.cursor_y;
            }
            'u' => {
                self.cursor_x = self.saved_cursor_x.min(self.cols.saturating_sub(1));
                self.cursor_y = self.saved_cursor_y.min(self.rows.saturating_sub(1));
                self.dirty = true;
            }

            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_print_and_backspace() {
        let mut grid = TerminalGrid::new(20, 5);
        grid.advance_bytes(b"hello");
        assert_eq!(grid.cursor_x, 5);
        assert_eq!(grid.cells[0][0].c, 'h');
        assert_eq!(grid.cells[0][4].c, 'o');

        // Backspace \x08 moves cursor back
        grid.advance_bytes(b"\x08");
        assert_eq!(grid.cursor_x, 4);

        // Erase to end \x1b[K
        grid.advance_bytes(b"\x1b[K");
        assert_eq!(grid.cells[0][4].c, ' ');
    }

    #[test]
    fn test_typing_spans_visibility() {
        let mut grid = TerminalGrid::new(40, 5);
        // Simulate prompt: PS C:\> 
        grid.advance_bytes(b"PS C:\\> ");
        assert_eq!(grid.cursor_x, 8);

        // Simulate typing: git status
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
        // Set red foreground: \x1b[31m
        grid.advance_bytes(b"\x1b[31mhi");
        assert_eq!(grid.cells[0][0].c, 'h');
        assert_eq!(grid.cells[0][0].style.fg, Some(ansi_16_color(1)));

        // Set TrueColor background 138, 173, 244 (Oh My Posh blue)
        grid.advance_bytes(b"\x1b[48;2;138;173;244m\x1b[38;2;255;255;255m!");
        assert_eq!(grid.cells[0][2].c, '!');
        assert_eq!(grid.cells[0][2].style.bg, Some(ColorRgb { r: 138, g: 173, b: 244 }));
        assert_eq!(grid.cells[0][2].style.fg, Some(ColorRgb { r: 255, g: 255, b: 255 }));

        // Reset \x1b[m
        grid.advance_bytes(b"\x1b[m?");
        assert_eq!(grid.cells[0][3].c, '?');
        assert_eq!(grid.cells[0][3].style.fg, None);
        assert_eq!(grid.cells[0][3].style.bg, None);
    }

    #[test]
    fn test_scrollback() {
        let mut grid = TerminalGrid::new(10, 5);
        grid.advance_bytes(b"line 1\r\nline 2\r\nline 3\r\nline 4\r\nline 5\r\nline 6\r\nline 7");
        assert_eq!(grid.scrollback.len(), 2); // line 1 and line 2 pushed to scrollback
        assert_eq!(grid.scrollback[0][0].c, 'l');
        assert_eq!(grid.scrollback[0][5].c, '1');

        // Test view scrolling
        grid.scroll_up_view(1);
        assert_eq!(grid.scroll_offset, 1);
        let top_row = grid.visible_row(0);
        assert_eq!(top_row[5].c, '2'); // line 2 is now top row of view

        grid.scroll_to_bottom();
        assert_eq!(grid.scroll_offset, 0);

        // Test cannot scroll up when no history
        let mut fresh = TerminalGrid::new(10, 5);
        fresh.scroll_up_view(5);
        assert_eq!(fresh.scroll_offset, 0);
    }

    #[test]
    fn test_selection() {
        let mut grid = TerminalGrid::new(20, 5);
        grid.advance_bytes(b"hello world\r\nsecond line");

        // Single line selection: "world"
        grid.selection = Some(Selection {
            anchor: SelectionPoint { col: 6, abs_row: 0 },
            active: SelectionPoint { col: 10, abs_row: 0 },
        });
        assert!(grid.has_selection());
        assert_eq!(grid.get_selected_text(), Some("world".to_string()));

        // Reverse selection (drag from right to left): should produce same text
        grid.selection = Some(Selection {
            anchor: SelectionPoint { col: 10, abs_row: 0 },
            active: SelectionPoint { col: 6, abs_row: 0 },
        });
        assert_eq!(grid.get_selected_text(), Some("world".to_string()));

        // Multi-line selection
        grid.selection = Some(Selection {
            anchor: SelectionPoint { col: 6, abs_row: 0 },
            active: SelectionPoint { col: 5, abs_row: 1 },
        });
        let selected = grid.get_selected_text().unwrap();
        assert_eq!(selected, "world\r\nsecond");

        // Check highlight renders
        let (_, bg_rects) = grid.render_spans_and_backgrounds(8.0, 16.0, 16.0, 16.0);
        assert!(!bg_rects.is_empty());
        assert_eq!(bg_rects[0].color, grid.selection_color);

        // Clear selection
        grid.clear_selection();
        assert!(!grid.has_selection());
        assert_eq!(grid.get_selected_text(), None);

        // Test that empty rows and trailing blank space do not get selected or highlighted
        let mut empty_grid = TerminalGrid::new(20, 5);
        empty_grid.advance_bytes(b"prompt>");
        assert_eq!(empty_grid.last_content_abs_row(), Some(0));
        assert_eq!(empty_grid.abs_row_content_len(0), 7);
        assert_eq!(empty_grid.abs_row_content_len(1), 0);

        // Select from prompt across all 5 rows and all 20 columns
        empty_grid.selection = Some(Selection {
            anchor: SelectionPoint { col: 0, abs_row: 0 },
            active: SelectionPoint { col: 19, abs_row: 4 },
        });

        // get_selected_text should only return "prompt>" and NO trailing newlines / empty lines
        assert_eq!(empty_grid.get_selected_text(), Some("prompt>".to_string()));

        // Check rendered bg_rects: should only contain 7 rects for "prompt>", none for rows 1..4
        let cell_w = 8.0;
        let cell_h = 16.0;
        let pad_x = 10.0;
        let pad_y = 10.0;
        let (_, bg_rects) = empty_grid.render_spans_and_backgrounds(cell_w, cell_h, pad_x, pad_y);
        assert_eq!(bg_rects.len(), 7);
        for rect in &bg_rects {
            // All selection rects must be on row 0 (y == pad_y)
            assert_eq!(rect.y, pad_y);
            assert_eq!(rect.color, empty_grid.selection_color);
        }
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
        // CSI 5 q = Blinking Beam
        grid.advance_bytes(b"\x1b[5 q");
        assert_eq!(grid.cursor_config.shape, CursorShape::Beam);
        assert!(grid.cursor_config.blinking);

        // CSI 2 q = Steady Block
        grid.advance_bytes(b"\x1b[2 q");
        assert_eq!(grid.cursor_config.shape, CursorShape::Block);
        assert!(!grid.cursor_config.blinking);

        // CSI 3 q = Blinking Underline
        grid.advance_bytes(b"\x1b[3 q");
        assert_eq!(grid.cursor_config.shape, CursorShape::Underline);
        assert!(grid.cursor_config.blinking);
    }

    #[test]
    fn test_decorations_underline_strikethrough_undercurl() {
        let mut grid = TerminalGrid::new(20, 5);
        // Strikethrough \x1b[9m
        grid.advance_bytes(b"\x1b[9mtest\x1b[29m");
        assert!(grid.cells[0][0].style.strikethrough);
        assert!(!grid.cells[0][4].style.strikethrough);

        // Underline \x1b[4m and Underline Color \x1b[58;2;255;0;0m
        grid.advance_bytes(b"\x1b[4m\x1b[58;2;255;0;0merror");
        assert!(grid.cells[0][4].style.underline);
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

        // Enter alternate screen: \x1b[?1049h
        grid.advance_bytes(b"\x1b[?1049h");
        assert!(grid.is_alt_screen);
        assert_eq!(grid.cells[0][0].c, ' '); // alternate screen is fresh and clean

        grid.advance_bytes(b"nvim");
        assert_eq!(grid.cells[0][0].c, 'n');

        // Exit alternate screen: \x1b[?1049l
        grid.advance_bytes(b"\x1b[?1049l");
        assert!(!grid.is_alt_screen);
        assert_eq!(grid.cells[0][0].c, 'p'); // restored primary text!
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
        // The procedural shapes replace glyphs with space in text spans so font doesn't draw missing boxes
        assert_eq!(spans[0].0, "  ");
        // And bg_rects contains the procedural vector quads
        assert!(!bg_rects.is_empty());
    }

    #[test]
    fn test_scroll_region_and_bounded_scrolling() {
        let mut grid = TerminalGrid::new(10, 5);
        // Header on row 0
        grid.advance_bytes(b"HEADER\r\n");
        // Row 1, 2, 3
        grid.advance_bytes(b"row1\r\nrow2\r\nrow3\r\n");
        // Status on row 4
        grid.advance_bytes(b"STATUS");

        assert_eq!(grid.cells[0][0].c, 'H');
        assert_eq!(grid.cells[4][0].c, 'S');

        // Set DECSTBM scroll region: rows 2 to 4 (1-based: Pt=2, Pb=4) -> 0-based: 1..=3
        grid.advance_bytes(b"\x1b[2;4r");
        assert_eq!(grid.scroll_region_top, 1);
        assert_eq!(grid.scroll_region_bottom, 3);
        assert_eq!(grid.cursor_x, 0);
        assert_eq!(grid.cursor_y, 0);

        // Move cursor to bottom of scroll region (row 3) and newline
        grid.advance_bytes(b"\x1b[4;1H"); // 1-based row 4 = 0-based row 3
        assert_eq!(grid.cursor_y, 3);
        grid.advance_bytes(b"new4\r\n");

        // Row 0 (HEADER) and Row 4 (STATUS) MUST remain completely unchanged!
        assert_eq!(grid.cells[0][0].c, 'H');
        assert_eq!(grid.cells[4][0].c, 'S');

        // Scrollback MUST be empty because scrolling was bounded in scroll region
        assert_eq!(grid.scrollback.len(), 0);
    }

    #[test]
    fn test_sgr_mouse_and_bracketed_paste_modes() {
        let mut grid = TerminalGrid::new(20, 5);

        // Bracketed paste: CSI ? 2004 h / l
        grid.advance_bytes(b"\x1b[?2004h");
        assert!(grid.bracketed_paste);
        grid.advance_bytes(b"\x1b[?2004l");
        assert!(!grid.bracketed_paste);

        // Mouse reporting: CSI ? 1000 h / l
        grid.advance_bytes(b"\x1b[?1000h");
        assert!(grid.mouse_reporting);
        grid.advance_bytes(b"\x1b[?1000l");
        assert!(!grid.mouse_reporting);

        // SGR mouse: CSI ? 1006 h / l
        grid.advance_bytes(b"\x1b[?1006h");
        assert!(grid.sgr_mouse);
        grid.advance_bytes(b"\x1b[?1006l");
        assert!(!grid.sgr_mouse);
    }

    #[test]
    fn test_word_and_line_selection() {
        let mut grid = TerminalGrid::new(30, 5);
        grid.advance_bytes(b"git checkout -b feature-foo");

        // Double click on "checkout" (col 6)
        grid.select_word_at(6, 0);
        assert_eq!(grid.get_selected_text(), Some("checkout".to_string()));

        // Double click on "feature-foo" (col 18)
        grid.select_word_at(18, 0);
        assert_eq!(grid.get_selected_text(), Some("feature-foo".to_string()));

        // Triple click line selection
        grid.select_line_at(0);
        assert_eq!(grid.get_selected_text(), Some("git checkout -b feature-foo".to_string()));
    }

    #[test]
    fn test_procedural_rounded_corners_and_blocks() {
        let mut grid = TerminalGrid::new(20, 5);
        grid.advance_bytes("╭╮╯╰█═║░".as_bytes());
        let (spans, bg_rects) = grid.render_spans_and_backgrounds(10.0, 20.0, 0.0, 0.0);
        // All replaced by spaces in glyph pass because they are procedurally rendered in bg pass
        assert_eq!(spans[0].0, "        ");
        assert!(!bg_rects.is_empty());
    }

    #[test]
    fn test_osc_8_hyperlinks() {
        let mut grid = TerminalGrid::new(30, 5);
        grid.advance_bytes(b"\x1b]8;;https://github.com\x1b\\GitHub Repo\x1b]8;;\x1b\\");

        // Cells 0..11 should point to hyperlink
        assert!(grid.cells[0][0].hyperlink_idx > 0);
        assert_eq!(
            grid.get_hyperlink_at(3, 0),
            Some("https://github.com".to_string())
        );

        // Test fallback URL detection on plaintext
        let mut plain_grid = TerminalGrid::new(40, 5);
        plain_grid.advance_bytes(b"visit https://example.com for info");
        assert_eq!(
            plain_grid.get_hyperlink_at(10, 0),
            Some("https://example.com".to_string())
        );
    }
}
