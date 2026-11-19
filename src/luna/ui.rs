//! Multi-theme ANSI/TrueColor TUI renderer for `luna`.

use super::buffer::TextBuffer;
use super::syntax::{self, Language};
use crate::theme::Theme;
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
    theme: &Theme,
) -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = BufWriter::with_capacity(8192, stdout.lock());
    render_to(buf, cols, rows, status_msg, prompt, theme, &mut out)?;
    out.flush()?;
    Ok(())
}

pub fn render_to<W: Write>(
    buf: &TextBuffer,
    cols: usize,
    rows: usize,
    status_msg: &str,
    prompt: &PromptMode,
    theme: &Theme,
    out: &mut W,
) -> io::Result<()> {
    let rows_safe = rows.max(4);
    let view_rows = rows_safe.saturating_sub(3).max(1);
    let total_lines_digits = buf.lines.len().to_string().len().max(3);
    let gutter_w = total_lines_digits + 3; // e.g. "  1 │ "
    let text_cols = cols.saturating_sub(gutter_w).max(10);

    // Hide cursor while drawing frame
    out.write_all(b"\x1b[?25l")?;

    // 1. Top Header Bar (Row 1): Styled using active theme
    let lang = Language::from_path(buf.path.as_deref());
    let modified_tag = if buf.dirty { " [Modified]" } else { "" };
    let lang_tag = lang.display_tag();
    let left_title = format!("  🌙 LUNA  │  {}{}{}", buf.filename_display(), modified_tag, lang_tag);
    let right_pos = format!(
        "Ln {}, Col {}  ",
        buf.cursor_row + 1,
        buf.cursor_col + 1
    );
    let left_w = UnicodeWidthStr::width(left_title.as_str());
    let right_w = UnicodeWidthStr::width(right_pos.as_str());
    let pad = cols.saturating_sub(left_w + right_w);

    let bg_code = theme.tab_bar.to_bg_ansi();
    let fg_code = theme.fg.to_fg_ansi();
    let title_code = theme.cursor.to_fg_ansi();

    write!(
        out,
        "\x1b[1;1H{}{}\x1b[1m{}\x1b[0m{}{}{}{}\x1b[K\x1b[0m",
        bg_code,
        title_code,
        left_title,
        bg_code,
        fg_code,
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
                    "{}\x1b[1m{:>width$} \x1b[0m{}│ \x1b[0m",
                    theme.gutter_active.to_fg_ansi(),
                    line_idx + 1,
                    theme.border.to_fg_ansi(),
                    width = total_lines_digits
                )?;
            } else {
                write!(
                    out,
                    "{}{:>width$} \x1b[0m{}│ \x1b[0m",
                    theme.gutter_fg.to_fg_ansi(),
                    line_idx + 1,
                    theme.border.to_fg_ansi(),
                    width = total_lines_digits
                )?;
            }

            let line = &buf.lines[line_idx];
            syntax::render_highlighted_line(line, lang, buf.scroll_col, text_cols, theme, out)?;
        } else {
            write!(
                out,
                "{}{:>width$} │ ~\x1b[K\x1b[0m",
                theme.border.to_fg_ansi(),
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
                "{}{} Search: {}{}\x1b[K\x1b[0m",
                theme.selection.to_bg_ansi(),
                theme.gutter_active.to_fg_ansi(),
                theme.fg.to_fg_ansi(),
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
                "{}{} {}\x1b[K\x1b[0m",
                theme.bg.to_bg_ansi(),
                theme.syntax_string.to_fg_ansi(),
                status_msg
            )?;
        }
    }

    // 4. Bottom Shortcut Legend (Row rows_safe)
    let bar_bg = theme.tab_bar.to_bg_ansi();
    let key_fg = theme.syntax_type.to_fg_ansi();
    let lbl_fg = theme.fg.to_fg_ansi();
    write!(
        out,
        "\x1b[{};1H{}{} ^S {}{}Save   {}^X {}{}Exit   {}^F {}{}Find   {}^Z {}{}Undo\x1b[K\x1b[0m",
        rows_safe,
        bar_bg,
        key_fg,
        bar_bg,
        lbl_fg,
        key_fg,
        bar_bg,
        lbl_fg,
        key_fg,
        bar_bg,
        lbl_fg,
        key_fg,
        bar_bg,
        lbl_fg
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
        let theme = Theme::kanagawa();
        render_to(&buf, 80, 24, "Ready", &PromptMode::None, &theme, &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();

        // Ensure there are no newlines in the rendered output that would scroll the terminal viewport
        assert!(!s.contains('\n'), "Render output must not contain newlines which scroll the viewport");
        assert!(s.contains("\x1b[1;1H"), "Header bar must be placed explicitly at row 1");
        assert!(s.contains("\x1b[2;1H"), "First viewport line must be placed explicitly at row 2");
        assert!(s.contains("🌙 LUNA"), "Must contain editor brand title");
        assert!(s.contains("Ln 1, Col 1"), "Must display Ln and Col");
    }
}
