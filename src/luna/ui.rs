//! Catppuccin Mocha ANSI/TrueColor TUI renderer for `luna`.

use super::buffer::TextBuffer;
use std::io::{self, BufWriter, Write};

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

    let view_rows = rows.saturating_sub(3).max(1);
    let total_lines_digits = buf.lines.len().to_string().len().max(3);
    let gutter_w = total_lines_digits + 3; // e.g. "  1 │ "
    let text_cols = cols.saturating_sub(gutter_w).max(10);

    // Hide cursor while drawing frame
    out.write_all(b"\x1b[?25l\x1b[H")?;

    // 1. Top Header Bar (Catppuccin Mantle #181825 + Lavender #b4befe)
    let modified_tag = if buf.dirty { " [Modified]" } else { "" };
    let left_title = format!("  🌙 LUNA  │  {}{}", buf.filename_display(), modified_tag);
    let right_pos = format!(
        "Ln {}, Col {}  ",
        buf.cursor_row + 1,
        buf.cursor_col + 1
    );
    let pad = cols.saturating_sub(left_title.chars().count() + right_pos.chars().count());
    write!(
        out,
        "\x1b[48;2;24;24;37m\x1b[38;2;180;190;254m\x1b[1m{}\x1b[0m\x1b[48;2;24;24;37m\x1b[38;2;166;173;200m{}{}\x1b[K\x1b[0m\r\n",
        left_title,
        " ".repeat(pad),
        right_pos
    )?;

    // 2. Editor Viewport Lines
    for vr in 0..view_rows {
        let line_idx = buf.scroll_row + vr;
        if line_idx < buf.lines.len() {
            let is_active_line = line_idx == buf.cursor_row;
            if is_active_line {
                write!(
                    out,
                    "\x1b[38;2;180;190;254m\x1b[1m{:>width$} \x1b[0m\x1b[38;2;69;71;90m│ \x1b[0m",
                    line_idx + 1,
                    width = total_lines_digits
                )?;
            } else {
                write!(
                    out,
                    "\x1b[38;2;108;112;134m{:>width$} \x1b[38;2;69;71;90m│ \x1b[0m",
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
            write!(out, "\x1b[38;2;205;214;244m{}\x1b[K\x1b[0m\r\n", visible)?;
        } else {
            write!(
                out,
                "\x1b[38;2;88;91;112m{:>width$} │ ~\x1b[K\x1b[0m\r\n",
                "",
                width = total_lines_digits
            )?;
        }
    }

    // 3. Status / Prompt Bar
    match prompt {
        PromptMode::Search(query) => {
            write!(
                out,
                "\x1b[48;2;49;50;68m\x1b[38;2;249;226;175m Search: \x1b[38;2;205;214;244m{}\x1b[K\x1b[0m\r\n",
                query
            )?;
        }
        PromptMode::ConfirmExit => {
            write!(
                out,
                "\x1b[48;2;243;139;168m\x1b[38;2;17;17;27m\x1b[1m Save modified buffer? (Y = Save, N = Discard, Esc = Cancel)\x1b[K\x1b[0m\r\n"
            )?;
        }
        PromptMode::None => {
            write!(
                out,
                "\x1b[48;2;30;30;46m\x1b[38;2;166;227;161m {}\x1b[K\x1b[0m\r\n",
                status_msg
            )?;
        }
    }

    // 4. Bottom Shortcut Legend
    write!(
        out,
        "\x1b[48;2;24;24;37m\x1b[38;2;137;180;250m ^S \x1b[38;2;186;194;222mSave   \x1b[38;2;137;180;250m^X \x1b[38;2;186;194;222mExit   \x1b[38;2;137;180;250m^F \x1b[38;2;186;194;222mFind   \x1b[38;2;137;180;250m^Z \x1b[38;2;186;194;222mUndo\x1b[K\x1b[0m"
    )?;

    // Position terminal cursor
    let screen_row = match prompt {
        PromptMode::Search(_) => rows - 1,
        _ => (buf.cursor_row.saturating_sub(buf.scroll_row)) + 2,
    };
    let screen_col = match prompt {
        PromptMode::Search(q) => 10 + q.chars().count(),
        _ => gutter_w + buf.cursor_col.saturating_sub(buf.scroll_col) + 1,
    };

    write!(out, "\x1b[{};{}H\x1b[?25h", screen_row, screen_col)?;
    out.flush()?;
    Ok(())
}
