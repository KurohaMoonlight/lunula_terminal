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

        // Toggle Settings & Configuration Hub
        if crate::config::matches_shortcut(
            &self.config.keybinds.toggle_settings,
            self.ctrl_pressed,
            self.shift_pressed,
            self.alt_pressed,
            phys_code,
            logical_char,
        ) || (self.ctrl_pressed && (logical_char == Some(',') || phys_code == Some(KeyCode::Comma)))
        {
            self.toggle_settings();
            return;
        }

        // Settings Modal Navigation & Execution
        if self.settings_visible {
            if matches!(&key_event.logical_key, Key::Named(NamedKey::Escape))
                || phys_code == Some(KeyCode::Escape)
                || logical_char == Some('q')
                || logical_char == Some('Q')
            {
                self.settings_visible = false;
                self.window.request_redraw();
                return;
            }

            match phys_code {
                Some(KeyCode::ArrowDown | KeyCode::KeyJ) => {
                    self.settings_selected_index = (self.settings_selected_index + 1).min(5);
                    self.update_settings_buffer();
                    self.window.request_redraw();
                    return;
                }
                Some(KeyCode::ArrowUp | KeyCode::KeyK) => {
                    self.settings_selected_index = self.settings_selected_index.saturating_sub(1);
                    self.update_settings_buffer();
                    self.window.request_redraw();
                    return;
                }
                Some(KeyCode::Enter | KeyCode::NumpadEnter) => {
                    self.execute_settings_action(self.settings_selected_index);
                    return;
                }
                _ => {}
            }

            if let Some(ch @ '1'..='6') = logical_char {
                let idx = (ch as usize) - ('1' as usize);
                self.settings_selected_index = idx;
                self.execute_settings_action(idx);
                return;
            }

            return;
        }

        // Toggle Help Modal
        if crate::config::matches_shortcut(
            &self.config.keybinds.toggle_help,
            self.ctrl_pressed,
            self.shift_pressed,
            self.alt_pressed,
            phys_code,
            logical_char,
        ) || (self.shift_pressed
            && (matches!(&key_event.logical_key, Key::Named(NamedKey::F1))
                || phys_code == Some(KeyCode::F1)))
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
            let card_h = super::HELP_MODAL_HEIGHT.min(content_h - 24.0);
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

        // 1. Fullscreen toggle
        if crate::config::matches_shortcut(
            &self.config.keybinds.toggle_fullscreen,
            self.ctrl_pressed,
            self.shift_pressed,
            self.alt_pressed,
            phys_code,
            logical_char,
        ) || matches!(&key_event.logical_key, Key::Named(NamedKey::F11))
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

        // --- Dynamic Configurable Keybindings ---
        if crate::config::matches_shortcut(
            &self.config.keybinds.new_tab,
            self.ctrl_pressed,
            self.shift_pressed,
            self.alt_pressed,
            phys_code,
            logical_char,
        ) {
            let _ = self.tab_manager.new_tab(
                content_w,
                content_h,
                &mut self.text_engine,
                &self.config,
            );
            self.window.request_redraw();
            return;
        }

        if crate::config::matches_shortcut(
            &self.config.keybinds.close_pane,
            self.ctrl_pressed,
            self.shift_pressed,
            self.alt_pressed,
            phys_code,
            logical_char,
        ) {
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

        if crate::config::matches_shortcut(
            &self.config.keybinds.split_vertical,
            self.ctrl_pressed,
            self.shift_pressed,
            self.alt_pressed,
            phys_code,
            logical_char,
        ) {
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

        if crate::config::matches_shortcut(
            &self.config.keybinds.split_horizontal,
            self.ctrl_pressed,
            self.shift_pressed,
            self.alt_pressed,
            phys_code,
            logical_char,
        ) {
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

        if crate::config::matches_shortcut(
            &self.config.keybinds.next_tab,
            self.ctrl_pressed,
            self.shift_pressed,
            self.alt_pressed,
            phys_code,
            logical_char,
        ) {
            self.tab_manager.next_tab(
                content_w,
                content_h,
                &mut self.text_engine,
                &self.config,
            );
            self.window.request_redraw();
            return;
        }

        if crate::config::matches_shortcut(
            &self.config.keybinds.prev_tab,
            self.ctrl_pressed,
            self.shift_pressed,
            self.alt_pressed,
            phys_code,
            logical_char,
        ) {
            self.tab_manager.prev_tab(
                content_w,
                content_h,
                &mut self.text_engine,
                &self.config,
            );
            self.window.request_redraw();
            return;
        }

        if crate::config::matches_shortcut(
            &self.config.keybinds.search,
            self.ctrl_pressed,
            self.shift_pressed,
            self.alt_pressed,
            phys_code,
            logical_char,
        ) {
            if let Some(pane) = self.tab_manager.active_pane_mut() {
                if pane.vi_mode.is_none() {
                    let sb_len = pane.terminal.scrollback.len();
                    let cur_abs_row = sb_len + pane.terminal.cursor_y;
                    pane.vi_mode = Some(pane::ViMode::new(pane.terminal.cursor_x, cur_abs_row));
                }
                if let Some(vi) = &mut pane.vi_mode {
                    vi.in_search_input = true;
                    if !vi.search_query.is_empty() {
                        vi.next_match(&mut pane.terminal);
                    }
                }
                self.window.request_redraw();
                return;
            }
        }

        if crate::config::matches_shortcut(
            &self.config.keybinds.reload_config,
            self.ctrl_pressed,
            self.shift_pressed,
            self.alt_pressed,
            phys_code,
            logical_char,
        ) {
            self.reload_config();
            return;
        }

        if crate::config::matches_shortcut(
            &self.config.keybinds.reload_profile,
            self.ctrl_pressed,
            self.shift_pressed,
            self.alt_pressed,
            phys_code,
            logical_char,
        ) {
            if let Some(pane) = self.tab_manager.active_pane_mut() {
                pane.pty.write_bytes(b". $PROFILE\r\n");
                pane.terminal.scroll_to_bottom();
            }
            return;
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

            // Ctrl + Shift + Space: Toggle Alacritty-style Vi Mode
            if phys_code == Some(KeyCode::Space)
                || matches!(&key_event.logical_key, Key::Named(NamedKey::Space))
            {
                if let Some(pane) = self.tab_manager.active_pane_mut() {
                    if pane.vi_mode.is_some() {
                        pane.vi_mode = None;
                        pane.terminal.clear_selection();
                    } else {
                        let sb_len = pane.terminal.scrollback.len();
                        let cur_abs_row = sb_len + pane.terminal.cursor_y;
                        pane.vi_mode = Some(pane::ViMode::new(pane.terminal.cursor_x, cur_abs_row));
                    }
                    self.window.request_redraw();
                    return;
                }
            }

            // Ctrl + Shift + U: Scan & Toggle Regex Hints
            if phys_code == Some(KeyCode::KeyU)
                || matches!(logical_char, Some('u' | 'U' | '\x15'))
            {
                if let Some(pane) = self.tab_manager.active_pane_mut() {
                    if pane.vi_mode.is_none() {
                        let sb_len = pane.terminal.scrollback.len();
                        let cur_abs_row = sb_len + pane.terminal.cursor_y;
                        pane.vi_mode = Some(pane::ViMode::new(pane.terminal.cursor_x, cur_abs_row));
                    }
                    if let Some(vi) = &mut pane.vi_mode {
                        vi.scan_hints(&pane.terminal);
                    }
                    self.window.request_redraw();
                    return;
                }
            }

            // Ctrl + Shift + F: Quick Find / Scrollback Search
            if phys_code == Some(KeyCode::KeyF)
                || matches!(logical_char, Some('f' | 'F' | '\x06'))
            {
                if let Some(pane) = self.tab_manager.active_pane_mut() {
                    if pane.vi_mode.is_none() {
                        let sb_len = pane.terminal.scrollback.len();
                        let cur_abs_row = sb_len + pane.terminal.cursor_y;
                        pane.vi_mode = Some(pane::ViMode::new(pane.terminal.cursor_x, cur_abs_row));
                    }
                    if let Some(vi) = &mut pane.vi_mode {
                        vi.in_search_input = true;
                        if !vi.search_query.is_empty() {
                            vi.next_match(&mut pane.terminal);
                        }
                    }
                    self.window.request_redraw();
                    return;
                }
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

        // 6. Vi Mode handling (temporarily disables forwarding keystrokes to PTY)
        if let Some(pane) = self.tab_manager.active_pane_mut() {
            if let Some(vi) = &mut pane.vi_mode {
                // A. Search Input Mode
                if vi.in_search_input {
                    match &key_event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            vi.in_search_input = false;
                            pane.vi_mode = None;
                            pane.terminal.clear_selection();
                            self.window.request_redraw();
                            return;
                        }
                        Key::Named(NamedKey::Enter) => {
                            if self.shift_pressed {
                                vi.prev_match(&mut pane.terminal);
                            } else if vi.search_matches.is_empty() && !vi.search_query.is_empty() {
                                vi.execute_search(&mut pane.terminal);
                            } else {
                                vi.next_match(&mut pane.terminal);
                            }
                            self.window.request_redraw();
                            return;
                        }
                        Key::Named(NamedKey::Backspace) => {
                            vi.search_query.pop();
                            vi.execute_search(&mut pane.terminal);
                            self.window.request_redraw();
                            return;
                        }
                        _ => {
                            if let Some(c) = logical_char {
                                if !c.is_control() {
                                    vi.search_query.push(c);
                                    vi.execute_search(&mut pane.terminal);
                                    self.window.request_redraw();
                                    return;
                                }
                            }
                        }
                    }
                    return;
                }

                // B. Vi Mode Navigation & Commands (Normal, Visual, VisualLine)
                match &key_event.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        if vi.hints_active {
                            vi.hints_active = false;
                        } else if vi.kind != pane::ViModeKind::Normal {
                            vi.kind = pane::ViModeKind::Normal;
                            vi.anchor = None;
                            pane.terminal.clear_selection();
                        } else {
                            pane.vi_mode = None;
                            pane.terminal.clear_selection();
                        }
                        self.window.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::ArrowLeft) => {
                        vi.move_left(&mut pane.terminal);
                        self.window.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::ArrowRight) => {
                        vi.move_right(&mut pane.terminal);
                        self.window.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        vi.move_up(&mut pane.terminal);
                        self.window.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::ArrowDown) => {
                        vi.move_down(&mut pane.terminal);
                        self.window.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::PageUp) => {
                        for _ in 0..10 {
                            vi.move_up(&mut pane.terminal);
                        }
                        self.window.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::PageDown) => {
                        for _ in 0..10 {
                            vi.move_down(&mut pane.terminal);
                        }
                        self.window.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::Home) => {
                        vi.move_to_line_start(&mut pane.terminal);
                        self.window.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::End) => {
                        vi.move_to_line_end(&mut pane.terminal);
                        self.window.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::Enter) => {
                        if !vi.search_matches.is_empty() {
                            if self.shift_pressed {
                                vi.prev_match(&mut pane.terminal);
                            } else {
                                vi.next_match(&mut pane.terminal);
                            }
                            self.window.request_redraw();
                            return;
                        }
                        if let Some(url) =
                            pane.terminal.get_hyperlink_at(vi.cursor_col, vi.cursor_abs_row)
                        {
                            crate::platform::open_url_safely(&url);
                        }
                        self.window.request_redraw();
                        return;
                    }
                    _ => {}
                }

                if self.ctrl_pressed && (phys_code == Some(KeyCode::KeyC) || logical_char == Some('c') || logical_char == Some('\x03')) {
                    pane.vi_mode = None;
                    pane.terminal.clear_selection();
                    self.window.request_redraw();
                    return;
                }

                match logical_char {
                    Some('h') => vi.move_left(&mut pane.terminal),
                    Some('l') => vi.move_right(&mut pane.terminal),
                    Some('k') => vi.move_up(&mut pane.terminal),
                    Some('j') => vi.move_down(&mut pane.terminal),
                    Some('0') => vi.move_to_line_start(&mut pane.terminal),
                    Some('$') => vi.move_to_line_end(&mut pane.terminal),
                    Some('w') => vi.move_word_forward(&mut pane.terminal),
                    Some('b') => vi.move_word_backward(&mut pane.terminal),
                    Some('g') => vi.move_to_top(&mut pane.terminal),
                    Some('G') => vi.move_to_bottom(&mut pane.terminal),
                    Some('v') => vi.toggle_visual(&mut pane.terminal),
                    Some('V') => vi.toggle_visual_line(&mut pane.terminal),
                    Some('y') => {
                        if let Some(text) = pane.terminal.get_selected_text() {
                            clipboard::set_text(&text);
                        }
                        vi.kind = pane::ViModeKind::Normal;
                        vi.anchor = None;
                        pane.terminal.clear_selection();
                    }
                    Some('/') => {
                        vi.in_search_input = true;
                        vi.search_query.clear();
                    }
                    Some('n') => vi.next_match(&mut pane.terminal),
                    Some('N') => vi.prev_match(&mut pane.terminal),
                    Some('u' | 'o') => vi.scan_hints(&pane.terminal),
                    Some('i' | 'q') => {
                        pane.vi_mode = None;
                        pane.terminal.clear_selection();
                    }
                    _ => {}
                }

                self.window.request_redraw();
                return;
            }
        }

        // 7. Active pane terminal keystroke forwarding
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
                    let kitty_flags = pane.terminal.kitty_keyboard_flags;
                    let mut mods = 1u8;
                    if self.shift_pressed { mods += 1; }
                    if self.alt_pressed { mods += 2; }
                    if self.ctrl_pressed { mods += 4; }
                    if self.super_pressed { mods += 8; }

                    if kitty_flags != 0 {
                        match &key_event.logical_key {
                            Key::Named(NamedKey::Enter) => {
                                pane.terminal.clear_selection();
                                if mods > 1 || (kitty_flags & 8 != 0) {
                                    pane.pty.write_bytes(format!("\x1b[13;{}u", mods).as_bytes());
                                } else {
                                    pane.pty.write_bytes(b"\r");
                                }
                                self.drain_pty();
                                self.window.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Tab) => {
                                pane.terminal.clear_selection();
                                if mods > 1 || (kitty_flags & 8 != 0) {
                                    pane.pty.write_bytes(format!("\x1b[9;{}u", mods).as_bytes());
                                } else {
                                    pane.pty.write_bytes(b"\t");
                                }
                                self.drain_pty();
                                self.window.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Backspace) => {
                                pane.terminal.clear_selection();
                                if mods > 1 || (kitty_flags & 8 != 0) {
                                    pane.pty.write_bytes(format!("\x1b[127;{}u", mods).as_bytes());
                                } else {
                                    pane.pty.write_bytes(b"\x7f");
                                }
                                self.drain_pty();
                                self.window.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Escape) => {
                                pane.terminal.clear_selection();
                                if mods > 1 || (kitty_flags & 8 != 0) {
                                    pane.pty.write_bytes(format!("\x1b[27;{}u", mods).as_bytes());
                                } else {
                                    pane.pty.write_bytes(b"\x1b");
                                }
                                self.drain_pty();
                                self.window.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Space) => {
                                pane.terminal.clear_selection();
                                if mods > 1 || (kitty_flags & 8 != 0) {
                                    pane.pty.write_bytes(format!("\x1b[32;{}u", mods).as_bytes());
                                } else {
                                    pane.pty.write_bytes(b" ");
                                }
                                self.drain_pty();
                                self.window.request_redraw();
                                return;
                            }
                            Key::Character(text_str) => {
                                if mods > 1 {
                                    let mut handled = false;
                                    for ch in text_str.chars() {
                                        if (ch == 'c' || ch == 'C' || ch == '\x03') && self.ctrl_pressed && pane.terminal.has_selection() {
                                            if let Some(text) = pane.terminal.get_selected_text() {
                                                clipboard::set_text(&text);
                                            }
                                            pane.terminal.clear_selection();
                                            handled = true;
                                            break;
                                        }
                                        if (ch == 'v' || ch == 'V' || ch == '\x16') && self.ctrl_pressed && !self.alt_pressed {
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
                                            }
                                            handled = true;
                                            break;
                                        }
                                        let cp = if ch.is_ascii_uppercase() && self.shift_pressed {
                                            ch.to_ascii_lowercase() as u32
                                        } else {
                                            ch as u32
                                        };
                                        pane.pty.write_bytes(format!("\x1b[{};{}u", cp, mods).as_bytes());
                                        handled = true;
                                    }
                                    if handled {
                                        self.drain_pty();
                                        self.window.request_redraw();
                                        return;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }

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
                            } else if self.alt_pressed {
                                pane.pty.write_bytes(b"\x1b\x7f");
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
                            if mods > 1 {
                                pane.pty.write_bytes(format!("\x1b[1;{}A", mods).as_bytes());
                            } else if pane.terminal.app_cursor_keys {
                                pane.pty.write_bytes(b"\x1bOA");
                            } else {
                                pane.pty.write_bytes(b"\x1b[A");
                            }
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            if mods > 1 {
                                pane.pty.write_bytes(format!("\x1b[1;{}B", mods).as_bytes());
                            } else if pane.terminal.app_cursor_keys {
                                pane.pty.write_bytes(b"\x1bOB");
                            } else {
                                pane.pty.write_bytes(b"\x1b[B");
                            }
                        }
                        Key::Named(NamedKey::ArrowRight) => {
                            if mods > 1 {
                                pane.pty.write_bytes(format!("\x1b[1;{}C", mods).as_bytes());
                            } else if pane.terminal.app_cursor_keys {
                                pane.pty.write_bytes(b"\x1bOC");
                            } else {
                                pane.pty.write_bytes(b"\x1b[C");
                            }
                        }
                        Key::Named(NamedKey::ArrowLeft) => {
                            if mods > 1 {
                                pane.pty.write_bytes(format!("\x1b[1;{}D", mods).as_bytes());
                            } else if pane.terminal.app_cursor_keys {
                                pane.pty.write_bytes(b"\x1bOD");
                            } else {
                                pane.pty.write_bytes(b"\x1b[D");
                            }
                        }
                        Key::Named(NamedKey::Home) => {
                            if mods > 1 {
                                pane.pty.write_bytes(format!("\x1b[1;{}H", mods).as_bytes());
                            } else if pane.terminal.app_cursor_keys {
                                pane.pty.write_bytes(b"\x1bOH");
                            } else {
                                pane.pty.write_bytes(b"\x1b[H");
                            }
                        }
                        Key::Named(NamedKey::End) => {
                            if mods > 1 {
                                pane.pty.write_bytes(format!("\x1b[1;{}F", mods).as_bytes());
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
                                        'w' | 'W' | '\x17' => pane.pty.write_bytes(b"\x17"),
                                        _ => {
                                            if ch.is_ascii_alphabetic() {
                                                let b =
                                                    (ch.to_ascii_uppercase() as u8 - b'A') + 1;
                                                pane.pty.write_bytes(&[b]);
                                            }
                                        }
                                    }
                                }
                            } else if self.alt_pressed {
                                pane.terminal.clear_selection();
                                for ch in text_str.chars() {
                                    let mut buf = [0u8; 4];
                                    let s = ch.encode_utf8(&mut buf);
                                    pane.pty.write_bytes(b"\x1b");
                                    pane.pty.write_bytes(s.as_bytes());
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
        if !self.drain_pty() || self.should_exit {
            el.exit();
            return;
        }
        self.window.request_redraw();
    }
}
