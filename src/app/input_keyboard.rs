//! Keyboard shortcut routing, pane/tab navigation, and PTY keystroke forwarding.

use super::state::GpuState;
use crate::pane;
use crate::platform::clipboard;
use winit::event::KeyEvent;
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};

impl GpuState {
    pub fn handle_keyboard_input(&mut self, el: &ActiveEventLoop, key_event: KeyEvent) {
        if !key_event.state.is_pressed() {
            return;
        }

        self.check_background_metadata();

        let content_w = self.wobble.content_size[0] as f32;
        let content_h = self.wobble.content_size[1] as f32;

        let phys_code = match &key_event.physical_key {
            PhysicalKey::Code(code) => Some(*code),
            _ => None,
        };
        let logical_char = match &key_event.logical_key {
            Key::Character(text_str) => text_str.chars().next(),
            _ => None,
        };

        // F1 / KeyCode::F1: Toggle Help Modal
        if matches!(&key_event.logical_key, Key::Named(NamedKey::F1))
            || phys_code == Some(KeyCode::F1)
        {
            self.toggle_help();
            return;
        }

        // Escape: Dismiss Help if visible
        if (matches!(&key_event.logical_key, Key::Named(NamedKey::Escape))
            || phys_code == Some(KeyCode::Escape))
            && self.help_visible
        {
            self.help_visible = false;
            self.help_dragging_scrollbar = false;
            self.window.request_redraw();
            return;
        }

        if self.help_visible {
            let card_h = 580.0f32.min(content_h - 24.0);
            let visible_h = card_h - 32.0;
            let max_scroll = (self.help_total_height - visible_h).max(0.0);
            let cell_h = self.text_engine.cell_height;

            match phys_code {
                Some(KeyCode::ArrowDown) | Some(KeyCode::KeyJ) => {
                    self.help_scroll_y = (self.help_scroll_y + cell_h * 2.5).clamp(0.0, max_scroll);
                    self.window.request_redraw();
                    return;
                }
                Some(KeyCode::ArrowUp) | Some(KeyCode::KeyK) => {
                    self.help_scroll_y = (self.help_scroll_y - cell_h * 2.5).clamp(0.0, max_scroll);
                    self.window.request_redraw();
                    return;
                }
                Some(KeyCode::PageDown) | Some(KeyCode::Space) => {
                    self.help_scroll_y =
                        (self.help_scroll_y + visible_h * 0.75).clamp(0.0, max_scroll);
                    self.window.request_redraw();
                    return;
                }
                Some(KeyCode::PageUp) => {
                    self.help_scroll_y =
                        (self.help_scroll_y - visible_h * 0.75).clamp(0.0, max_scroll);
                    self.window.request_redraw();
                    return;
                }
                Some(KeyCode::Home) => {
                    self.help_scroll_y = 0.0;
                    self.window.request_redraw();
                    return;
                }
                Some(KeyCode::End) => {
                    self.help_scroll_y = max_scroll;
                    self.window.request_redraw();
                    return;
                }
                _ => {}
            }

            if let Some('q' | 'Q') = logical_char {
                self.help_visible = false;
                self.help_dragging_scrollbar = false;
                self.window.request_redraw();
                return;
            }

            if self.ctrl_pressed
                && self.shift_pressed
                && (phys_code == Some(KeyCode::KeyH)
                    || logical_char == Some('h')
                    || logical_char == Some('H')
                    || logical_char == Some('\x08'))
            {
                self.help_visible = false;
                self.help_dragging_scrollbar = false;
                self.window.request_redraw();
                return;
            }

            return;
        }

        // 1. F11 Fullscreen toggle
        if matches!(&key_event.logical_key, Key::Named(NamedKey::F11))
            || phys_code == Some(KeyCode::F11)
        {
            self.toggle_maximize();
            return;
        }

        // 2. Alt + Arrow keys: Navigate between split panes
        if self.alt_pressed {
            match &key_event.logical_key {
                Key::Named(NamedKey::ArrowLeft) => {
                    self.tab_manager.navigate_pane(pane::NavDirection::Left);
                    self.window.request_redraw();
                    return;
                }
                Key::Named(NamedKey::ArrowRight) => {
                    self.tab_manager.navigate_pane(pane::NavDirection::Right);
                    self.window.request_redraw();
                    return;
                }
                Key::Named(NamedKey::ArrowUp) => {
                    self.tab_manager.navigate_pane(pane::NavDirection::Up);
                    self.window.request_redraw();
                    return;
                }
                Key::Named(NamedKey::ArrowDown) => {
                    self.tab_manager.navigate_pane(pane::NavDirection::Down);
                    self.window.request_redraw();
                    return;
                }
                _ => {}
            }
        }

        // 3. Ctrl + Shift shortcuts: Tabs, Splits, Help & Config management
        if self.ctrl_pressed && self.shift_pressed {
            if phys_code == Some(KeyCode::KeyC)
                || matches!(logical_char, Some('c' | 'C' | '\x03'))
            {
                if let Some(pane) = self.tab_manager.active_pane_mut() {
                    if let Some(text) = pane.terminal.get_selected_text() {
                        clipboard::set_text(&text);
                    }
                    pane.terminal.clear_selection();
                    self.window.request_redraw();
                }
                return;
            }

            if phys_code == Some(KeyCode::KeyV)
                || matches!(logical_char, Some('v' | 'V' | '\x16'))
            {
                if let Some(pane) = self.tab_manager.active_pane_mut() {
                    if let Some(text) = clipboard::get_text() {
                        let normalized = text.replace("\r\n", "\r").replace('\n', "\r");
                        if pane.terminal.bracketed_paste {
                            pane.pty.write_bytes(b"\x1b[200~");
                            pane.pty.write_bytes(normalized.as_bytes());
                            pane.pty.write_bytes(b"\x1b[201~");
                        } else {
                            pane.pty.write_bytes(normalized.as_bytes());
                        }
                        pane.terminal.scroll_to_bottom();
                        self.drain_pty();
                        self.window.request_redraw();
                    }
                }
                return;
            }

            if phys_code == Some(KeyCode::KeyH)
                || matches!(logical_char, Some('h' | 'H' | '\x08' | '?' | '/'))
            {
                self.toggle_help();
                return;
            }

            if phys_code == Some(KeyCode::KeyW)
                || matches!(logical_char, Some('w' | 'W' | '\x17'))
            {
                let alive = self.tab_manager.close_active_pane(
                    content_w,
                    content_h,
                    &mut self.text_engine,
                    &self.config,
                );
                if !alive {
                    el.exit();
                } else {
                    self.window.request_redraw();
                }
                return;
            }

            if phys_code == Some(KeyCode::KeyT)
                || matches!(logical_char, Some('t' | 'T' | '\x14'))
            {
                let _ = self.tab_manager.new_tab(
                    content_w,
                    content_h,
                    &mut self.text_engine,
                    &self.config,
                );
                self.window.request_redraw();
                return;
            }

            if phys_code == Some(KeyCode::KeyE)
                || matches!(logical_char, Some('e' | 'E' | '\x05'))
            {
                let _ = self.tab_manager.split_active_pane(
                    pane::SplitDirection::Vertical,
                    content_w,
                    content_h,
                    &mut self.text_engine,
                    &self.config,
                );
                self.window.request_redraw();
                return;
            }

            if phys_code == Some(KeyCode::KeyD)
                || matches!(logical_char, Some('d' | 'D' | '\x04'))
            {
                let _ = self.tab_manager.split_active_pane(
                    pane::SplitDirection::Horizontal,
                    content_w,
                    content_h,
                    &mut self.text_engine,
                    &self.config,
                );
                self.window.request_redraw();
                return;
            }

            if phys_code == Some(KeyCode::KeyR)
                || matches!(logical_char, Some('r' | 'R' | '\x12'))
            {
                self.reload_config();
                return;
            }

            if phys_code == Some(KeyCode::KeyP)
                || matches!(logical_char, Some('p' | 'P' | '\x10'))
            {
                if let Some(pane) = self.tab_manager.active_pane_mut() {
                    pane.pty.write_bytes(b". $PROFILE\r\n");
                    pane.terminal.scroll_to_bottom();
                }
                return;
            }
        }

        // 4. Ctrl + W (without shift) when multiple split panes exist
        if self.ctrl_pressed && !self.shift_pressed {
            if (phys_code == Some(KeyCode::KeyW)
                || matches!(logical_char, Some('w' | 'W' | '\x17')))
                && self.tab_manager.active_tab().root.all_leaves().len() > 1
            {
                self.tab_manager.close_active_pane(
                    content_w,
                    content_h,
                    &mut self.text_engine,
                    &self.config,
                );
                self.window.request_redraw();
                return;
            }
        }

        // 5. Ctrl + Tab / Ctrl + 1..9: Tab switching
        if self.ctrl_pressed {
            match &key_event.logical_key {
                Key::Named(NamedKey::Tab) => {
                    if self.shift_pressed {
                        self.tab_manager.prev_tab(
                            content_w,
                            content_h,
                            &mut self.text_engine,
                            &self.config,
                        );
                    } else {
                        self.tab_manager.next_tab(
                            content_w,
                            content_h,
                            &mut self.text_engine,
                            &self.config,
                        );
                    }
                    self.window.request_redraw();
                    return;
                }
                Key::Named(NamedKey::PageDown) => {
                    self.tab_manager.next_tab(
                        content_w,
                        content_h,
                        &mut self.text_engine,
                        &self.config,
                    );
                    self.window.request_redraw();
                    return;
                }
                Key::Named(NamedKey::PageUp) => {
                    self.tab_manager.prev_tab(
                        content_w,
                        content_h,
                        &mut self.text_engine,
                        &self.config,
                    );
                    self.window.request_redraw();
                    return;
                }
                Key::Character(s_str) => {
                    if let Some(d) = s_str.chars().next().and_then(|c| c.to_digit(10)) {
                        if (1..=9).contains(&d) {
                            self.tab_manager.switch_tab(
                                (d - 1) as usize,
                                content_w,
                                content_h,
                                &mut self.text_engine,
                                &self.config,
                            );
                            self.window.request_redraw();
                            return;
                        }
                    }
                }
                _ => {}
            }
        }

        // 6. Active pane terminal keystroke forwarding
        if let Some(pane) = self.tab_manager.active_pane_mut() {
            match &key_event.logical_key {
                Key::Named(NamedKey::PageUp) => {
                    if self.shift_pressed {
                        pane.terminal.scroll_up_view(pane.terminal.rows / 2);
                    } else {
                        pane.pty.write_bytes(b"\x1b[5~");
                    }
                }
                Key::Named(NamedKey::PageDown) => {
                    if self.shift_pressed {
                        pane.terminal.scroll_down_view(pane.terminal.rows / 2);
                    } else {
                        pane.pty.write_bytes(b"\x1b[6~");
                    }
                }
                Key::Named(NamedKey::Home) if self.shift_pressed => {
                    pane.terminal.scroll_up_view(usize::MAX);
                }
                Key::Named(NamedKey::End) if self.shift_pressed => {
                    pane.terminal.scroll_to_bottom();
                }
                _ => {
                    pane.terminal.scroll_to_bottom();
                    match &key_event.logical_key {
                        Key::Named(NamedKey::Space) => {
                            pane.terminal.clear_selection();
                            pane.pty.write_bytes(b" ");
                        }
                        Key::Named(NamedKey::Enter) => {
                            pane.terminal.clear_selection();
                            pane.pty.write_bytes(b"\r");
                        }
                        Key::Named(NamedKey::Backspace) => {
                            pane.terminal.clear_selection();
                            if self.ctrl_pressed {
                                pane.pty.write_bytes(b"\x08");
                            } else {
                                pane.pty.write_bytes(b"\x7f");
                            }
                        }
                        Key::Named(NamedKey::Tab) => {
                            pane.terminal.clear_selection();
                            pane.pty.write_bytes(b"\t");
                        }
                        Key::Named(NamedKey::Escape) => {
                            pane.terminal.clear_selection();
                            pane.pty.write_bytes(b"\x1b");
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            if self.ctrl_pressed {
                                pane.pty.write_bytes(b"\x1b[1;5A");
                            } else if pane.terminal.app_cursor_keys {
                                pane.pty.write_bytes(b"\x1bOA");
                            } else {
                                pane.pty.write_bytes(b"\x1b[A");
                            }
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            if self.ctrl_pressed {
                                pane.pty.write_bytes(b"\x1b[1;5B");
                            } else if pane.terminal.app_cursor_keys {
                                pane.pty.write_bytes(b"\x1bOB");
                            } else {
                                pane.pty.write_bytes(b"\x1b[B");
                            }
                        }
                        Key::Named(NamedKey::ArrowRight) => {
                            if self.ctrl_pressed {
                                pane.pty.write_bytes(b"\x1b[1;5C");
                            } else if pane.terminal.app_cursor_keys {
                                pane.pty.write_bytes(b"\x1bOC");
                            } else {
                                pane.pty.write_bytes(b"\x1b[C");
                            }
                        }
                        Key::Named(NamedKey::ArrowLeft) => {
                            if self.ctrl_pressed {
                                pane.pty.write_bytes(b"\x1b[1;5D");
                            } else if pane.terminal.app_cursor_keys {
                                pane.pty.write_bytes(b"\x1bOD");
                            } else {
                                pane.pty.write_bytes(b"\x1b[D");
                            }
                        }
                        Key::Named(NamedKey::Home) => {
                            if self.ctrl_pressed {
                                pane.pty.write_bytes(b"\x1b[1;5H");
                            } else if pane.terminal.app_cursor_keys {
                                pane.pty.write_bytes(b"\x1bOH");
                            } else {
                                pane.pty.write_bytes(b"\x1b[H");
                            }
                        }
                        Key::Named(NamedKey::End) => {
                            if self.ctrl_pressed {
                                pane.pty.write_bytes(b"\x1b[1;5F");
                            } else if pane.terminal.app_cursor_keys {
                                pane.pty.write_bytes(b"\x1bOF");
                            } else {
                                pane.pty.write_bytes(b"\x1b[F");
                            }
                        }
                        Key::Named(NamedKey::F1) => {
                            pane.pty.write_bytes(b"\x1bOP");
                        }
                        Key::Named(NamedKey::F2) => {
                            pane.pty.write_bytes(b"\x1bOQ");
                        }
                        Key::Named(NamedKey::F3) => {
                            pane.pty.write_bytes(b"\x1bOR");
                        }
                        Key::Named(NamedKey::F4) => {
                            pane.pty.write_bytes(b"\x1bOS");
                        }
                        Key::Named(NamedKey::F5) => {
                            pane.pty.write_bytes(b"\x1b[15~");
                        }
                        Key::Named(NamedKey::F6) => {
                            pane.pty.write_bytes(b"\x1b[17~");
                        }
                        Key::Named(NamedKey::F7) => {
                            pane.pty.write_bytes(b"\x1b[18~");
                        }
                        Key::Named(NamedKey::F8) => {
                            pane.pty.write_bytes(b"\x1b[19~");
                        }
                        Key::Named(NamedKey::F9) => {
                            pane.pty.write_bytes(b"\x1b[20~");
                        }
                        Key::Named(NamedKey::F10) => {
                            pane.pty.write_bytes(b"\x1b[21~");
                        }
                        Key::Named(NamedKey::F11) => {
                            pane.pty.write_bytes(b"\x1b[23~");
                        }
                        Key::Named(NamedKey::F12) => {
                            pane.pty.write_bytes(b"\x1b[24~");
                        }
                        Key::Named(NamedKey::Delete) => {
                            pane.pty.write_bytes(b"\x1b[3~");
                        }
                        Key::Named(NamedKey::Insert) => {
                            if self.shift_pressed {
                                if let Some(text) = clipboard::get_text() {
                                    let normalized =
                                        text.replace("\r\n", "\r").replace('\n', "\r");
                                    if pane.terminal.bracketed_paste {
                                        pane.pty.write_bytes(b"\x1b[200~");
                                        pane.pty.write_bytes(normalized.as_bytes());
                                        pane.pty.write_bytes(b"\x1b[201~");
                                    } else {
                                        pane.pty.write_bytes(normalized.as_bytes());
                                    }
                                    pane.terminal.scroll_to_bottom();
                                }
                            } else {
                                pane.pty.write_bytes(b"\x1b[2~");
                            }
                        }
                        Key::Character(text_str) => {
                            if self.ctrl_pressed {
                                for ch in text_str.chars() {
                                    match ch {
                                        'c' | 'C' | '\x03' => {
                                            if pane.terminal.has_selection() {
                                                if let Some(text) =
                                                    pane.terminal.get_selected_text()
                                                {
                                                    clipboard::set_text(&text);
                                                }
                                                pane.terminal.clear_selection();
                                            } else {
                                                pane.pty.write_bytes(b"\x03");
                                            }
                                        }
                                        'v' | 'V' | '\x16' => {
                                            if let Some(text) = clipboard::get_text() {
                                                let normalized = text
                                                    .replace("\r\n", "\r")
                                                    .replace('\n', "\r");
                                                if pane.terminal.bracketed_paste {
                                                    pane.pty.write_bytes(b"\x1b[200~");
                                                    pane.pty.write_bytes(normalized.as_bytes());
                                                    pane.pty.write_bytes(b"\x1b[201~");
                                                } else {
                                                    pane.pty.write_bytes(normalized.as_bytes());
                                                }
                                                pane.terminal.scroll_to_bottom();
                                            }
                                        }
                                        'l' | 'L' | '\x0c' => pane.pty.write_bytes(b"\x0c"),
                                        'd' | 'D' | '\x04' => pane.pty.write_bytes(b"\x04"),
                                        'a' | 'A' | '\x01' => pane.pty.write_bytes(b"\x01"),
                                        'z' | 'Z' | '\x1a' => pane.pty.write_bytes(b"\x1a"),
                                        _ => {
                                            if ch.is_ascii_alphabetic() {
                                                let b =
                                                    (ch.to_ascii_uppercase() as u8 - b'A') + 1;
                                                pane.pty.write_bytes(&[b]);
                                            }
                                        }
                                    }
                                }
                            } else {
                                pane.terminal.clear_selection();
                                pane.pty.write_bytes(text_str.as_bytes());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Non-blocking drain; if ConPTY echoes asynchronously, `conpty-reader` wakes the event loop immediately
        self.drain_pty();
        self.window.request_redraw();
    }
}
