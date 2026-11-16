use unicode_width::UnicodeWidthChar;
use vte::{Params, Parser, Perform};

pub struct TerminalGrid {
    pub cols: usize,
    pub rows: usize,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub saved_cursor_x: usize,
    pub saved_cursor_y: usize,
    pub cursor_visible: bool,
    pub win32_input_mode: bool,
    pub app_cursor_keys: bool,
    pub cells: Vec<Vec<char>>,
    pub parser: Parser,
    pub dirty: bool,
    pub responses: Vec<Vec<u8>>,
}

impl TerminalGrid {
    pub fn new(cols: usize, rows: usize) -> Self {
        let cols = cols.max(10);
        let rows = rows.max(5);
        let cells = vec![vec![' '; cols]; rows];

        Self {
            cols,
            rows,
            cursor_x: 0,
            cursor_y: 0,
            saved_cursor_x: 0,
            saved_cursor_y: 0,
            cursor_visible: true,
            win32_input_mode: true, // ConPTY defaults to Win32 Input Mode on Windows 10/11
            app_cursor_keys: false,
            cells,
            parser: Parser::new(),
            dirty: true,
            responses: Vec::new(),
        }
    }

    pub fn resize(&mut self, new_cols: usize, new_rows: usize) {
        let new_cols = new_cols.max(10);
        let new_rows = new_rows.max(5);

        let mut new_cells = vec![vec![' '; new_cols]; new_rows];
        for r in 0..self.rows.min(new_rows) {
            for c in 0..self.cols.min(new_cols) {
                new_cells[r][c] = self.cells[r][c];
            }
        }

        self.cols = new_cols;
        self.rows = new_rows;
        self.cursor_x = self.cursor_x.min(new_cols - 1);
        self.cursor_y = self.cursor_y.min(new_rows - 1);
        self.cells = new_cells;
        self.dirty = true;
    }

    pub fn advance_bytes(&mut self, bytes: &[u8]) {
        let mut parser = std::mem::replace(&mut self.parser, Parser::new());
        parser.advance(self, bytes);
        self.parser = parser;
    }

    fn scroll_up(&mut self) {
        if self.rows > 0 {
            self.cells.remove(0);
            self.cells.push(vec![' '; self.cols]);
        }
    }

    pub fn render_to_string(&self) -> String {
        let mut out = String::with_capacity(self.cols * self.rows + self.rows);
        for (r, row) in self.cells.iter().enumerate() {
            if r > 0 {
                out.push('\n');
            }

            let mut end = row.iter().rposition(|&c| c != ' ').map(|i| i + 1).unwrap_or(0);
            if self.cursor_visible && r == self.cursor_y && self.cursor_x >= end {
                end = self.cursor_x + 1;
            }

            for c in 0..end {
                let is_cursor = self.cursor_visible && r == self.cursor_y && c == self.cursor_x;
                if is_cursor {
                    if c < row.len() && row[c] != ' ' {
                        out.push(row[c]);
                        out.push('\u{0332}'); // Combining low line: clearly underlines character under cursor
                    } else {
                        out.push('▏'); // Sleek vertical cursor bar (never an ugly solid block)
                    }
                } else if c < row.len() {
                    out.push(row[c]);
                } else {
                    out.push(' ');
                }
            }
        }
        out
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
            self.cursor_y += 1;
        }
        while self.cursor_y >= self.rows {
            self.scroll_up();
            self.cursor_y = self.rows.saturating_sub(1);
        }

