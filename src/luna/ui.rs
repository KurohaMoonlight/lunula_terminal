//! Kanagawa Wave ANSI/TrueColor TUI renderer for `luna`.

use super::buffer::TextBuffer;
use std::io::{self, BufWriter, Write};
use unicode_width::UnicodeWidthStr;

pub enum PromptMode {
    None,
    Search(String),
    ConfirmExit,
}

pub fn render(
    buf: &TextBuffer,
    cols: usize,
    rows: usize,
    status_msg: &str,
    prompt: &PromptMode,
) -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = BufWriter::with_capacity(8192, stdout.lock());
    render_to(buf, cols, rows, status_msg, prompt, &mut out)?;
    out.flush()?;
    Ok(())
}

pub fn render_to<W: Write>(
    buf: &TextBuffer,
    cols: usize,
    rows: usize,
    status_msg: &str,
    prompt: &PromptMode,
    out: &mut W,
) -> io::Result<()> {
    let rows_safe = rows.max(4);
    let view_rows = rows_safe.saturating_sub(3).max(1);
    let total_lines_digits = buf.lines.len().to_string().len().max(3);
    let gutter_w = total_lines_digits + 3; // e.g. "  1 │ "
    let text_cols = cols.saturating_sub(gutter_w).max(10);

    // Hide cursor while drawing frame
    out.write_all(b"\x1b[?25l")?;

    // 1. Top Header Bar (Row 1): Kanagawa sumiInk0 #16161D + springBlue #7fb4ca
    // Position explicitly with absolute row coordinates (no trailing \r\n to prevent viewport scrolling)
    let modified_tag = if buf.dirty { " [Modified]" } else { "" };
    let left_title = format!("  🌙 LUNA  │  {}{}", buf.filename_display(), modified_tag);
    let right_pos = format!(
        "Ln {}, Col {}  ",
        buf.cursor_row + 1,
        buf.cursor_col + 1
    );
    let left_w = UnicodeWidthStr::width(left_title.as_str());
    let right_w = UnicodeWidthStr::width(right_pos.as_str());
    let pad = cols.saturating_sub(left_w + right_w);

    write!(
        out,
        "\x1b[1;1H\x1b[48;2;22;22;29m\x1b[38;2;127;180;202m\x1b[1m{}\x1b[0m\x1b[48;2;22;22;29m\x1b[38;2;200;192;147m{}{}\x1b[K\x1b[0m",
        left_title,
        " ".repeat(pad),
        right_pos
    )?;

    // 2. Editor Viewport Lines (Rows 2 ..= view_rows + 1)
    for vr in 0..view_rows {
        let screen_r = vr + 2;
        let line_idx = buf.scroll_row + vr;

        write!(out, "\x1b[{};1H", screen_r)?;

        if line_idx < buf.lines.len() {
            let is_active_line = line_idx == buf.cursor_row;
            if is_active_line {
                write!(
                    out,
                    "\x1b[38;2;230;195;132m\x1b[1m{:>width$} \x1b[0m\x1b[38;2;54;54;70m│ \x1b[0m",
                    line_idx + 1,
                    width = total_lines_digits
                )?;
            } else {
                write!(
                    out,
                    "\x1b[38;2;114;113;105m{:>width$} \x1b[38;2;54;54;70m│ \x1b[0m",
                    line_idx + 1,
                    width = total_lines_digits
                )?;
            }

            let line = &buf.lines[line_idx];
            let visible: String = line
                .chars()
                .skip(buf.scroll_col)
                .take(text_cols)
                .collect();
            write!(out, "\x1b[38;2;220;215;186m{}\x1b[K\x1b[0m", visible)?;
        } else {
            write!(
                out,
                "\x1b[38;2;54;54;70m{:>width$} │ ~\x1b[K\x1b[0m",
                "",
                width = total_lines_digits
            )?;
        }
    }

    // 3. Status / Prompt Bar (Row rows_safe - 1)
    let status_row = rows_safe.saturating_sub(1);
    write!(out, "\x1b[{};1H", status_row)?;
    match prompt {
        PromptMode::Search(query) => {
            write!(
                out,
                "\x1b[48;2;45;79;103m\x1b[38;2;230;195;132m Search: \x1b[38;2;220;215;186m{}\x1b[K\x1b[0m",
                query
            )?;
        }
        PromptMode::ConfirmExit => {
            write!(
                out,
                "\x1b[48;2;195;64;67m\x1b[38;2;22;22;29m\x1b[1m Save modified buffer? (Y = Save, N = Discard, Esc = Cancel)\x1b[K\x1b[0m"
            )?;
        }
        PromptMode::None => {
            write!(
                out,
                "\x1b[48;2;31;31;40m\x1b[38;2;152;187;108m {}\x1b[K\x1b[0m",
                status_msg
            )?;
        }
    }

    // 4. Bottom Shortcut Legend (Row rows_safe)
    write!(
        out,
        "\x1b[{};1H\x1b[48;2;22;22;29m\x1b[38;2;127;180;202m ^S \x1b[38;2;200;192;147mSave   \x1b[38;2;127;180;202m^X \x1b[38;2;200;192;147mExit   \x1b[38;2;127;180;202m^F \x1b[38;2;200;192;147mFind   \x1b[38;2;127;180;202m^Z \x1b[38;2;200;192;147mUndo\x1b[K\x1b[0m",
        rows_safe
    )?;

    // Position terminal cursor
    let screen_row = match prompt {
        PromptMode::Search(_) => status_row,
        _ => (buf.cursor_row.saturating_sub(buf.scroll_row)) + 2,
    };
    let screen_col = match prompt {
        PromptMode::Search(q) => 10 + UnicodeWidthStr::width(q.as_str()),
        _ => {
            let line = buf
                .lines
                .get(buf.cursor_row)
                .map(|s| s.as_str())
                .unwrap_or("");
            let visible_chars: String = line
                .chars()
                .skip(buf.scroll_col)
                .take(buf.cursor_col.saturating_sub(buf.scroll_col))
                .collect();
            let col_offset = UnicodeWidthStr::width(visible_chars.as_str());
            gutter_w + col_offset + 1
        }
    };

    write!(out, "\x1b[{};{}H\x1b[?25h", screen_row, screen_col)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_no_viewport_scrolling_newlines() {
        let buf = TextBuffer::open(None).unwrap();
        let mut out = Vec::new();
        render_to(&buf, 80, 24, "Ready", &PromptMode::None, &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();

        // Ensure there are no newlines in the rendered output that would scroll the terminal viewport
        assert!(!s.contains('\n'), "Render output must not contain newlines which scroll the viewport");
        assert!(s.contains("\x1b[1;1H"), "Header bar must be placed explicitly at row 1");
        assert!(s.contains("\x1b[2;1H"), "First viewport line must be placed explicitly at row 2");
        assert!(s.contains("🌙 LUNA"), "Must contain editor brand title");
        assert!(s.contains("Ln 1, Col 1"), "Must display Ln and Col");
    }
}
