//! UTF-8 safe line-oriented `TextBuffer` with Undo (`Ctrl+Z`), Search (`Ctrl+F`), and File I/O.

use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
}

pub struct TextBuffer {
    pub path: Option<PathBuf>,
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize, // char index (not byte index)
    pub scroll_row: usize,
    pub scroll_col: usize,
    pub dirty: bool,
    undo_stack: Vec<Snapshot>,
}

impl TextBuffer {
    pub fn open(path_str: Option<&str>) -> io::Result<Self> {
        match path_str {
            Some(p) => {
                let pb = PathBuf::from(p);
                let lines = if pb.exists() {
                    let content = fs::read_to_string(&pb)?;
                    let mut v: Vec<String> = content
                        .lines()
                        .map(|l| l.trim_end_matches('\r').to_string())
                        .collect();
                    if content.ends_with('\n') {
                        v.push(String::new());
                    }
                    if v.is_empty() {
                        v.push(String::new());
                    }
                    v
                } else {
                    vec![String::new()]
                };
                Ok(Self {
                    path: Some(pb),
                    lines,
                    cursor_row: 0,
                    cursor_col: 0,
                    scroll_row: 0,
                    scroll_col: 0,
                    dirty: false,
                    undo_stack: Vec::with_capacity(64),
                })
            }
            None => Ok(Self {
                path: None,
                lines: vec![String::new()],
                cursor_row: 0,
                cursor_col: 0,
                scroll_row: 0,
                scroll_col: 0,
                dirty: false,
                undo_stack: Vec::with_capacity(64),
            }),
        }
    }