        if self.cursor_y < self.rows && self.cursor_x < self.cols {
            self.cells[self.cursor_y][self.cursor_x] = c;
            for extra in 1..w {
                if self.cursor_x + extra < self.cols {
                    self.cells[self.cursor_y][self.cursor_x + extra] = ' ';
                }
            }
            self.cursor_x += w;
            self.dirty = true;
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            // Line Feed (\n)
            0x0A => {
                self.cursor_y += 1;
                while self.cursor_y >= self.rows {
                    self.scroll_up();
                    self.cursor_y = self.rows.saturating_sub(1);
                }
                self.dirty = true;
            }
            // Carriage Return (\r)
            0x0D => {
                self.cursor_x = 0;
                self.dirty = true;
            }
            // Backspace (\x08)
            0x08 => {
                if self.cursor_x > 0 {
                    self.cursor_x -= 1;
                    self.dirty = true;
                }
            }
            // Tab (\t)
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
            // DECSC: Save Cursor
            b'7' => {
                self.saved_cursor_x = self.cursor_x;
                self.saved_cursor_y = self.cursor_y;
            }
            // DECRC: Restore Cursor
            b'8' => {
                self.cursor_x = self.saved_cursor_x.min(self.cols.saturating_sub(1));
                self.cursor_y = self.saved_cursor_y.min(self.rows.saturating_sub(1));
                self.dirty = true;
            }
            // RI: Reverse Index
            b'M' => {
                if self.cursor_y == 0 {
                    self.cells.insert(0, vec![' '; self.cols]);
                    self.cells.pop();
                } else {
                    self.cursor_y -= 1;
                }
                self.dirty = true;
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        let is_private = intermediates.contains(&b'?');
        let is_secondary = intermediates.contains(&b'>');
        let mut it = params.iter();
        let p0 = it.next().and_then(|p| p.first()).copied().unwrap_or(0) as usize;
        let p1 = it.next().and_then(|p| p.first()).copied().unwrap_or(0) as usize;

        // DSR: Device Status Report (CSI 6 n or CSI ? 6 n) - Immediate answer avoids timeouts!
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

        // Window Size query (CSI 18 t -> report rows;cols)
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
                    9001 => self.win32_input_mode = true,
                    1 => self.app_cursor_keys = true,
                    _ => {}
                },
                'l' => match p0 {
                    25 => {
                        self.cursor_visible = false;
                        self.dirty = true;
                    }
                    9001 => self.win32_input_mode = false,
                    1 => self.app_cursor_keys = false,
                    _ => {}
                },
                _ => {}
            }
            return;
        }

        match action {
            // DSR: Device Status Report (CSI 6 n) - CRITICAL for ConPTY / PSReadLine!
            'n' if p0 == 6 => {
                let resp = format!("\x1b[{};{}R", self.cursor_y + 1, self.cursor_x + 1);
                self.responses.push(resp.into_bytes());
            }
            // DSR: Status Report (CSI 5 n) - Reply OK
            'n' if p0 == 5 => {
                self.responses.push(b"\x1b[0n".to_vec());
            }
            // DA: Primary Device Attributes (CSI c)
            'c' => {
                self.responses.push(b"\x1b[?1;2c".to_vec());
            }

            // Cursor Position (CSI row ; col H / f)
            'H' | 'f' => {
                let row = if p0 == 0 { 1 } else { p0 };
                let col = if p1 == 0 { 1 } else { p1 };
                self.cursor_y = (row - 1).min(self.rows.saturating_sub(1));
                self.cursor_x = (col - 1).min(self.cols.saturating_sub(1));
                self.dirty = true;
            }

            // Cursor Horizontal Absolute (CSI col G / `)
            'G' | '`' => {
                let col = if p0 == 0 { 1 } else { p0 };
                self.cursor_x = (col - 1).min(self.cols.saturating_sub(1));
                self.dirty = true;
            }

            // Line Position Absolute (CSI row d)
            'd' => {
                let row = if p0 == 0 { 1 } else { p0 };
                self.cursor_y = (row - 1).min(self.rows.saturating_sub(1));
                self.dirty = true;
            }

            // Cursor Next Line (CSI n E)
            'E' => {
                let n = if p0 == 0 { 1 } else { p0 };
                self.cursor_x = 0;
                self.cursor_y = (self.cursor_y + n).min(self.rows.saturating_sub(1));
                self.dirty = true;
            }

            // Cursor Previous Line (CSI n F)
            'F' => {
                let n = if p0 == 0 { 1 } else { p0 };
                self.cursor_x = 0;
                self.cursor_y = self.cursor_y.saturating_sub(n);
                self.dirty = true;
            }

            // Erase in Display (CSI n J)
            'J' => {
                match p0 {
                    // Clear from cursor to end
                    0 => {
                        if self.cursor_y < self.rows {
                            for c in self.cursor_x..self.cols {
                                self.cells[self.cursor_y][c] = ' ';
                            }
                            for r in (self.cursor_y + 1)..self.rows {
                                for c in 0..self.cols {
                                    self.cells[r][c] = ' ';
                                }
                            }
                        }
                    }
                    // Clear from start to cursor
                    1 => {
                        for r in 0..self.cursor_y {
                            for c in 0..self.cols {
                                self.cells[r][c] = ' ';
                            }
                        }
                        if self.cursor_y < self.rows {
                            for c in 0..=self.cursor_x.min(self.cols - 1) {
                                self.cells[self.cursor_y][c] = ' ';
                            }
                        }
                    }
                    // Clear all
                    2 | 3 => {
                        for r in 0..self.rows {
                            for c in 0..self.cols {
                                self.cells[r][c] = ' ';
                            }
                        }
                        self.cursor_x = 0;
                        self.cursor_y = 0;
                    }
                    _ => {}
                }
                self.dirty = true;
            }

            // Erase in Line (CSI n K)
            'K' => {
                if self.cursor_y < self.rows {
                    match p0 {
                        // Cursor to end
                        0 => {
                            for c in self.cursor_x..self.cols {
                                self.cells[self.cursor_y][c] = ' ';
                            }
                        }
                        // Start to cursor
                        1 => {
                            for c in 0..=self.cursor_x.min(self.cols - 1) {
                                self.cells[self.cursor_y][c] = ' ';
                            }
                        }
                        // Entire line
                        2 => {
                            for c in 0..self.cols {
                                self.cells[self.cursor_y][c] = ' ';
                            }
                        }
                        _ => {}
                    }
                    self.dirty = true;
                }
            }

            // Erase Characters (CSI n X)
            'X' => {
                let n = if p0 == 0 { 1 } else { p0 };
                if self.cursor_y < self.rows {
                    let end = (self.cursor_x + n).min(self.cols);
                    for c in self.cursor_x..end {
                        self.cells[self.cursor_y][c] = ' ';
                    }
                    self.dirty = true;
                }
            }

            // Delete Characters (CSI n P)
            'P' => {
                let n = if p0 == 0 { 1 } else { p0 };
                if self.cursor_y < self.rows && self.cursor_x < self.cols {
                    let count = n.min(self.cols - self.cursor_x);
                    let row = &mut self.cells[self.cursor_y];
                    for i in self.cursor_x..(self.cols - count) {
                        row[i] = row[i + count];
                    }
                    for i in (self.cols - count)..self.cols {
                        row[i] = ' ';
                    }
                    self.dirty = true;
                }
            }

            // Insert Characters (CSI n @)
            '@' => {
                let n = if p0 == 0 { 1 } else { p0 };
                if self.cursor_y < self.rows && self.cursor_x < self.cols {
                    let count = n.min(self.cols - self.cursor_x);
                    let row = &mut self.cells[self.cursor_y];
                    for i in (self.cursor_x..self.cols).rev() {
                        if i >= self.cursor_x + count {
                            row[i] = row[i - count];
                        } else {
                            row[i] = ' ';
                        }
                    }
                    self.dirty = true;
                }
            }

            // Insert Lines (CSI n L)
            'L' => {
                let n = (if p0 == 0 { 1 } else { p0 }).min(self.rows.saturating_sub(self.cursor_y));
                for _ in 0..n {
                    if self.cursor_y < self.rows {
                        self.cells.insert(self.cursor_y, vec![' '; self.cols]);
                        self.cells.pop();
                    }
                }
                self.dirty = true;
            }

            // Delete Lines (CSI n M)
            'M' => {
                let n = (if p0 == 0 { 1 } else { p0 }).min(self.rows.saturating_sub(self.cursor_y));
                for _ in 0..n {
                    if self.cursor_y < self.rows {
                        self.cells.remove(self.cursor_y);
                        self.cells.push(vec![' '; self.cols]);
                    }
                }
                self.dirty = true;
            }

            // Cursor Up (CSI n A)
            'A' => {
                let n = if p0 == 0 { 1 } else { p0 };
                self.cursor_y = self.cursor_y.saturating_sub(n);
                self.dirty = true;
            }
            // Cursor Down (CSI n B)
            'B' => {
                let n = if p0 == 0 { 1 } else { p0 };
                self.cursor_y = (self.cursor_y + n).min(self.rows.saturating_sub(1));
                self.dirty = true;
            }
            // Cursor Forward (CSI n C)
            'C' => {
                let n = if p0 == 0 { 1 } else { p0 };
                self.cursor_x = (self.cursor_x + n).min(self.cols.saturating_sub(1));
                self.dirty = true;
            }
            // Cursor Backward (CSI n D)
            'D' => {
                let n = if p0 == 0 { 1 } else { p0 };
                self.cursor_x = self.cursor_x.saturating_sub(n);
                self.dirty = true;
            }

            // Save Cursor (CSI s)
            's' => {
                self.saved_cursor_x = self.cursor_x;
                self.saved_cursor_y = self.cursor_y;
            }

            // Restore Cursor (CSI u)
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
        assert_eq!(&grid.cells[0][..5], &['h', 'e', 'l', 'l', 'o']);

        // Backspace \x08 moves cursor back
        grid.advance_bytes(b"\x08");
        assert_eq!(grid.cursor_x, 4);

        // Erase to end \x1b[K
        grid.advance_bytes(b"\x1b[K");
        assert_eq!(&grid.cells[0][..5], &['h', 'e', 'l', 'l', ' ']);
    }

    #[test]
    fn test_cha_and_line_erase() {
        let mut grid = TerminalGrid::new(40, 5);
        grid.advance_bytes(b"PS C:\\Users> Get-Process");
        assert_eq!(grid.cursor_x, 24);

        // PSReadLine moves cursor to col 14 (1-based: CSI 14 G) and erases line
        grid.advance_bytes(b"\x1b[14G\x1b[K");
        assert_eq!(grid.cursor_x, 13);
        assert_eq!(&grid.cells[0][..13], &['P', 'S', ' ', 'C', ':', '\\', 'U', 's', 'e', 'r', 's', '>', ' ']);
        assert_eq!(grid.cells[0][13], ' ');
        assert_eq!(grid.cells[0][20], ' ');
    }

    #[test]
    fn test_cursor_navigation_and_dsr() {
        let mut grid = TerminalGrid::new(40, 10);
        grid.advance_bytes(b"\x1b[3;5H");
        assert_eq!(grid.cursor_y, 2);
        assert_eq!(grid.cursor_x, 4);

        // DSR query
        grid.advance_bytes(b"\x1b[6n");
        assert_eq!(grid.responses.len(), 1);
        assert_eq!(grid.responses[0], b"\x1b[3;5R");

        // Cursor Left
        grid.advance_bytes(b"\x1b[2D");
        assert_eq!(grid.cursor_x, 2);

        // Cursor Down
        grid.advance_bytes(b"\x1b[3B");
        assert_eq!(grid.cursor_y, 5);
    }
}
