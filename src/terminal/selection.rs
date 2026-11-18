//! Text selection state, word/line double/triple-click expansion, and OSC 8 / plaintext hyperlink lookup.

use super::cell::Cell;
use super::TerminalGrid;

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

impl TerminalGrid {
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
                active: SelectionPoint {
                    col: content_len.saturating_sub(1),
                    abs_row,
                },
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
        for (start_idx, _) in text
            .match_indices("http://")
            .chain(text.match_indices("https://"))
        {
            let end_idx = text[start_idx..]
                .find(|c: char| {
                    c.is_whitespace() || c == '"' || c == '\'' || c == ')' || c == '>'
                })
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
                    let mut line = String::with_capacity(col_end - col_start + 1);
                    for c in col_start..=col_end {
                        if c < row.len() && row[c].c != '\0' {
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
