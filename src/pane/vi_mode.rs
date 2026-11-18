//! Alacritty-style Vi Mode, Scrollback Search, and Regex Hints.

use crate::terminal::{Selection, SelectionPoint, TerminalGrid};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViModeKind {
    Normal,
    Visual,
    VisualLine,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchMatch {
    pub point: SelectionPoint,
    pub len: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HintMatch {
    pub point: SelectionPoint,
    pub len: usize,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViMode {
    pub kind: ViModeKind,
    pub cursor_col: usize,
    pub cursor_abs_row: usize,
    pub anchor: Option<SelectionPoint>,

    // Search state
    pub in_search_input: bool,
    pub search_query: String,
    pub search_matches: Vec<SearchMatch>,
    pub active_match_idx: Option<usize>,

    // Hints state
    pub hints_active: bool,
    pub hint_matches: Vec<HintMatch>,
}

impl ViMode {
    pub fn new(cursor_col: usize, cursor_abs_row: usize) -> Self {
        Self {
            kind: ViModeKind::Normal,
            cursor_col,
            cursor_abs_row,
            anchor: None,
            in_search_input: false,
            search_query: String::new(),
            search_matches: Vec::new(),
            active_match_idx: None,
            hints_active: false,
            hint_matches: Vec::new(),
        }
    }

    /// Moves cursor left by 1 column ('h').
    pub fn move_left(&mut self, grid: &mut TerminalGrid) {
        self.cursor_col = self.cursor_col.saturating_sub(1);
        self.update_selection(grid);
    }

    /// Moves cursor right by 1 column ('l').
    pub fn move_right(&mut self, grid: &mut TerminalGrid) {
        let max_col = grid.cols.saturating_sub(1);
        self.cursor_col = (self.cursor_col + 1).min(max_col);
        self.update_selection(grid);
    }

    /// Moves cursor up by 1 row into scrollback ('k').
    pub fn move_up(&mut self, grid: &mut TerminalGrid) {
        self.cursor_abs_row = self.cursor_abs_row.saturating_sub(1);
        self.ensure_cursor_visible(grid);
        self.update_selection(grid);
    }

    /// Moves cursor down by 1 row ('j').
    pub fn move_down(&mut self, grid: &mut TerminalGrid) {
        let total_rows = grid.scrollback.len() + grid.rows;
        if total_rows > 0 {
            self.cursor_abs_row = (self.cursor_abs_row + 1).min(total_rows.saturating_sub(1));
        }
        self.ensure_cursor_visible(grid);
        self.update_selection(grid);
    }

    /// Moves cursor to start of line ('0').
    pub fn move_to_line_start(&mut self, grid: &mut TerminalGrid) {
        self.cursor_col = 0;
        self.update_selection(grid);
    }

    /// Moves cursor to end of line content ('$').
    pub fn move_to_line_end(&mut self, grid: &mut TerminalGrid) {
        let content_len = grid.abs_row_content_len(self.cursor_abs_row);
        self.cursor_col = content_len.saturating_sub(1);
        self.update_selection(grid);
    }

    /// Move to next word forward ('w').
    pub fn move_word_forward(&mut self, grid: &mut TerminalGrid) {
        if let Some(row) = grid.get_row(self.cursor_abs_row) {
            let content_len = grid.abs_row_content_len(self.cursor_abs_row);
            let is_word_char = |c: char| c.is_alphanumeric() || c == '_' || c == '-';

            let mut col = self.cursor_col;
            while col < content_len && is_word_char(row[col].c) {
                col += 1;
            }
            while col < content_len && !is_word_char(row[col].c) {
                col += 1;
            }
            if col < content_len {
                self.cursor_col = col;
            } else {
                let total_rows = grid.scrollback.len() + grid.rows;
                if self.cursor_abs_row + 1 < total_rows {
                    self.cursor_abs_row += 1;
                    self.cursor_col = 0;
                    self.ensure_cursor_visible(grid);
                }
            }
            self.update_selection(grid);
        }
    }

    /// Move to previous word backward ('b').
    pub fn move_word_backward(&mut self, grid: &mut TerminalGrid) {
        if let Some(row) = grid.get_row(self.cursor_abs_row) {
            let is_word_char = |c: char| c.is_alphanumeric() || c == '_' || c == '-';
            let mut col = self.cursor_col;

            if col > 0 {
                col -= 1;
                while col > 0 && !is_word_char(row[col].c) {
                    col -= 1;
                }
                while col > 0 && is_word_char(row[col - 1].c) {
                    col -= 1;
                }
                self.cursor_col = col;
            } else if self.cursor_abs_row > 0 {
                self.cursor_abs_row -= 1;
                let prev_len = grid.abs_row_content_len(self.cursor_abs_row);
                self.cursor_col = prev_len.saturating_sub(1);
                self.ensure_cursor_visible(grid);
            }
            self.update_selection(grid);
        }
    }

    /// Jump to top of scrollback ('g').
    pub fn move_to_top(&mut self, grid: &mut TerminalGrid) {
        self.cursor_abs_row = 0;
        self.cursor_col = 0;
        self.ensure_cursor_visible(grid);
        self.update_selection(grid);
    }

    /// Jump to bottom of active screen ('G').
    pub fn move_to_bottom(&mut self, grid: &mut TerminalGrid) {
        let total_rows = grid.scrollback.len() + grid.rows;
        self.cursor_abs_row = total_rows.saturating_sub(1);
        self.cursor_col = 0;
        self.ensure_cursor_visible(grid);
        self.update_selection(grid);
    }

    /// Toggles character-wise visual mode ('v').
    pub fn toggle_visual(&mut self, grid: &mut TerminalGrid) {
        match self.kind {
            ViModeKind::Visual => {
                self.kind = ViModeKind::Normal;
                self.anchor = None;
                grid.clear_selection();
            }
            _ => {
                self.kind = ViModeKind::Visual;
                let pt = SelectionPoint {
                    col: self.cursor_col,
                    abs_row: self.cursor_abs_row,
                };
                self.anchor = Some(pt);
                grid.selection = Some(Selection {
                    anchor: pt,
                    active: pt,
                });
                grid.mark_all_rows_dirty();
            }
        }
    }

    /// Toggles line-wise visual mode ('V').
    pub fn toggle_visual_line(&mut self, grid: &mut TerminalGrid) {
        match self.kind {
            ViModeKind::VisualLine => {
                self.kind = ViModeKind::Normal;
                self.anchor = None;
                grid.clear_selection();
            }
            _ => {
                self.kind = ViModeKind::VisualLine;
                let pt = SelectionPoint {
                    col: self.cursor_col,
                    abs_row: self.cursor_abs_row,
                };
                self.anchor = Some(pt);
                self.update_selection(grid);
            }
        }
    }

    /// Synchronizes `grid.selection` to the current anchor and Vi cursor.
    pub fn update_selection(&mut self, grid: &mut TerminalGrid) {
        let Some(anchor) = self.anchor else {
            return;
        };

        match self.kind {
            ViModeKind::Normal => {}
            ViModeKind::Visual => {
                grid.selection = Some(Selection {
                    anchor,
                    active: SelectionPoint {
                        col: self.cursor_col,
                        abs_row: self.cursor_abs_row,
                    },
                });
                grid.mark_all_rows_dirty();
            }
            ViModeKind::VisualLine => {
                let start_row = anchor.abs_row.min(self.cursor_abs_row);
                let end_row = anchor.abs_row.max(self.cursor_abs_row);
                let end_content_len = grid.abs_row_content_len(end_row);
                let end_col = end_content_len.saturating_sub(1).max(grid.cols.saturating_sub(1));

                grid.selection = Some(Selection {
                    anchor: SelectionPoint { col: 0, abs_row: start_row },
                    active: SelectionPoint { col: end_col, abs_row: end_row },
                });
                grid.mark_all_rows_dirty();
            }
        }
    }

    /// Keeps the viewport scroll offset focused on the Vi cursor.
    pub fn ensure_cursor_visible(&self, grid: &mut TerminalGrid) {
        let sb_len = grid.scrollback.len();
        let total_rows = sb_len + grid.rows;
        if total_rows == 0 {
            return;
        }

        let cell_h = grid.cell_height.max(1.0);
        let base_offset = if grid.current_scroll_y > 0.0 {
            (grid.current_scroll_y / cell_h).floor() as usize
        } else {
            grid.scroll_offset
        };

        let start_abs = total_rows.saturating_sub(base_offset + grid.rows);
        let end_abs = start_abs + grid.rows;

        if self.cursor_abs_row < start_abs {
            let new_offset = total_rows.saturating_sub(self.cursor_abs_row + grid.rows);
            grid.scroll_offset = new_offset.min(sb_len);
            grid.target_scroll_y = grid.scroll_offset as f32 * cell_h;
            grid.current_scroll_y = grid.target_scroll_y;
            grid.mark_all_rows_dirty();
        } else if self.cursor_abs_row >= end_abs {
            let new_offset = total_rows.saturating_sub(self.cursor_abs_row + 1);
            grid.scroll_offset = new_offset.min(sb_len);
            grid.target_scroll_y = grid.scroll_offset as f32 * cell_h;
            grid.current_scroll_y = grid.target_scroll_y;
            grid.mark_all_rows_dirty();
        }
    }

    /// Runs case-insensitive search across all scrollback lines and active grid.
    pub fn execute_search(&mut self, grid: &mut TerminalGrid) {
        self.search_matches.clear();
        self.active_match_idx = None;

        if self.search_query.is_empty() {
            return;
        }

        let query_lower = self.search_query.to_lowercase();
        let query_len = self.search_query.chars().count();
        let total_rows = grid.scrollback.len() + grid.rows;

        for abs_row in 0..total_rows {
            if let Some(row) = grid.get_row(abs_row) {
                let content_len = grid.abs_row_content_len(abs_row);
                if content_len == 0 {
                    continue;
                }
                let mut line = String::with_capacity(content_len);
                for c in 0..content_len {
                    line.push(if c < row.len() && row[c].c != '\0' { row[c].c } else { ' ' });
                }
                let line_lower = line.to_lowercase();
                for (byte_idx, _) in line_lower.match_indices(&query_lower) {
                    let char_col = line[..byte_idx].chars().count();
                    self.search_matches.push(SearchMatch {
                        point: SelectionPoint { col: char_col, abs_row },
                        len: query_len,
                    });
                }
            }
        }

        if !self.search_matches.is_empty() {
            let match_idx = self
                .search_matches
                .iter()
                .position(|m| {
                    m.point.abs_row > self.cursor_abs_row
                        || (m.point.abs_row == self.cursor_abs_row && m.point.col >= self.cursor_col)
                })
                .unwrap_or(0);

            self.jump_to_match(match_idx, grid);
        }
    }

    pub fn jump_to_match(&mut self, idx: usize, grid: &mut TerminalGrid) {
        if idx < self.search_matches.len() {
            self.active_match_idx = Some(idx);
            let m = &self.search_matches[idx];
            self.cursor_col = m.point.col;
            self.cursor_abs_row = m.point.abs_row;
            self.ensure_cursor_visible(grid);
            self.update_selection(grid);
        }
    }

    pub fn next_match(&mut self, grid: &mut TerminalGrid) {
        if self.search_matches.is_empty() {
            return;
        }
        let next_idx = match self.active_match_idx {
            Some(i) => (i + 1) % self.search_matches.len(),
            None => 0,
        };
        self.jump_to_match(next_idx, grid);
    }

    pub fn prev_match(&mut self, grid: &mut TerminalGrid) {
        if self.search_matches.is_empty() {
            return;
        }
        let len = self.search_matches.len();
        let prev_idx = match self.active_match_idx {
            Some(0) | None => len.saturating_sub(1),
            Some(i) => i - 1,
        };
        self.jump_to_match(prev_idx, grid);
    }

    /// Scans for Regex Hints (URLs, file paths, git hashes, and IPs) across the grid.
    pub fn scan_hints(&mut self, grid: &TerminalGrid) {
        self.hint_matches.clear();
        let total_rows = grid.scrollback.len() + grid.rows;

        for abs_row in 0..total_rows {
            if let Some(row) = grid.get_row(abs_row) {
                let content_len = grid.abs_row_content_len(abs_row);
                if content_len < 3 {
                    continue;
                }
                let mut line = String::with_capacity(content_len);
                for c in 0..content_len {
                    line.push(if c < row.len() && row[c].c != '\0' { row[c].c } else { ' ' });
                }

                scan_line_for_hints(&line, abs_row, &mut self.hint_matches);
            }
        }
        self.hints_active = !self.hint_matches.is_empty();
    }
}

fn scan_line_for_hints(line: &str, abs_row: usize, matches: &mut Vec<HintMatch>) {
    // 1. URLs
    for prefix in &["https://", "http://", "git@"] {
        for (byte_idx, _) in line.match_indices(prefix) {
            let after = &line[byte_idx..];
            let end_len = after
                .find(|c: char| {
                    c.is_whitespace() || c == '"' || c == '\'' || c == ')' || c == '>' || c == '`'
                })
                .unwrap_or(after.len());
            if end_len > prefix.len() {
                let char_col = line[..byte_idx].chars().count();
                let char_len = line[byte_idx..byte_idx + end_len].chars().count();
                matches.push(HintMatch {
                    point: SelectionPoint { col: char_col, abs_row },
                    len: char_len,
                    text: line[byte_idx..byte_idx + end_len].to_string(),
                });
            }
        }
    }

    // 2. Tokenized words for file paths, hashes, and IP addresses
    let mut words = Vec::new();
    let mut start = None;
    for (i, c) in line.char_indices() {
        if !c.is_whitespace() && c != '"' && c != '\'' && c != '(' && c != ')' && c != '[' && c != ']' && c != '<' && c != '>' && c != '`' {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start.take() {
            words.push((s, &line[s..i]));
        }
    }
    if let Some(s) = start {
        words.push((s, &line[s..]));
    }

    for (byte_start, word) in words {
        if word.starts_with("http://") || word.starts_with("https://") || word.starts_with("git@") {
            continue;
        }

        let is_path = (word.contains('/') || word.contains('\\') || word.contains('.'))
            && (word.ends_with(".rs")
                || word.ends_with(".toml")
                || word.ends_with(".lua")
                || word.ends_with(".json")
                || word.ends_with(".wgsl")
                || word.ends_with(".md")
                || word.ends_with(".txt")
                || word.ends_with(".ts")
                || word.ends_with(".js")
                || word.ends_with(".lock")
                || word.ends_with(".ps1")
                || word.ends_with(".yml")
                || word.ends_with(".yaml")
                || word.starts_with("./")
                || word.starts_with("../")
                || (word.len() >= 3
                    && word.chars().nth(1) == Some(':')
                    && (word.chars().nth(2) == Some('\\') || word.chars().nth(2) == Some('/'))));

        let is_hex_hash =
            word.len() >= 7 && word.len() <= 40 && word.chars().all(|c| c.is_ascii_hexdigit());

        let is_ip = {
            let parts: Vec<&str> = word.split(':').next().unwrap_or("").split('.').collect();
            parts.len() == 4 && parts.iter().all(|p| p.parse::<u8>().is_ok())
        };

        if is_path || is_hex_hash || is_ip {
            let char_col = line[..byte_start].chars().count();
            let char_len = word.chars().count();
            matches.push(HintMatch {
                point: SelectionPoint { col: char_col, abs_row },
                len: char_len,
                text: word.to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vi_mode_movement_and_bounds() {
        let mut grid = TerminalGrid::new(20, 10);
        let mut vi = ViMode::new(5, 3);

        vi.move_left(&mut grid);
        assert_eq!(vi.cursor_col, 4);

        vi.move_right(&mut grid);
        assert_eq!(vi.cursor_col, 5);

        vi.move_up(&mut grid);
        assert_eq!(vi.cursor_abs_row, 2);

        vi.move_down(&mut grid);
        assert_eq!(vi.cursor_abs_row, 3);

        vi.move_to_line_start(&mut grid);
        assert_eq!(vi.cursor_col, 0);
    }

    #[test]
    fn test_vi_visual_character_selection() {
        let mut grid = TerminalGrid::new(20, 10);
        let mut vi = ViMode::new(2, 4);

        vi.toggle_visual(&mut grid);
        assert_eq!(vi.kind, ViModeKind::Visual);
        assert!(grid.selection.is_some());

        vi.move_right(&mut grid);
        vi.move_right(&mut grid);

        let sel = grid.selection.as_ref().unwrap();
        assert_eq!(sel.anchor, SelectionPoint { col: 2, abs_row: 4 });
        assert_eq!(sel.active, SelectionPoint { col: 4, abs_row: 4 });

        vi.toggle_visual(&mut grid);
        assert_eq!(vi.kind, ViModeKind::Normal);
        assert!(grid.selection.is_none());
    }

    #[test]
    fn test_vi_visual_line_selection() {
        let mut grid = TerminalGrid::new(20, 10);
        grid.advance_bytes(b"line 0 content\r\nline 1 content");
        let mut vi = ViMode::new(3, 0);

        vi.toggle_visual_line(&mut grid);
        assert_eq!(vi.kind, ViModeKind::VisualLine);
        assert!(grid.selection.is_some());

        vi.move_down(&mut grid);
        let sel = grid.selection.as_ref().unwrap();
        assert_eq!(sel.anchor.abs_row, 0);
        assert_eq!(sel.anchor.col, 0);
        assert_eq!(sel.active.abs_row, 1);
        assert!(sel.active.col >= 13);
    }

    #[test]
    fn test_vi_scrollback_search() {
        let mut grid = TerminalGrid::new(40, 10);
        grid.advance_bytes(b"hello world\r\nfoo bar\r\nhello again");
        let mut vi = ViMode::new(0, 0);

        vi.search_query = "hello".to_string();
        vi.execute_search(&mut grid);

        assert_eq!(vi.search_matches.len(), 2);
        assert_eq!(vi.search_matches[0].point.abs_row, 0);
        assert_eq!(vi.search_matches[0].point.col, 0);
        assert_eq!(vi.search_matches[1].point.abs_row, 2);
        assert_eq!(vi.search_matches[1].point.col, 0);

        vi.next_match(&mut grid);
        assert_eq!(vi.cursor_abs_row, 2);
        vi.prev_match(&mut grid);
        assert_eq!(vi.cursor_abs_row, 0);
    }

    #[test]
    fn test_vi_regex_hints_scanner() {
        let mut grid = TerminalGrid::new(80, 10);
        grid.advance_bytes(b"Check https://github.com/foo and src/main.rs with commit a1b2c3d4e5f6");
        let mut vi = ViMode::new(0, 0);

        vi.scan_hints(&grid);
        assert!(vi.hints_active);
        let texts: Vec<String> = vi.hint_matches.iter().map(|m| m.text.clone()).collect();
        assert!(texts.iter().any(|t| t.contains("https://github.com/foo")));
        assert!(texts.iter().any(|t| t == "src/main.rs"));
        assert!(texts.iter().any(|t| t == "a1b2c3d4e5f6"));
    }
}
