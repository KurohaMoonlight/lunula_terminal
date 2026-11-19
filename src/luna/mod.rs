//! Core `luna` micro-editor module: lightweight, zero-dependency, nano-style terminal editor.

pub mod buffer;
pub mod console;
pub mod syntax;
pub mod ui;

use buffer::TextBuffer;
use console::{ConsoleGuard, InputEvent};
use std::io;
use ui::PromptMode;

pub fn run() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let target_file = args.get(1).map(|s| s.as_str());

    let theme = crate::theme::Theme::load_from_config();
    let mut buf = TextBuffer::open(target_file)?;
    let guard = ConsoleGuard::enter()?;
    let (mut cols, mut rows) = guard.size();

    let mut status_msg = match &buf.path {
        Some(p) if p.exists() => format!("Opened {} ({} lines)", buf.filename_display(), buf.lines.len()),
        Some(_) => format!("New file: {}", buf.filename_display()),
        None => "New buffer".to_string(),
    };
    let mut prompt = PromptMode::None;

    loop {
        let view_rows = rows.saturating_sub(3).max(1);
        let gutter_w = buf.lines.len().to_string().len().max(3) + 3;
        let text_cols = cols.saturating_sub(gutter_w).max(10);
        buf.adjust_viewport(view_rows, text_cols);

        ui::render(&buf, cols, rows, &status_msg, &prompt, &theme)?;

        match guard.read_event()? {
            InputEvent::Resize(c, r) => {
                cols = c;
                rows = r;
            }
            ev => match &mut prompt {
                PromptMode::ConfirmExit => match ev {
                    InputEvent::Char('y' | 'Y') => {
                        buf.save()?;
                        break;
                    }
                    InputEvent::Char('n' | 'N') => break,
                    InputEvent::Esc | InputEvent::Ctrl('c') => {
                        prompt = PromptMode::None;
                        status_msg = "Cancelled".to_string();
                    }
                    _ => {}
                },
                PromptMode::Search(query) => match ev {
                    InputEvent::Enter => {
                        let q = query.clone();
                        prompt = PromptMode::None;
                        if buf.find_next(&q) {
                            status_msg = format!("Found '{}'", q);
                        } else {
                            status_msg = format!("'{}' not found", q);
                        }
                    }
                    InputEvent::Esc | InputEvent::Ctrl('c') => {
                        prompt = PromptMode::None;
                        status_msg = "Search cancelled".to_string();
                    }
                    InputEvent::Backspace => {
                        query.pop();
                    }
                    InputEvent::Char(c) => {
                        query.push(c);
                    }
                    _ => {}
                },
                PromptMode::None => match ev {
                    InputEvent::Ctrl('x') => {
                        if buf.dirty {
                            prompt = PromptMode::ConfirmExit;
                        } else {
                            break;
                        }
                    }
                    InputEvent::Ctrl('s') => match buf.save() {
                        Ok(bytes) => {
                            status_msg = format!("Saved {} ({} bytes)", buf.filename_display(), bytes);
                        }
                        Err(e) => {
                            status_msg = format!("Save error: {}", e);
                        }
                    },
                    InputEvent::Ctrl('f') => {
                        prompt = PromptMode::Search(String::new());
                    }
                    InputEvent::Ctrl('z') => {
                        if buf.undo() {
                            status_msg = "Undo".to_string();
                        } else {
                            status_msg = "Nothing to undo".to_string();
                        }
                    }
                    InputEvent::Up => buf.move_up(),
                    InputEvent::Down => buf.move_down(),
                    InputEvent::Left => buf.move_left(),
                    InputEvent::Right => buf.move_right(),
                    InputEvent::Home => buf.move_home(),
                    InputEvent::End => buf.move_end(),
                    InputEvent::PageUp => buf.page_up(view_rows),
                    InputEvent::PageDown => buf.page_down(view_rows),
                    InputEvent::Enter => buf.insert_newline(),
                    InputEvent::Backspace => buf.backspace(),
                    InputEvent::Delete => buf.delete(),
                    InputEvent::Tab => buf.insert_tab(),
                    InputEvent::Char(c) => buf.insert_char(c),
                    _ => {}
                },
            },
        }
    }

    Ok(())
}
