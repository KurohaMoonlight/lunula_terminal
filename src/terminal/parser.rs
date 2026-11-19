//! ANSI/VT100 escape sequence parser (`vte::Perform`) with stack-allocated SGR and row recycling.

use super::cell::{color_256, Cell, CellStyle, ColorRgb, CursorShape};
use super::TerminalGrid;
use unicode_width::UnicodeWidthChar;
use vte::{Params, Perform};

impl TerminalGrid {
    pub(crate) fn handle_sgr(&mut self, params: &[u16]) {
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
                    self.current_style.set_bold(true);
                    i += 1;
                }
                2 => {
                    self.current_style.set_dim(true);
                    i += 1;
                }
                3 => {
                    self.current_style.set_italic(true);
                    i += 1;
                }
                4 => {
                    self.current_style.set_underline(true);
                    i += 1;
                }
                7 => {
                    self.current_style.set_reverse(true);
                    i += 1;
                }
                9 => {
                    self.current_style.set_strikethrough(true);
                    i += 1;
                }
                22 => {
                    self.current_style.set_bold(false);
                    self.current_style.set_dim(false);
                    i += 1;
                }
                23 => {
                    self.current_style.set_italic(false);
                    i += 1;
                }
                24 => {
                    self.current_style.set_underline(false);
                    self.current_style.set_undercurl(false);
                    i += 1;
                }
                27 => {
                    self.current_style.set_reverse(false);
                    i += 1;
                }
                29 => {
                    self.current_style.set_strikethrough(false);
                    i += 1;
                }
                30..=37 => {
                    let idx = (params[i] - 30) as usize;
                    self.current_style.fg = Some(self.palette[idx.min(15)]);
                    i += 1;
                }
                39 => {
                    self.current_style.fg = None;
                    i += 1;
                }
                40..=47 => {
                    let idx = (params[i] - 40) as usize;
                    self.current_style.bg = Some(self.palette[idx.min(15)]);
                    i += 1;
                }
                49 => {
                    self.current_style.bg = None;
                    i += 1;
                }
                90..=97 => {
                    let idx = (params[i] - 90 + 8) as usize;
                    self.current_style.fg = Some(self.palette[idx.min(15)]);
                    i += 1;
                }
                100..=107 => {
                    let idx = (params[i] - 100 + 8) as usize;
                    self.current_style.bg = Some(self.palette[idx.min(15)]);
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
            // Attach combining diacritics, variation selectors (e.g. U+FE0F), and ZWJ (U+200D) to the preceding cell
            if self.cursor_x > 0 && self.cursor_y < self.rows {
                let mut target_col = self.cursor_x - 1;
                while target_col > 0 && self.cells[self.cursor_y][target_col].c == '\0' {
                    target_col -= 1;
                }
                let abs_r = self.screen_to_abs_row(self.cursor_y);
                self.combining_chars.entry((abs_r, target_col)).or_default().push(c);
                let row_idx = self.cursor_y;
                self.mark_row_dirty(row_idx);
            }
            return;
        }