    pub fn filename_display(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "[New Buffer]".to_string())
    }

    pub fn save(&mut self) -> io::Result<usize> {
        let target = self
            .path
            .clone()
            .unwrap_or_else(|| PathBuf::from("untitled.txt"));
        let content = self.lines.join("\n");
        let bytes = content.len();
        fs::write(&target, content)?;
        self.path = Some(target);
        self.dirty = false;
        Ok(bytes)
    }

    fn push_undo(&mut self) {
        if self.undo_stack.len() >= 128 {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(Snapshot {
            lines: self.lines.clone(),
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
        });
    }

    pub fn undo(&mut self) -> bool {
        if let Some(snap) = self.undo_stack.pop() {
            self.lines = snap.lines;
            self.cursor_row = snap.cursor_row.min(self.lines.len().saturating_sub(1));
            self.cursor_col = snap.cursor_col.min(self.line_char_len(self.cursor_row));
            self.dirty = true;
            true
        } else {
            false
        }
    }

    #[inline]
    pub fn line_char_len(&self, row: usize) -> usize {
        self.lines
            .get(row)
            .map_or(0, |l| l.chars().count())
    }

    #[inline]
    fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
        s.char_indices()
            .nth(char_idx)
            .map(|(b, _)| b)
            .unwrap_or(s.len())
    }

    pub fn insert_char(&mut self, ch: char) {
        self.push_undo();
        let row = self.cursor_row;
        let col = self.cursor_col.min(self.line_char_len(row));
        let byte_idx = Self::char_to_byte_idx(&self.lines[row], col);
        self.lines[row].insert(byte_idx, ch);
        self.cursor_col = col + 1;
        self.dirty = true;
    }

    pub fn insert_tab(&mut self) {
        self.push_undo();
        let row = self.cursor_row;
        let col = self.cursor_col.min(self.line_char_len(row));
        let byte_idx = Self::char_to_byte_idx(&self.lines[row], col);
        self.lines[row].insert_str(byte_idx, "    ");
        self.cursor_col = col + 4;
        self.dirty = true;
    }

    pub fn insert_newline(&mut self) {
        self.push_undo();
        let row = self.cursor_row;
        let col = self.cursor_col.min(self.line_char_len(row));
        let byte_idx = Self::char_to_byte_idx(&self.lines[row], col);
        let tail = self.lines[row].split_off(byte_idx);
        self.lines.insert(row + 1, tail);
        self.cursor_row += 1;
        self.cursor_col = 0;
        self.dirty = true;
    }

    pub fn backspace(&mut self) {
        let row = self.cursor_row;
        let col = self.cursor_col.min(self.line_char_len(row));
        if col > 0 {
            self.push_undo();
            let start_b = Self::char_to_byte_idx(&self.lines[row], col - 1);
            let end_b = Self::char_to_byte_idx(&self.lines[row], col);
            self.lines[row].replace_range(start_b..end_b, "");
            self.cursor_col = col - 1;
            self.dirty = true;
        } else if row > 0 {
            self.push_undo();
            let current_line = self.lines.remove(row);
            let prev_len = self.line_char_len(row - 1);
            self.lines[row - 1].push_str(&current_line);
            self.cursor_row -= 1;
            self.cursor_col = prev_len;
            self.dirty = true;
        }
    }

    pub fn delete(&mut self) {
        let row = self.cursor_row;
        let col = self.cursor_col.min(self.line_char_len(row));
        let len = self.line_char_len(row);
        if col < len {
            self.push_undo();
            let start_b = Self::char_to_byte_idx(&self.lines[row], col);
            let end_b = Self::char_to_byte_idx(&self.lines[row], col + 1);
            self.lines[row].replace_range(start_b..end_b, "");
            self.dirty = true;
        } else if row + 1 < self.lines.len() {
            self.push_undo();
            let next_line = self.lines.remove(row + 1);
            self.lines[row].push_str(&next_line);
            self.dirty = true;
        }
    }

    pub fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.cursor_col.min(self.line_char_len(self.cursor_row));
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = self.cursor_col.min(self.line_char_len(self.cursor_row));
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.line_char_len(self.cursor_row);
        }
    }

    pub fn move_right(&mut self) {
        let len = self.line_char_len(self.cursor_row);
        if self.cursor_col < len {
            self.cursor_col += 1;
        } else if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
    }

    pub fn move_home(&mut self) {
        self.cursor_col = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor_col = self.line_char_len(self.cursor_row);
    }

    pub fn page_up(&mut self, view_rows: usize) {
        self.cursor_row = self.cursor_row.saturating_sub(view_rows.max(1));
        self.cursor_col = self.cursor_col.min(self.line_char_len(self.cursor_row));
    }

    pub fn page_down(&mut self, view_rows: usize) {
        self.cursor_row =
            (self.cursor_row + view_rows.max(1)).min(self.lines.len().saturating_sub(1));
        self.cursor_col = self.cursor_col.min(self.line_char_len(self.cursor_row));
    }

    pub fn adjust_viewport(&mut self, view_rows: usize, text_cols: usize) {
        if self.cursor_row < self.scroll_row {
            self.scroll_row = self.cursor_row;
        } else if self.cursor_row >= self.scroll_row + view_rows {
            self.scroll_row = self.cursor_row - view_rows + 1;
        }

        if self.cursor_col < self.scroll_col {
            self.scroll_col = self.cursor_col;
        } else if self.cursor_col >= self.scroll_col + text_cols {
            self.scroll_col = self.cursor_col - text_cols + 1;
        }
    }

    /// Searches forward from the current cursor position (wrapping around) for `query`.
    pub fn find_next(&mut self, query: &str) -> bool {
        if query.is_empty() {
            return false;
        }
        let q_lower = query.to_lowercase();
        let total = self.lines.len();
        for offset in 0..=total {
            let r = (self.cursor_row + offset) % total;
            let start_char = if offset == 0 {
                self.cursor_col + 1
            } else {
                0
            };
            let line = &self.lines[r];
            let start_b = Self::char_to_byte_idx(line, start_char);
            if start_b <= line.len() {
                let slice_lower = line[start_b..].to_lowercase();
                if let Some(found_b) = slice_lower.find(&q_lower) {
                    let prefix_chars = line[..start_b + found_b].chars().count();
                    self.cursor_row = r;
                    self.cursor_col = prefix_chars;
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_edit_undo_and_find() {
        let mut buf = TextBuffer::open(None).unwrap();
        buf.insert_char('h');
        buf.insert_char('i');
        buf.insert_newline();
        buf.insert_char('l');
        buf.insert_char('u');
        buf.insert_char('n');
        buf.insert_char('a');
        assert_eq!(buf.lines, vec!["hi", "luna"]);
        assert!(buf.dirty);

        assert!(buf.undo());
        assert_eq!(buf.lines, vec!["hi", "lun"]);

        buf.cursor_row = 0;
        buf.cursor_col = 0;
        assert!(buf.find_next("lun"));
        assert_eq!(buf.cursor_row, 1);
        assert_eq!(buf.cursor_col, 0);
    }
}