        if self.cursor_x + w > self.cols {
            if self.cursor_x > 0 && self.cursor_y < self.rows {
                self.cells[self.cursor_y][self.cursor_x - 1].style.set_wrapped(true);
            }
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
                        c: '\0',
                        style: self.current_style,
                        hyperlink_idx: self.current_hyperlink_idx,
                    };
                }
            }
            let row_idx = self.cursor_y;
            self.cursor_x += w;
            self.mark_row_dirty(row_idx);
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x0A | 0x0B | 0x0C => {
                if self.cursor_y == self.scroll_region_bottom {
                    self.scroll_region_up();
                } else if self.cursor_y + 1 < self.rows {
                    self.cursor_y += 1;
                    self.mark_row_dirty(self.cursor_y);
                }
            }
            0x0D => {
                self.cursor_x = 0;
                self.mark_row_dirty(self.cursor_y);
            }
            0x08 => {
                if self.cursor_x > 0 {
                    self.cursor_x -= 1;
                    self.mark_row_dirty(self.cursor_y);
                }
            }
            0x09 => {
                self.cursor_x = ((self.cursor_x / 8) + 1) * 8;
                if self.cursor_x >= self.cols {
                    self.cursor_x = self.cols.saturating_sub(1);
                }
                self.mark_row_dirty(self.cursor_y);
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
        } else if params[0] == b"7" {
            // OSC 7: Current Working Directory, e.g. "file://localhost/C:/path" or "file:///C:/path"
            if params.len() >= 2 {
                if let Ok(raw) = std::str::from_utf8(params[1]) {
                    let path_str = raw.strip_prefix("file://").unwrap_or(raw);
                    let path_str = path_str.strip_prefix("localhost").unwrap_or(path_str);
                    let path_str = if path_str.len() >= 3
                        && path_str.starts_with('/')
                        && path_str.as_bytes()[2] == b':'
                    {
                        &path_str[1..]
                    } else {
                        path_str
                    };
                    let pb = std::path::PathBuf::from(path_str);
                    if pb.exists() {
                        self.cwd = Some(pb);
                    }
                }
            }
        } else if params[0] == b"9" || params[0] == b"9;9" {
            // OSC 9;9: ConEmu / Windows Terminal CWD sequence: OSC 9 ; 9 ; "path" ST
            let raw_path = if params[0] == b"9;9" && params.len() >= 2 {
                std::str::from_utf8(params[1]).ok()
            } else if params.len() >= 3 && params[1] == b"9" {
                std::str::from_utf8(params[2]).ok()
            } else {
                None
            };
            if let Some(raw) = raw_path {
                let trimmed = raw.trim().trim_matches('"');
                let pb = std::path::PathBuf::from(trimmed);
                if pb.exists() {
                    self.cwd = Some(pb);
                }
            }
        } else if params[0] == b"8" {
            if params.len() >= 3 {
                let url = std::str::from_utf8(params[2]).unwrap_or("");
                if url.is_empty() {
                    self.current_hyperlink_idx = 0;
                } else if let Some(existing_pos) = self.hyperlinks.iter().rposition(|u| u == url) {
                    self.current_hyperlink_idx = (existing_pos + 1) as u16;
                } else {
                    // Periodic / capacity compaction:
                    // 1. Table grew to >= 512 entries and has added >= 128 new entries since last prune, or
                    // 2. Table approaches the u16::MAX index limit.
                    if (self.hyperlinks.len() >= 512
                        && self.hyperlinks.len().saturating_sub(self.last_hyperlink_prune_len) >= 128)
                        || self.hyperlinks.len() >= u16::MAX as usize - 2
                    {
                        self.prune_hyperlinks();
                    }

                    // Check again in case compaction retained this URL or freed space
                    if let Some(existing_pos) = self.hyperlinks.iter().rposition(|u| u == url) {
                        self.current_hyperlink_idx = (existing_pos + 1) as u16;
                    } else if self.hyperlinks.len() < u16::MAX as usize - 1 {
                        let idx = (self.hyperlinks.len() + 1) as u16;
                        self.hyperlinks.push(url.to_string());
                        self.current_hyperlink_idx = idx;
                    }
                }
            } else {
                self.current_hyperlink_idx = 0;
            }
        } else if params[0] == b"10" {
            // OSC 10: Foreground Color Query (OSC 10 ; ? ST)
            if params.len() >= 2 && params[1] == b"?" {
                let resp = format!(
                    "\x1b]10;rgb:{:02x}00/{:02x}00/{:02x}00\x1b\\",
                    self.fg_color.r, self.fg_color.g, self.fg_color.b
                );
                self.responses.push(resp.into_bytes());
            }
        } else if params[0] == b"11" {
            // OSC 11: Background Color Query (OSC 11 ; ? ST)
            if params.len() >= 2 && params[1] == b"?" {
                let resp = format!(
                    "\x1b]11;rgb:{:02x}00/{:02x}00/{:02x}00\x1b\\",
                    self.bg_color.r, self.bg_color.g, self.bg_color.b
                );
                self.responses.push(resp.into_bytes());
            }
        } else if params[0] == b"12" {
            // OSC 12: Cursor Color Query (OSC 12 ; ? ST)
            if params.len() >= 2 && params[1] == b"?" {
                let c = self.cursor_config.color;
                let resp = format!(
                    "\x1b]12;rgb:{:02x}00/{:02x}00/{:02x}00\x1b\\",
                    c.r, c.g, c.b
                );
                self.responses.push(resp.into_bytes());
            }
        } else if params[0] == b"52" {
            // OSC 52: Clipboard Manipulation: OSC 52 ; Pc ; Pd ST
            if params.len() >= 3 {
                if params[2] == b"?" {
                    if self.allow_clipboard_read {
                        if let Some(text) = crate::platform::clipboard::get_text() {
                            let encoded = crate::terminal::graphics::encode_base64(text.as_bytes());
                            let target = std::str::from_utf8(params[1]).unwrap_or("c");
                            let resp = format!("\x1b]52;{};{}\x1b\\", target, encoded);
                            self.responses.push(resp.into_bytes());
                        }
                    } else {
                        log::warn!(
                            "Ignored OSC 52 clipboard read query: allow_clipboard_read is disabled in configuration"
                        );
                    }
                } else if let Some(decoded) = crate::terminal::graphics::decode_base64(params[2]) {
                    if let Ok(text) = String::from_utf8(decoded) {
                        crate::platform::clipboard::set_text(&text);
                    }
                }
            }
        }
    }

    fn hook(&mut self, _params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        if action == 'G' || intermediates.contains(&b'G') {
            self.dcs_graphics_buffer.clear();
        }
    }

    fn put(&mut self, byte: u8) {
        if self.dcs_graphics_buffer.len() < 100 * 1024 * 1024 {
            self.dcs_graphics_buffer.push(byte);
        }
    }

    fn unhook(&mut self) {
        if !self.dcs_graphics_buffer.is_empty() {
            let buf = std::mem::take(&mut self.dcs_graphics_buffer);
            self.handle_apc_kitty(&buf);
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        let is_private = intermediates.contains(&b'?');
        let is_secondary = intermediates.contains(&b'>');
        let mut it = params.iter();
        let p0 = it.next().and_then(|p| p.first()).copied().unwrap_or(0) as usize;
        let p1 = it.next().and_then(|p| p.first()).copied().unwrap_or(0) as usize;

        if action == '_' {
            return;
        }

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

        if action == 'c' {
            if is_secondary {
                self.responses.push(b"\x1b[>0;10;1c".to_vec());
            } else {
                self.responses.push(b"\x1b[?1;2c".to_vec());
            }
            return;
        }

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
                        self.mark_row_dirty(self.cursor_y);
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
                            self.mark_all_rows_dirty();
                        }
                    }
                    2004 => self.bracketed_paste = true,
                    1004 => self.focus_reporting = true,
                    // DEC Private Mode ?2026h: Begin Synchronized Update (BSU)
                    2026 => {
                        self.in_sync_update = true;
                        self.sync_update_start = Some(std::time::Instant::now());
                    }
                    1000 | 1002 | 1003 => self.mouse_reporting = true,
                    1006 => self.sgr_mouse = true,
                    _ => {}
                },
                'l' => match p0 {
                    25 => {
                        self.cursor_visible = false;
                        self.mark_row_dirty(self.cursor_y);
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
                            self.mark_all_rows_dirty();
                        }
                    }
                    2004 => self.bracketed_paste = false,
                    1004 => self.focus_reporting = false,
                    // DEC Private Mode ?2026l: End Synchronized Update (ESU)
                    2026 => {
                        self.in_sync_update = false;
                        self.sync_update_start = None;
                        self.mark_all_rows_dirty();
                    }
                    1000 | 1002 | 1003 => self.mouse_reporting = false,
                    1006 => self.sgr_mouse = false,
                    _ => {}
                },
                'p' if intermediates.contains(&b'$') => {
                    // DECRQM (DEC Request Mode) for Mode 2026
                    if p0 == 2026 {
                        let status = if self.in_sync_update { 1 } else { 2 };
                        let resp = format!("\x1b[?2026;{}$y", status);
                        self.responses.push(resp.into_bytes());
                    } else if p0 == 1004 {
                        let status = if self.focus_reporting { 1 } else { 2 };
                        let resp = format!("\x1b[?1004;{}$y", status);
                        self.responses.push(resp.into_bytes());
                    }
                }
                'u' => {
                    // Kitty Keyboard Protocol: CSI ? u (Query current active flags)
                    let resp = format!("\x1b[?{}u", self.kitty_keyboard_flags);
                    self.responses.push(resp.into_bytes());
                }
                _ => {}
            }
            return;
        }

        match action {
            // SGR: Zero-allocation stack buffer for style parameters
            'm' => {
                let mut flat = [0u16; 32];
                let mut len = 0;
                for slice in params.iter() {
                    if slice.first() == Some(&4) && slice.get(1) == Some(&3) {
                        self.current_style.set_undercurl(true);
                    }
                    for &val in slice {
                        if len < flat.len() {
                            flat[len] = val;
                            len += 1;
                        }
                    }
                }
                self.handle_sgr(&flat[..len]);
            }

            'H' | 'f' => {
                let row = if p0 == 0 { 1 } else { p0 };
                let col = if p1 == 0 { 1 } else { p1 };
                self.cursor_y = (row - 1).min(self.rows.saturating_sub(1));
                self.cursor_x = (col - 1).min(self.cols.saturating_sub(1));
                self.dirty = true;
            }

            'G' | '`' => {
                let col = if p0 == 0 { 1 } else { p0 };
                self.cursor_x = (col - 1).min(self.cols.saturating_sub(1));
                self.dirty = true;
            }

            'd' => {
                let row = if p0 == 0 { 1 } else { p0 };
                self.cursor_y = (row - 1).min(self.rows.saturating_sub(1));
                self.dirty = true;
            }

            'E' => {
                let n = if p0 == 0 { 1 } else { p0 };
                self.cursor_x = 0;
                self.cursor_y = (self.cursor_y + n).min(self.rows.saturating_sub(1));
                self.dirty = true;
            }

            'F' => {
                let n = if p0 == 0 { 1 } else { p0 };
                self.cursor_x = 0;
                self.cursor_y = self.cursor_y.saturating_sub(n);
                self.dirty = true;
            }

            'J' => {
                match p0 {
                    0 => {
                        if self.cursor_y < self.rows {
                            self.cells[self.cursor_y][self.cursor_x..self.cols].fill(Cell::default());
                            for r in (self.cursor_y + 1)..self.rows {
                                self.cells[r].fill(Cell::default());
                            }
                        }
                    }
                    1 => {
                        for r in 0..self.cursor_y {
                            self.cells[r].fill(Cell::default());
                        }
                        if self.cursor_y < self.rows {
                            let end = self.cursor_x.min(self.cols - 1);
                            self.cells[self.cursor_y][0..=end].fill(Cell::default());
                        }
                    }
                    2 => {
                        for r in 0..self.rows {
                            self.cells[r].fill(Cell::default());
                        }
                        // ECMA-48 / VT100: CSI 2 J erases display without resetting cursor
                        self.graphics.images.clear();
                    }
                    3 => {
                        // xterm / VT: CSI 3 J erases saved lines (scrollback buffer only)
                        self.scrollback.clear();
                        self.scroll_offset = 0;
                        self.target_scroll_y = 0.0;
                        self.current_scroll_y = 0.0;
                        if !self.hyperlinks.is_empty() {
                            self.prune_hyperlinks();
                        }
                    }
                    _ => {}
                }
                self.mark_all_rows_dirty();
            }

            'K' => {
                if self.cursor_y < self.rows {
                    let blank = Cell {
                        c: ' ',
                        style: self.current_style,
                        hyperlink_idx: 0,
                    };
                    match p0 {
                        0 => {
                            self.cells[self.cursor_y][self.cursor_x..self.cols].fill(blank);
                        }
                        1 => {
                            let end = self.cursor_x.min(self.cols - 1);
                            self.cells[self.cursor_y][0..=end].fill(blank);
                        }
                        2 => {
                            self.cells[self.cursor_y].fill(blank);
                        }
                        _ => {}
                    }
                    self.mark_row_dirty(self.cursor_y);
                }
            }

            'X' => {
                let n = if p0 == 0 { 1 } else { p0 };
                if self.cursor_y < self.rows {
                    let end = (self.cursor_x + n).min(self.cols);
                    self.cells[self.cursor_y][self.cursor_x..end].fill(Cell::default());
                    self.mark_row_dirty(self.cursor_y);
                }
            }

            'P' => {
                let n = if p0 == 0 { 1 } else { p0 };
                if self.cursor_y < self.rows && self.cursor_x < self.cols {
                    let count = n.min(self.cols - self.cursor_x);
                    let row = &mut self.cells[self.cursor_y];
                    row.copy_within((self.cursor_x + count)..self.cols, self.cursor_x);
                    row[(self.cols - count)..self.cols].fill(Cell::default());
                    self.mark_row_dirty(self.cursor_y);
                }
            }

            '@' => {
                let n = if p0 == 0 { 1 } else { p0 };
                if self.cursor_y < self.rows && self.cursor_x < self.cols {
                    let count = n.min(self.cols - self.cursor_x);
                    let row = &mut self.cells[self.cursor_y];
                    row.copy_within(self.cursor_x..(self.cols - count), self.cursor_x + count);
                    row[self.cursor_x..(self.cursor_x + count)].fill(Cell::default());
                    self.mark_row_dirty(self.cursor_y);
                }
            }

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
                    self.mark_all_rows_dirty();
                }
            }

            'S' => {
                let n = (if p0 == 0 { 1 } else { p0 }).min(self.rows);
                for _ in 0..n {
                    self.scroll_region_up();
                }
                self.mark_all_rows_dirty();
            }

            'T' => {
                let n = (if p0 == 0 { 1 } else { p0 }).min(self.rows);
                for _ in 0..n {
                    self.scroll_region_down();
                }
                self.mark_all_rows_dirty();
            }

            // Insert Lines (recycles bottom row of scroll region, 0 heap allocation)
            'L' => {
                if self.cursor_y >= self.scroll_region_top && self.cursor_y <= self.scroll_region_bottom {
                    let n = (if p0 == 0 { 1 } else { p0 })
                        .min(self.scroll_region_bottom.saturating_sub(self.cursor_y) + 1);
                    for _ in 0..n {
                        let mut recycled = self.cells.remove(self.scroll_region_bottom);
                        recycled.fill(Cell::default());
                        self.cells.insert(self.cursor_y, recycled);
                    }
                    self.mark_all_rows_dirty();
                }
            }

            // Delete Lines (recycles deleted row, 0 heap allocation)
            'M' => {
                if self.cursor_y >= self.scroll_region_top && self.cursor_y <= self.scroll_region_bottom {
                    let n = (if p0 == 0 { 1 } else { p0 })
                        .min(self.scroll_region_bottom.saturating_sub(self.cursor_y) + 1);
                    for _ in 0..n {
                        let mut recycled = self.cells.remove(self.cursor_y);
                        recycled.fill(Cell::default());
                        self.cells.insert(self.scroll_region_bottom, recycled);
                    }
                    self.mark_all_rows_dirty();
                }
            }

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

            's' => {
                self.saved_cursor_x = self.cursor_x;
                self.saved_cursor_y = self.cursor_y;
            }
            'u' => {
                if intermediates.contains(&b'>') {
                    // Kitty Keyboard Protocol: CSI > flags u (Push flags onto stack)
                    let flags = p0 as u8;
                    self.kitty_keyboard_stack.push(self.kitty_keyboard_flags);
                    if self.kitty_keyboard_stack.len() > 32 {
                        self.kitty_keyboard_stack.remove(0);
                    }
                    self.kitty_keyboard_flags = flags;
                } else if intermediates.contains(&b'<') {
                    // Kitty Keyboard Protocol: CSI < count u (Pop flags from stack)
                    let count = if p0 == 0 { 1 } else { p0 };
                    for _ in 0..count {
                        if let Some(prev) = self.kitty_keyboard_stack.pop() {
                            self.kitty_keyboard_flags = prev;
                        } else {
                            self.kitty_keyboard_flags = 0;
                            break;
                        }
                    }
                } else if intermediates.contains(&b'?') {
                    // Kitty Keyboard Protocol: CSI ? u (Query current active flags)
                    let resp = format!("\x1b[?{}u", self.kitty_keyboard_flags);
                    self.responses.push(resp.into_bytes());
                } else if intermediates.contains(&b'=') {
                    // Kitty Keyboard Protocol: CSI = flags ; mode u (Set/modify flags)
                    let flags = p0 as u8;
                    let mode = if p1 == 0 { 1 } else { p1 };
                    match mode {
                        1 => self.kitty_keyboard_flags = flags,
                        2 => self.kitty_keyboard_flags |= flags,
                        3 => self.kitty_keyboard_flags &= !flags,
                        _ => {}
                    }
                } else {
                    // Traditional VT: Restore Cursor
                    self.cursor_x = self.saved_cursor_x.min(self.cols.saturating_sub(1));
                    self.cursor_y = self.saved_cursor_y.min(self.rows.saturating_sub(1));
                    self.dirty = true;
                }
            }

            _ => {}
        }
    }
}
