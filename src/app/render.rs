//! Frame rendering pipeline, status bar shaping cache, and decoupled idle blink timer.

use super::state::GpuState;
use crate::pane;
use crate::platform::get_local_time_hm;
use crate::terminal;
use std::time::{Duration, Instant};

impl GpuState {
    pub fn render(&mut self) -> bool {
        if self.should_exit {
            return false;
        }

        let now = Instant::now();
        let dt = (now - self.last_frame_time).as_secs_f32().clamp(0.001, 0.05);
        self.last_frame_time = now;

        // 1. Update wobble physics (early-exits in O(1) when stationary)
        let wobble_active = self.wobble.update_physics();

        let content_w = self.wobble.content_size[0] as f32;
        let content_h = self.wobble.content_size[1] as f32;
        let content_mouse = self
            .wobble
            .screen_to_content_pos(self.mouse_pos[0], self.mouse_pos[1])
            .unwrap_or([-1000.0, -1000.0]);
        let cell_w = self.text_engine.cell_width;
        let cell_h = self.text_engine.cell_height;

        // 2. Drain PTY inputs across all tabs and sync visible caches
        if !self.drain_pty() || self.tab_manager.tabs.is_empty() {
            return false;
        }

        // 3. Update physics & sub-pixel smooth scrolling for active tab panes
        let active_tab = self.tab_manager.active_tab();
        let active_pane_id = active_tab.active_pane_id;
        let visible_leaves = active_tab.root.all_leaves();

        let mut high_fps_animating = self.wobble.is_dragging || wobble_active;
        let mut needs_blink_timer = self.config.window.animate_titlebar;

        for &id in &visible_leaves {
            if let Some(pane) = self.tab_manager.panes.get_mut(&id) {
                if self.config.scroll.smooth {
                    if pane.terminal.update_smooth_scroll(dt) {
                        let _ = pane.sync_render_cache(&mut self.text_engine);
                        high_fps_animating = true;
                    }
                }
                pane.update_physics(dt, &self.config, cell_w, cell_h);
                let target_x = pane.rect[0] + pane::PANE_PAD + pane.terminal.cursor_x as f32 * cell_w;
                let target_y = pane.rect[1] + pane::PANE_PAD + pane.terminal.cursor_y as f32 * cell_h;
                let head_dist = (pane.cursor_head[0] - target_x).hypot(pane.cursor_head[1] - target_y);
                let tail_dist = (pane.cursor_head[0] - pane.cursor_tail[0]).abs();
                if head_dist > 0.4 || tail_dist > 0.4 {
                    high_fps_animating = true;
                }
            }
        }

        // 4. Assemble all GPU Background Quads & Decorations
        let mut bg_rects = Vec::with_capacity(256);
        let tab_bar_visible = self.tab_manager.tab_bar_visible(&self.config);
        let tab_bar_h = if tab_bar_visible {
            self.config.tabs.height
        } else {
            0.0
        };

        // 4a. Update status bar clock only when minute changes
        let time_str = get_local_time_hm();
        if time_str != self.last_time_str {
            self.last_time_str = time_str.clone();
            let clock_spans = vec![(
                time_str,
                terminal::CellStyle::new()
                    .with_fg(terminal::ColorRgb { r: 200, g: 192, b: 147 }) // oldWhite
                    .with_bold(true),
            )];
            self.text_engine
                .shape_buffer(&mut self.bar_clock_buffer, &clock_spans);
            self.bar_clock_width = crate::render::TextEngine::measure_buffer_width(&self.bar_clock_buffer);
        }

        // Shape Gear & Help icons only at startup or font change
        if self.bar_icons_dirty {
            let gear_spans = vec![(
                "⚙".to_string(),
                terminal::CellStyle::new()
                    .with_fg(terminal::ColorRgb { r: 122, g: 168, b: 159 }) // waveAqua2
                    .with_bold(true),
            )];
            self.text_engine
                .shape_buffer(&mut self.bar_gear_buffer, &gear_spans);
            self.bar_gear_width = crate::render::TextEngine::measure_buffer_width(&self.bar_gear_buffer);

            let help_spans = vec![(
                "?".to_string(),
                terminal::CellStyle::new()
                    .with_fg(terminal::ColorRgb { r: 230, g: 195, b: 132 }) // boatYellow2
                    .with_bold(true),
            )];
            self.text_engine
                .shape_buffer(&mut self.bar_help_buffer, &help_spans);
            self.bar_help_width = crate::render::TextEngine::measure_buffer_width(&self.bar_help_buffer);
            self.bar_icons_dirty = false;
        }

        // 4b. Shape center buffer ONLY when dirty (eliminates per-frame HarfBuzz shaping)
        if self.bar_center_dirty {
            let center_spans = vec![
                (
                    format!("{} ", self.cached_username),
                    terminal::CellStyle::new()
                        .with_fg(terminal::ColorRgb { r: 220, g: 215, b: 186 }) // fujiWhite
                        .with_bold(true),
                ),
                (
                    ":".to_string(),
                    terminal::CellStyle::new()
                        .with_fg(terminal::ColorRgb { r: 114, g: 113, b: 105 }), // fujiGray
                ),
                (
                    format!("  {}", self.cached_git_branch),
                    terminal::CellStyle::new()
                        .with_fg(terminal::ColorRgb { r: 152, g: 187, b: 108 }) // springGreen
                        .with_bold(true),
                ),
            ];
            self.text_engine
                .shape_buffer(&mut self.bar_center_buffer, &center_spans);
            self.bar_center_width = crate::render::TextEngine::measure_buffer_width(&self.bar_center_buffer);
            self.bar_center_dirty = false;
        }

        // Kanagawa Wave titlebar background is rendered at the lowest z-index in wobble.wgsl.
        // We render the floating frosted sumi glass pills and buttons over it:

        let center_pill_w = (self.bar_center_width + 28.0).clamp(120.0, 420.0);
        let center_pill_x = ((content_w - center_pill_w) * 0.5).round();
        let show_center_pill = center_pill_x > 94.0 && center_pill_x + center_pill_w < content_w - 166.0;
        if show_center_pill {
            bg_rects.push(terminal::BgRect {
                x: center_pill_x,
                y: 10.0,
                w: center_pill_w,
                h: 28.0,
                color: terminal::ColorRgb { r: 31, g: 31, b: 40 }, // sumiInk3
                alpha: 0.55, // Smoked-ink glass lets the wave shine through
            });
            bg_rects.push(terminal::BgRect {
                x: center_pill_x,
                y: 37.0,
                w: center_pill_w,
                h: 1.0,
                color: terminal::ColorRgb { r: 45, g: 79, b: 103 }, // waveBlue2 hairline
                alpha: 0.55,
            });
        }

        let clock_w = (self.bar_clock_width + 20.0).max(58.0).round();
        let clock_x = (content_w - 82.0 - clock_w).round();
        bg_rects.push(terminal::BgRect {
            x: clock_x,
            y: 10.0,
            w: clock_w,
            h: 28.0,
            color: terminal::ColorRgb { r: 31, g: 31, b: 40 }, // sumiInk3
            alpha: 0.65,
        });

        let is_gear_hover = content_mouse[0] >= content_w - 74.0
            && content_mouse[0] <= content_w - 42.0
            && content_mouse[1] >= 9.0
            && content_mouse[1] <= 39.0;
        bg_rects.push(terminal::BgRect {
            x: content_w - 74.0,
            y: 10.0,
            w: 32.0,
            h: 28.0,
            color: if is_gear_hover {
                terminal::ColorRgb { r: 45, g: 79, b: 103 } // waveBlue2
            } else {
                terminal::ColorRgb { r: 31, g: 31, b: 40 } // sumiInk3
            },
            alpha: if is_gear_hover { 0.90 } else { 0.55 },
        });

        let is_help_hover = content_mouse[0] >= content_w - 38.0
            && content_mouse[0] <= content_w - 6.0
            && content_mouse[1] >= 9.0
            && content_mouse[1] <= 39.0;
        bg_rects.push(terminal::BgRect {
            x: content_w - 38.0,
            y: 10.0,
            w: 32.0,
            h: 28.0,
            color: if is_help_hover {
                terminal::ColorRgb { r: 45, g: 79, b: 103 } // waveBlue2
            } else {
                terminal::ColorRgb { r: 31, g: 31, b: 40 } // sumiInk3
            },
            alpha: if is_help_hover { 0.90 } else { 0.55 },
        });

        // 4c. Tab Bar Background & Tabs
        let titlebar_h = 48.0;
        if tab_bar_visible {
            let tab_bar_y = titlebar_h;
            bg_rects.push(terminal::BgRect {
                x: 0.0,
                y: tab_bar_y,
                w: content_w,
                h: tab_bar_h,
                color: terminal::ColorRgb {
                    r: self.theme.tab_bar.r,
                    g: self.theme.tab_bar.g,
                    b: self.theme.tab_bar.b,
                },
                alpha: 0.95,
            });
            bg_rects.push(terminal::BgRect {
                x: 0.0,
                y: tab_bar_y + tab_bar_h - 1.0,
                w: content_w,
                h: 1.0,
                color: terminal::ColorRgb {
                    r: self.theme.border.r,
                    g: self.theme.border.g,
                    b: self.theme.border.b,
                },
                alpha: 0.70,
            });

            let mut pill_x = 10.0;
            for (idx, t) in self.tab_manager.tabs.iter().enumerate() {
                let is_active = idx == self.tab_manager.active_tab_index;
                let pill_w = t.width;
                let is_hover = content_mouse[0] >= pill_x
                    && content_mouse[0] <= pill_x + pill_w
                    && content_mouse[1] >= tab_bar_y + 3.0
                    && content_mouse[1] <= tab_bar_y + tab_bar_h - 3.0;

                let (bg_col, bg_alpha) = if is_active {
                    (
                        terminal::ColorRgb {
                            r: self.theme.selection.r,
                            g: self.theme.selection.g,
                            b: self.theme.selection.b,
                        },
                        0.95,
                    )
                } else if is_hover {
                    (
                        terminal::ColorRgb {
                            r: self.theme.tab_active.r,
                            g: self.theme.tab_active.g,
                            b: self.theme.tab_active.b,
                        },
                        0.80,
                    )
                } else {
                    (
                        terminal::ColorRgb {
                            r: self.theme.tab_bar.r,
                            g: self.theme.tab_bar.g,
                            b: self.theme.tab_bar.b,
                        },
                        0.50,
                    )
                };

                // Sleek pill background
                bg_rects.push(terminal::BgRect {
                    x: pill_x,
                    y: tab_bar_y + 3.0,
                    w: pill_w,
                    h: tab_bar_h - 6.0,
                    color: bg_col,
                    alpha: bg_alpha,
                });

                // Active bottom highlight indicator (sleek 2px indicator using theme cursor color)
                if is_active {
                    bg_rects.push(terminal::BgRect {
                        x: pill_x + 2.0,
                        y: tab_bar_y + tab_bar_h - 3.0,
                        w: pill_w - 4.0,
                        h: 2.0,
                        color: terminal::ColorRgb {
                            r: self.theme.cursor.r,
                            g: self.theme.cursor.g,
                            b: self.theme.cursor.b,
                        },
                        alpha: 1.0,
                    });
                }
                pill_x += pill_w + 4.0;
            }

            let plus_w = 26.0;
            let is_plus_hover = content_mouse[0] >= pill_x
                && content_mouse[0] <= pill_x + plus_w
                && content_mouse[1] >= tab_bar_y + 3.0
                && content_mouse[1] <= tab_bar_y + tab_bar_h - 3.0;

            bg_rects.push(terminal::BgRect {
                x: pill_x,
                y: tab_bar_y + 3.0,
                w: plus_w,
                h: tab_bar_h - 6.0,
                color: if is_plus_hover {
                    terminal::ColorRgb {
                        r: self.theme.selection.r,
                        g: self.theme.selection.g,
                        b: self.theme.selection.b,
                    }
                } else {
                    terminal::ColorRgb {
                        r: self.theme.tab_active.r,
                        g: self.theme.tab_active.g,
                        b: self.theme.tab_active.b,
                    }
                },
                alpha: if is_plus_hover { 0.90 } else { 0.45 },
            });
        }

        // Terminal Panes Body Background (uses active theme bg)
        let body_y = if tab_bar_visible { titlebar_h + tab_bar_h } else { titlebar_h };
        bg_rects.push(terminal::BgRect {
            x: 0.0,
            y: body_y,
            w: content_w,
            h: (content_h - body_y).max(0.0),
            color: terminal::ColorRgb {
                r: self.theme.bg.r,
                g: self.theme.bg.g,
                b: self.theme.bg.b,
            },
            alpha: self.config.window.opacity,
        });

        // 4d. Visible Panes Backgrounds (with sub-pixel scroll offset), Dimming, Cursor & Trail
        for &id in &visible_leaves {
            if let Some(pane) = self.tab_manager.panes.get(&id) {
                let px = pane.rect[0];
                let py = pane.rect[1];
                let pw = pane.rect[2];
                let ph = pane.rect[3];
                let sub_y = pane.scroll_subpixel_y;
                let is_active = id == active_pane_id;

                for rect in &pane.cached_bg_rects {
                    let rx = px + rect.x;
                    let ry = py + rect.y - sub_y;
                    if rx < px + pw && ry + rect.h > py && ry < py + ph {
                        let clipped_y = ry.max(py);
                        let clipped_h = (ry + rect.h).min(py + ph) - clipped_y;
                        if clipped_h > 0.0 {
                            bg_rects.push(terminal::BgRect {
                                x: rx,
                                y: clipped_y,
                                w: rect.w.min(px + pw - rx),
                                h: clipped_h,
                                color: rect.color,
                                alpha: rect.alpha,
                            });
                        }
                    }
                }

                if visible_leaves.len() > 1 {
                    let btn_x = px + pw - 24.0;
                    let btn_y = py + 4.0;
                    let is_hovered = content_mouse[0] >= btn_x
                        && content_mouse[0] <= btn_x + 20.0
                        && content_mouse[1] >= btn_y
                        && content_mouse[1] <= btn_y + 20.0;
                    bg_rects.push(terminal::BgRect {
                        x: btn_x,
                        y: btn_y,
                        w: 20.0,
                        h: 20.0,
                        color: if is_hovered {
                            terminal::ColorRgb { r: 195, g: 64, b: 67 } // Kanagawa autumnRed
                        } else {
                            terminal::ColorRgb { r: 42, g: 42, b: 55 } // Kanagawa sumiInk4
                        },
                        alpha: if is_hovered { 0.95 } else { 0.65 },
                    });
                    bg_rects.push(terminal::BgRect {
                        x: btn_x + 5.0,
                        y: btn_y + 9.0,
                        w: 10.0,
                        h: 2.0,
                        color: terminal::ColorRgb { r: 220, g: 215, b: 186 }, // Kanagawa fujiWhite
                        alpha: 0.90,
                    });
                }

                if !is_active && self.config.tabs.dim_inactive_panes {
                    bg_rects.push(terminal::BgRect {
                        x: px,
                        y: py,
                        w: pw,
                        h: ph,
                        color: terminal::ColorRgb { r: 22, g: 22, b: 29 }, // Kanagawa sumiInk0
                        alpha: self.config.tabs.dim_opacity,
                    });
                }

                if is_active
                    && pane.vi_mode.is_none()
                    && pane.terminal.cursor_visible
                    && pane.terminal.current_scroll_y <= 5.0
                {
                    let cur_color = pane.terminal.cursor_config.color;
                    let dist = (pane.cursor_head[0] - pane.cursor_tail[0]).abs();

                    if dist > 1.5 && self.config.cursor.trail {
                        let min_x = pane.cursor_tail[0].min(pane.cursor_head[0]);
                        let max_x = pane.cursor_tail[0].max(pane.cursor_head[0]) + cell_w;
                        let min_y = pane.cursor_head[1];

                        bg_rects.push(terminal::BgRect {
                            x: min_x,
                            y: min_y,
                            w: (max_x - min_x).max(cell_w),
                            h: cell_h,
                            color: cur_color,
                            alpha: 0.28,
                        });
                    }

                    let is_moving =
                        now.duration_since(pane.cursor_last_moved).as_secs_f32() < 0.45;
                    let cursor_alpha = if is_moving || !self.config.cursor.blinking || !self.is_focused {
                        1.0
                    } else {
                        needs_blink_timer = true;
                        0.35 + 0.65 * (0.5 + 0.5 * (pane.cursor_blink_time * 5.0).sin())
                    };

                    match pane.terminal.cursor_config.shape {
                        terminal::CursorShape::Block => {
                            bg_rects.push(terminal::BgRect {
                                x: pane.cursor_head[0],
                                y: pane.cursor_head[1],
                                w: cell_w,
                                h: cell_h,
                                color: cur_color,
                                alpha: cursor_alpha,
                            });
                        }
                        terminal::CursorShape::Beam => {
                            bg_rects.push(terminal::BgRect {
                                x: pane.cursor_head[0],
                                y: pane.cursor_head[1],
                                w: 2.5,
                                h: cell_h,
                                color: cur_color,
                                alpha: cursor_alpha,
                            });
                        }
                        terminal::CursorShape::Underline => {
                            bg_rects.push(terminal::BgRect {
                                x: pane.cursor_head[0],
                                y: pane.cursor_head[1] + cell_h - 2.5,
                                w: cell_w,
                                h: 2.5,
                                color: cur_color,
                                alpha: cursor_alpha,
                            });
                        }
                    }

                    if id == active_pane_id {
                        self.window.set_ime_cursor_area(
                            winit::dpi::PhysicalPosition::new(
                                pane.cursor_head[0] as i32,
                                pane.cursor_head[1] as i32,
                            ),
                            winit::dpi::PhysicalSize::new(cell_w.max(1.0) as u32, cell_h.max(1.0) as u32),
                        );
                    }
                }

                // Vi Mode highlights & navigation overlays via pure GPU BgRects
                if let Some(vi) = &pane.vi_mode {
                    let sb_len = pane.terminal.scrollback.len();
                    let total_rows = sb_len + pane.terminal.rows;
                    let base_offset = (pane.terminal.current_scroll_y / cell_h.max(1.0)).floor() as usize;
                    let effective_base = if pane.terminal.current_scroll_y > 0.0 {
                        base_offset
                    } else {
                        pane.terminal.scroll_offset
                    };
                    let start_abs = total_rows.saturating_sub(effective_base + pane.terminal.rows);
                    let end_abs = start_abs + pane.terminal.rows;

                    // 1. Search Matches Highlights (Pure GPU BgRect)
                    for (idx, m) in vi.search_matches.iter().enumerate() {
                        if m.point.abs_row >= start_abs && m.point.abs_row < end_abs {
                            let vis_r = m.point.abs_row - start_abs;
                            let rx = px + pane::PANE_PAD + m.point.col as f32 * cell_w;
                            let ry = py + pane::PANE_PAD + vis_r as f32 * cell_h - sub_y;
                            let rw = m.len as f32 * cell_w;
                            let is_active_m = vi.active_match_idx == Some(idx);

                            if ry + cell_h > py && ry < py + ph {
                                bg_rects.push(terminal::BgRect {
                                    x: rx,
                                    y: ry,
                                    w: rw.min(px + pw - rx),
                                    h: cell_h,
                                    color: if is_active_m {
                                        terminal::ColorRgb { r: 255, g: 158, b: 59 } // Kanagawa surimiOrange
                                    } else {
                                        terminal::ColorRgb { r: 230, g: 195, b: 132 } // Kanagawa boatYellow2
                                    },
                                    alpha: if is_active_m { 0.70 } else { 0.35 },
                                });
                            }
                        }
                    }

                    // 2. Regex Hints Highlights (Pure GPU BgRect)
                    if vi.hints_active {
                        for m in &vi.hint_matches {
                            if m.point.abs_row >= start_abs && m.point.abs_row < end_abs {
                                let vis_r = m.point.abs_row - start_abs;
                                let rx = px + pane::PANE_PAD + m.point.col as f32 * cell_w;
                                let ry = py + pane::PANE_PAD + vis_r as f32 * cell_h - sub_y;
                                let rw = m.len as f32 * cell_w;

                                if ry + cell_h > py && ry < py + ph {
                                    bg_rects.push(terminal::BgRect {
                                        x: rx,
                                        y: ry,
                                        w: rw.min(px + pw - rx),
                                        h: cell_h,
                                        color: terminal::ColorRgb { r: 152, g: 187, b: 108 }, // Kanagawa springGreen
                                        alpha: 0.32,
                                    });
                                    bg_rects.push(terminal::BgRect {
                                        x: rx,
                                        y: ry + cell_h - 2.0,
                                        w: rw.min(px + pw - rx),
                                        h: 2.0,
                                        color: terminal::ColorRgb { r: 152, g: 187, b: 108 },
                                        alpha: 0.90,
                                    });
                                }
                            }
                        }
                    }

                    // 3. Vi Navigation Cursor (Pure GPU BgRect)
                    if vi.cursor_abs_row >= start_abs && vi.cursor_abs_row < end_abs {
                        let vis_r = vi.cursor_abs_row - start_abs;
                        let rx = px + pane::PANE_PAD + vi.cursor_col as f32 * cell_w;
                        let ry = py + pane::PANE_PAD + vis_r as f32 * cell_h - sub_y;

                        let vi_color = match vi.kind {
                            pane::ViModeKind::Normal => terminal::ColorRgb { r: 255, g: 158, b: 59 },
                            pane::ViModeKind::Visual => terminal::ColorRgb { r: 126, g: 156, b: 216 },
                            pane::ViModeKind::VisualLine => terminal::ColorRgb { r: 230, g: 195, b: 132 },
                        };

                        if ry + cell_h > py && ry < py + ph {
                            bg_rects.push(terminal::BgRect {
                                x: rx,
                                y: ry,
                                w: cell_w,
                                h: cell_h,
                                color: vi_color,
                                alpha: 0.75,
                            });
                        }
                    }

                    // 4. Vi Mode Badge / Search Input Bar (Pure GPU BgRect)
                    let bar_h = 24.0;
                    let bar_y = py + ph - bar_h - 6.0;
                    if vi.in_search_input {
                        bg_rects.push(terminal::BgRect {
                            x: px + 12.0,
                            y: bar_y,
                            w: (pw - 24.0).max(80.0),
                            h: bar_h,
                            color: terminal::ColorRgb { r: 34, g: 50, b: 73 }, // Kanagawa waveBlue1
                            alpha: 0.95,
                        });
                        bg_rects.push(terminal::BgRect {
                            x: px + 12.0,
                            y: bar_y,
                            w: (pw - 24.0).max(80.0),
                            h: 1.5,
                            color: terminal::ColorRgb { r: 126, g: 156, b: 216 },
                            alpha: 0.90,
                        });
                    } else {
                        let badge_w = 190.0;
                        bg_rects.push(terminal::BgRect {
                            x: px + 12.0,
                            y: bar_y,
                            w: badge_w,
                            h: bar_h,
                            color: match vi.kind {
                                pane::ViModeKind::Normal => terminal::ColorRgb { r: 42, g: 42, b: 55 },
                                pane::ViModeKind::Visual => terminal::ColorRgb { r: 45, g: 79, b: 103 },
                                pane::ViModeKind::VisualLine => terminal::ColorRgb { r: 65, g: 55, b: 40 },
                            },
                            alpha: 0.92,
                        });
                        bg_rects.push(terminal::BgRect {
                            x: px + 12.0,
                            y: bar_y,
                            w: badge_w,
                            h: 1.5,
                            color: match vi.kind {
                                pane::ViModeKind::Normal => terminal::ColorRgb { r: 255, g: 158, b: 59 },
                                pane::ViModeKind::Visual => terminal::ColorRgb { r: 126, g: 156, b: 216 },
                                pane::ViModeKind::VisualLine => terminal::ColorRgb { r: 230, g: 195, b: 132 },
                            },
                            alpha: 0.90,
                        });
                    }
                }
            }
        }

        // 4e. Pane Separators
        for &(_dir, sep) in &self.tab_manager.separators {
            bg_rects.push(terminal::BgRect {
                x: sep[0],
                y: sep[1],
                w: sep[2],
                h: sep[3],
                color: terminal::ColorRgb { r: 54, g: 54, b: 70 }, // Kanagawa sumiInk5
                alpha: 0.95,
            });
        }

        // 4f. Help Reference Modal Overlay
        let card_w = super::HELP_MODAL_WIDTH.min(content_w - 24.0);
        let card_h = super::HELP_MODAL_HEIGHT.min(content_h - 24.0);
        let card_x = (content_w - card_w) * 0.5;
        let card_y = (content_h - card_h) * 0.5;

        if self.help_visible {
            bg_rects.push(terminal::BgRect {
                x: 0.0,
                y: 0.0,
                w: content_w,
                h: content_h,
                color: terminal::ColorRgb { r: 14, g: 14, b: 18 },
                alpha: 0.92,
            });
            bg_rects.push(terminal::BgRect {
                x: card_x + 8.0,
                y: card_y + 10.0,
                w: card_w,
                h: card_h,
                color: terminal::ColorRgb { r: 8, g: 8, b: 12 },
                alpha: 0.60,
            });
            bg_rects.push(terminal::BgRect {
                x: card_x,
                y: card_y,
                w: card_w,
                h: card_h,
                color: terminal::ColorRgb {
                    r: self.theme.tab_bar.r,
                    g: self.theme.tab_bar.g,
                    b: self.theme.tab_bar.b,
                },
                alpha: 1.0,
            });
            bg_rects.push(terminal::BgRect {
                x: card_x,
                y: card_y,
                w: card_w,
                h: 3.5,
                color: terminal::ColorRgb { r: 127, g: 180, b: 202 }, // Kanagawa springBlue accent
                alpha: 1.0,
            });
            bg_rects.push(terminal::BgRect {
                x: card_x,
                y: card_y + 3.5,
                w: 1.0,
                h: card_h - 3.5,
                color: terminal::ColorRgb { r: 54, g: 54, b: 70 }, // Kanagawa sumiInk5
                alpha: 0.85,
            });
            bg_rects.push(terminal::BgRect {
                x: card_x + card_w - 1.0,
                y: card_y + 3.5,
                w: 1.0,
                h: card_h - 3.5,
                color: terminal::ColorRgb { r: 54, g: 54, b: 70 }, // Kanagawa sumiInk5
                alpha: 0.85,
            });
            bg_rects.push(terminal::BgRect {
                x: card_x,
                y: card_y + card_h - 1.0,
                w: card_w,
                h: 1.0,
                color: terminal::ColorRgb { r: 54, g: 54, b: 70 }, // Kanagawa sumiInk5
                alpha: 0.85,
            });

            let close_x = card_x + card_w - 32.0;
            let close_y = card_y + 8.0;
            let is_close_hovered = content_mouse[0] >= close_x
                && content_mouse[0] <= close_x + 24.0
                && content_mouse[1] >= close_y
                && content_mouse[1] <= close_y + 24.0;
            bg_rects.push(terminal::BgRect {
                x: close_x,
                y: close_y,
                w: 24.0,
                h: 24.0,
                color: if is_close_hovered {
                    terminal::ColorRgb { r: 195, g: 64, b: 67 } // Kanagawa autumnRed
                } else {
                    terminal::ColorRgb { r: 42, g: 42, b: 55 } // Kanagawa sumiInk4
                },
                alpha: if is_close_hovered { 0.95 } else { 0.70 },
            });
            bg_rects.push(terminal::BgRect {
                x: close_x + 6.0,
                y: close_y + 11.0,
                w: 12.0,
                h: 2.0,
                color: terminal::ColorRgb { r: 220, g: 215, b: 186 }, // Kanagawa fujiWhite
                alpha: 0.95,
            });

            let visible_h = card_h - 32.0;
            let max_scroll = (self.help_total_height - visible_h).max(0.0);
            if max_scroll > 0.0 {
                let track_x = card_x + card_w - 12.0;
                let track_y = card_y + 40.0;
                let track_w = 4.0;
                let track_h = card_h - 52.0;

                bg_rects.push(terminal::BgRect {
                    x: track_x,
                    y: track_y,
                    w: track_w,
                    h: track_h,
                    color: terminal::ColorRgb { r: 31, g: 31, b: 40 }, // Kanagawa sumiInk3
                    alpha: 0.50,
                });

                let thumb_ratio = (visible_h / self.help_total_height).clamp(0.1, 0.9);
                let thumb_h = (track_h * thumb_ratio).max(28.0);
                let scroll_ratio = (self.help_scroll_y / max_scroll).clamp(0.0, 1.0);
                let thumb_y = track_y + scroll_ratio * (track_h - thumb_h);

                let is_thumb_hovered = content_mouse[0] >= track_x - 6.0
                    && content_mouse[0] <= track_x + track_w + 6.0
                    && content_mouse[1] >= thumb_y
                    && content_mouse[1] <= thumb_y + thumb_h;

                bg_rects.push(terminal::BgRect {
                    x: track_x
                        - if is_thumb_hovered || self.help_dragging_scrollbar {
                            1.0
                        } else {
                            0.0
                        },
                    y: thumb_y,
                    w: track_w
                        + if is_thumb_hovered || self.help_dragging_scrollbar {
                            2.0
                        } else {
                            0.0
                        },
                    h: thumb_h,
                    color: if self.help_dragging_scrollbar || is_thumb_hovered {
                        terminal::ColorRgb { r: 127, g: 180, b: 202 } // Kanagawa springBlue
                    } else {
                        terminal::ColorRgb { r: 45, g: 79, b: 103 } // Kanagawa waveBlue2
                    },
                    alpha: if self.help_dragging_scrollbar || is_thumb_hovered {
                        0.95
                    } else {
                        0.75
                    },
                });
            }
        }

        // 4g. Settings & Configuration Hub Modal Overlay (Gear button / Ctrl+,)
        let s_card_w = super::SETTINGS_MODAL_WIDTH.min(content_w - 24.0);
        let s_card_h = super::SETTINGS_MODAL_HEIGHT.min(content_h - 24.0);
        let s_card_x = (content_w - s_card_w) * 0.5;
        let s_card_y = (content_h - s_card_h) * 0.5;

        if self.settings_visible {
            bg_rects.push(terminal::BgRect {
                x: 0.0,
                y: 0.0,
                w: content_w,
                h: content_h,
                color: terminal::ColorRgb {
                    r: self.theme.tab_bar.r,
                    g: self.theme.tab_bar.g,
                    b: self.theme.tab_bar.b,
                },
                alpha: 0.92,
            });
            bg_rects.push(terminal::BgRect {
                x: s_card_x + 8.0,
                y: s_card_y + 10.0,
                w: s_card_w,
                h: s_card_h,
                color: terminal::ColorRgb { r: 8, g: 8, b: 12 },
                alpha: 0.60,
            });
            bg_rects.push(terminal::BgRect {
                x: s_card_x,
                y: s_card_y,
                w: s_card_w,
                h: s_card_h,
                color: terminal::ColorRgb {
                    r: self.theme.tab_bar.r,
                    g: self.theme.tab_bar.g,
                    b: self.theme.tab_bar.b,
                },
                alpha: 1.0,
            });
            bg_rects.push(terminal::BgRect {
                x: s_card_x,
                y: s_card_y,
                w: s_card_w,
                h: 3.5,
                color: terminal::ColorRgb {
                    r: self.theme.syntax_type.r,
                    g: self.theme.syntax_type.g,
                    b: self.theme.syntax_type.b,
                },
                alpha: 1.0,
            });
            bg_rects.push(terminal::BgRect {
                x: s_card_x,
                y: s_card_y + 3.5,
                w: 1.0,
                h: s_card_h - 3.5,
                color: terminal::ColorRgb {
                    r: self.theme.border.r,
                    g: self.theme.border.g,
                    b: self.theme.border.b,
                },
                alpha: 0.85,
            });
            bg_rects.push(terminal::BgRect {
                x: s_card_x + s_card_w - 1.0,
                y: s_card_y + 3.5,
                w: 1.0,
                h: s_card_h - 3.5,
                color: terminal::ColorRgb {
                    r: self.theme.border.r,
                    g: self.theme.border.g,
                    b: self.theme.border.b,
                },
                alpha: 0.85,
            });
            bg_rects.push(terminal::BgRect {
                x: s_card_x,
                y: s_card_y + s_card_h - 1.0,
                w: s_card_w,
                h: 1.0,
                color: terminal::ColorRgb {
                    r: self.theme.border.r,
                    g: self.theme.border.g,
                    b: self.theme.border.b,
                },
                alpha: 0.85,
            });

            // Active row highlight (accurately positioned using font line_height)
            let line_h = self.text_engine.line_height;
            let item_y = s_card_y + 16.0 + (4.0 + self.settings_selected_index as f32 * 2.0) * line_h;
            let sel_row_y = item_y - 2.0;
            let sel_row_h = line_h + 4.0;

            // Highlight background bar
            bg_rects.push(terminal::BgRect {
                x: s_card_x + 12.0,
                y: sel_row_y,
                w: s_card_w - 24.0,
                h: sel_row_h,
                color: terminal::ColorRgb {
                    r: self.theme.selection.r,
                    g: self.theme.selection.g,
                    b: self.theme.selection.b,
                },
                alpha: 0.35,
            });

            // Accent indicator bar on the left edge of selected row
            bg_rects.push(terminal::BgRect {
                x: s_card_x + 12.0,
                y: sel_row_y,
                w: 3.5,
                h: sel_row_h,
                color: terminal::ColorRgb {
                    r: self.theme.syntax_keyword.r,
                    g: self.theme.syntax_keyword.g,
                    b: self.theme.syntax_keyword.b,
                },
                alpha: 0.95,
            });

            // Close button [✕]
            let close_x = s_card_x + s_card_w - 32.0;
            let close_y = s_card_y + 8.0;
            let is_close_hovered = content_mouse[0] >= close_x
                && content_mouse[0] <= close_x + 24.0
                && content_mouse[1] >= close_y
                && content_mouse[1] <= close_y + 24.0;
            bg_rects.push(terminal::BgRect {
                x: close_x,
                y: close_y,
                w: 24.0,
                h: 24.0,
                color: if is_close_hovered {
                    terminal::ColorRgb { r: 195, g: 64, b: 67 }
                } else {
                    terminal::ColorRgb {
                        r: self.theme.tab_active.r,
                        g: self.theme.tab_active.g,
                        b: self.theme.tab_active.b,
                    }
                },
                alpha: if is_close_hovered { 0.95 } else { 0.70 },
            });
            bg_rects.push(terminal::BgRect {
                x: close_x + 6.0,
                y: close_y + 11.0,
                w: 12.0,
                h: 2.0,
                color: terminal::ColorRgb {
                    r: self.theme.fg.r,
                    g: self.theme.fg.g,
                    b: self.theme.fg.b,
                },
                alpha: 0.95,
            });
        }

        // Upload background, decorations, and cursor quads via reusable scratch buffer
        self.text_engine
            .update_bg_rects(&self.device, &self.queue, &bg_rects);

        // Upload Kitty Graphics images for visible panes (with smooth scroll, aspect-ratio fit, & pane clipping)
        let mut live_image_ids = std::collections::HashSet::new();
        for pane in self.tab_manager.panes.values() {
            for img in &pane.terminal.graphics.images {
                live_image_ids.insert(img.id);
            }
        }
        let mut image_draw_requests = Vec::new();
        for &id in &visible_leaves {
            if let Some(pane) = self.tab_manager.panes.get(&id) {
                let px = pane.rect[0];
                let py = pane.rect[1];
                let pw = pane.rect[2];
                let ph = pane.rect[3];
                let clip_x0 = px + pane::PANE_PAD;
                let clip_y0 = py + pane::PANE_PAD;
                let clip_x1 = px + pw - pane::PANE_PAD;
                let clip_y1 = py + ph - pane::PANE_PAD;

                let scrollback_len = pane.terminal.scrollback.len();
                let effective_base = if pane.terminal.current_scroll_y > 0.0 {
                    (pane.terminal.current_scroll_y / cell_h.max(1.0)).floor() as usize
                } else {
                    pane.terminal.scroll_offset
                };
                let doc_start = scrollback_len.saturating_sub(effective_base);
                let sub_y = pane.scroll_subpixel_y;

                for img in &pane.terminal.graphics.images {
                    let screen_row = img.abs_row as f32 - doc_start as f32;
                    let box_w = (img.width_cells as f32 * cell_w).max(1.0);
                    let box_h = (img.height_cells as f32 * cell_h).max(1.0);

                    // Preserve exact native pixel aspect ratio inside the requested cell box (strict fit-contain)
                    let (w, h) = if img.pixel_width > 0 && img.pixel_height > 0 {
                        let scale_x = box_w / img.pixel_width as f32;
                        let scale_y = box_h / img.pixel_height as f32;
                        let scale = scale_x.min(scale_y);
                        (
                            (img.pixel_width as f32 * scale).min(box_w),
                            (img.pixel_height as f32 * scale).min(box_h),
                        )
                    } else {
                        (box_w, box_h)
                    };

                    let raw_x0 = clip_x0 + img.col as f32 * cell_w;
                    let raw_y0 = clip_y0 + screen_row * cell_h - sub_y;
                    let raw_x1 = raw_x0 + w;
                    let raw_y1 = raw_y0 + h;

                    // Clip quad & UVs cleanly to pane viewport so images never bleed into titlebar/tab bar
                    let cx0 = raw_x0.max(clip_x0);
                    let cy0 = raw_y0.max(clip_y0);
                    let cx1 = raw_x1.min(clip_x1);
                    let cy1 = raw_y1.min(clip_y1);

                    if cx1 > cx0 && cy1 > cy0 {
                        let u0 = (cx0 - raw_x0) / w;
                        let u1 = (cx1 - raw_x0) / w;
                        let v0 = (cy0 - raw_y0) / h;
                        let v1 = (cy1 - raw_y0) / h;
                        image_draw_requests.push((img, [cx0, cy0, cx1, cy1], [u0, v0, u1, v1]));
                    }
                }
            }
        }
        self.text_engine.update_images(
            &self.device,
            &self.queue,
            image_draw_requests,
            &live_image_ids,
        );

        // Prepare Text Areas
        let mut areas = Vec::with_capacity(visible_leaves.len() + 6);
        let bar_text_top = (10.0 + (28.0 - self.text_engine.line_height) * 0.5).round();

        if show_center_pill {
            let center_text_x = (center_pill_x + (center_pill_w - self.bar_center_width) * 0.5).round();
            areas.push(glyphon::TextArea {
                buffer: &self.bar_center_buffer,
                left: center_text_x,
                top: bar_text_top,
                scale: 1.0,
                bounds: glyphon::TextBounds {
                    left: center_pill_x as i32,
                    top: 9,
                    right: (center_pill_x + center_pill_w) as i32,
                    bottom: 39,
                },
                default_color: glyphon::Color::rgb(220, 215, 186), // fujiWhite
                custom_glyphs: &[],
            });
        }

        // Clock Text centered inside Clock Pill
        let clock_text_x = (clock_x + (clock_w - self.bar_clock_width) * 0.5).round();
        areas.push(glyphon::TextArea {
            buffer: &self.bar_clock_buffer,
            left: clock_text_x,
            top: bar_text_top,
            scale: 1.0,
            bounds: glyphon::TextBounds {
                left: clock_x as i32,
                top: 9,
                right: (clock_x + clock_w) as i32,
                bottom: 39,
            },
            default_color: glyphon::Color::rgb(200, 192, 147), // oldWhite
            custom_glyphs: &[],
        });

        // Gear Icon centered inside [content_w - 74.0 .. content_w - 42.0]
        let gear_x = content_w - 74.0;
        let gear_text_x = (gear_x + (32.0 - self.bar_gear_width) * 0.5).round();
        areas.push(glyphon::TextArea {
            buffer: &self.bar_gear_buffer,
            left: gear_text_x,
            top: bar_text_top,
            scale: 1.0,
            bounds: glyphon::TextBounds {
                left: gear_x as i32,
                top: 9,
                right: (gear_x + 32.0) as i32,
                bottom: 39,
            },
            default_color: glyphon::Color::rgb(122, 168, 159), // waveAqua2
            custom_glyphs: &[],
        });

        // Help Icon '?' centered inside [content_w - 38.0 .. content_w - 6.0]
        let help_x = content_w - 38.0;
        let help_text_x = (help_x + (32.0 - self.bar_help_width) * 0.5).round();
        areas.push(glyphon::TextArea {
            buffer: &self.bar_help_buffer,
            left: help_text_x,
            top: bar_text_top,
            scale: 1.0,
            bounds: glyphon::TextBounds {
                left: help_x as i32,
                top: 9,
                right: (help_x + 32.0) as i32,
                bottom: 39,
            },
            default_color: glyphon::Color::rgb(230, 195, 132), // boatYellow2
            custom_glyphs: &[],
        });

        if tab_bar_visible {
            let tab_bar_y = 48.0;
            let mut pill_x = 10.0;
            for (idx, tab) in self.tab_manager.tabs.iter().enumerate() {
                let is_active = idx == self.tab_manager.active_tab_index;
                let pill_w = tab.width;
                areas.push(glyphon::TextArea {
                    buffer: &tab.buffer,
                    left: pill_x + 10.0,
                    top: tab_bar_y + 6.0,
                    scale: 1.0,
                    bounds: glyphon::TextBounds {
                        left: pill_x as i32,
                        top: tab_bar_y as i32,
                        right: (pill_x + pill_w) as i32,
                        bottom: (tab_bar_y + tab_bar_h) as i32,
                    },
                    default_color: if is_active {
                        glyphon::Color::rgb(220, 215, 186)
                    } else {
                        glyphon::Color::rgb(140, 140, 135)
                    },
                    custom_glyphs: &[],
                });
                pill_x += pill_w + 4.0;
            }

            areas.push(glyphon::TextArea {
                buffer: &self.tab_manager.plus_buffer,
                left: pill_x + 9.0,
                top: tab_bar_y + 6.0,
                scale: 1.0,
                bounds: glyphon::TextBounds {
                    left: pill_x as i32,
                    top: tab_bar_y as i32,
                    right: (pill_x + 26.0) as i32,
                    bottom: (tab_bar_y + tab_bar_h) as i32,
                },
                default_color: glyphon::Color::rgb(127, 180, 202),
                custom_glyphs: &[],
            });
        }

        for &id in &visible_leaves {
            if let Some(pane) = self.tab_manager.panes.get(&id) {
                areas.push(glyphon::TextArea {
                    buffer: &pane.buffer,
                    left: pane.rect[0] + pane::PANE_PAD,
                    top: pane.rect[1] + pane::PANE_PAD - pane.scroll_subpixel_y,
                    scale: 1.0,
                    bounds: glyphon::TextBounds {
                        left: (pane.rect[0] + pane::PANE_PAD) as i32,
                        top: (pane.rect[1] + pane::PANE_PAD) as i32,
                        right: (pane.rect[0] + pane.rect[2] - pane::PANE_PAD) as i32,
                        bottom: (pane.rect[1] + pane.rect[3] - pane::PANE_PAD) as i32,
                    },
                    default_color: glyphon::Color::rgb(220, 215, 186),
                    custom_glyphs: &[],
                });
            }
        }

        // 4g. Vi Mode status overlay text
        if let Some(pane) = self.tab_manager.active_pane() {
            if let Some(vi) = &pane.vi_mode {
                let px = pane.rect[0];
                let py = pane.rect[1];
                let pw = pane.rect[2];
                let ph = pane.rect[3];
                let bar_h = 24.0;
                let bar_y = py + ph - bar_h - 6.0;

                let (status_text, status_color) = if vi.in_search_input {
                    let match_info = if vi.search_matches.is_empty() {
                        if vi.search_query.is_empty() {
                            String::new()
                        } else {
                            " [No matches]".to_string()
                        }
                    } else {
                        format!(
                            " [{}/{}] (Enter: next, Shift+Enter: prev, Esc: exit)",
                            vi.active_match_idx.map_or(1, |i| i + 1),
                            vi.search_matches.len()
                        )
                    };
                    (
                        format!(" 🔍 Find: {}{}", vi.search_query, match_info),
                        terminal::ColorRgb { r: 230, g: 195, b: 132 },
                    )
                } else {
                    let mode_str = match vi.kind {
                        pane::ViModeKind::Normal => "VI: NORMAL",
                        pane::ViModeKind::Visual => "VI: VISUAL",
                        pane::ViModeKind::VisualLine => "VI: VISUAL LINE",
                    };
                    let info = if vi.hints_active {
                        format!(" ({} hints)", vi.hint_matches.len())
                    } else if !vi.search_matches.is_empty() {
                        format!(
                            " ({}/{})",
                            vi.active_match_idx.map_or(1, |i| i + 1),
                            vi.search_matches.len()
                        )
                    } else {
                        String::new()
                    };
                    (
                        format!(" {}{} ", mode_str, info),
                        match vi.kind {
                            pane::ViModeKind::Normal => terminal::ColorRgb { r: 255, g: 158, b: 59 },
                            pane::ViModeKind::Visual => terminal::ColorRgb { r: 126, g: 156, b: 216 },
                            pane::ViModeKind::VisualLine => terminal::ColorRgb { r: 230, g: 195, b: 132 },
                        },
                    )
                };

                if self.last_vi_status != status_text {
                    let spans = [(
                        status_text.clone(),
                        terminal::CellStyle::new().with_fg(status_color).with_bold(true),
                    )];
                    self.text_engine.shape_buffer(&mut self.vi_status_buffer, &spans);
                    self.last_vi_status = status_text;
                }

                areas.push(glyphon::TextArea {
                    buffer: &self.vi_status_buffer,
                    left: px + 16.0,
                    top: bar_y + 2.0,
                    scale: 1.0,
                    bounds: glyphon::TextBounds {
                        left: (px + 12.0) as i32,
                        top: bar_y as i32,
                        right: (px + pw - 12.0) as i32,
                        bottom: (bar_y + bar_h) as i32,
                    },
                    default_color: glyphon::Color::rgb(status_color.r, status_color.g, status_color.b),
                    custom_glyphs: &[],
                });
            }
        }

        if self.help_visible {
            let padding_top = 16.0;
            let padding_bottom = 14.0;
            areas.push(glyphon::TextArea {
                buffer: &self.help_buffer,
                left: card_x + 16.0,
                top: card_y + padding_top - self.help_scroll_y,
                scale: 1.0,
                bounds: glyphon::TextBounds {
                    left: (card_x + 10.0) as i32,
                    top: (card_y + padding_top) as i32,
                    right: (card_x + card_w - 20.0) as i32,
                    bottom: (card_y + card_h - padding_bottom) as i32,
                },
                default_color: glyphon::Color::rgb(220, 215, 186),
                custom_glyphs: &[],
            });
        }

        if self.settings_visible {
            let padding_top = 16.0;
            let padding_bottom = 14.0;
            areas.push(glyphon::TextArea {
                buffer: &self.settings_buffer,
                left: s_card_x + 16.0,
                top: s_card_y + padding_top,
                scale: 1.0,
                bounds: glyphon::TextBounds {
                    left: (s_card_x + 10.0) as i32,
                    top: (s_card_y + padding_top) as i32,
                    right: (s_card_x + s_card_w - 20.0) as i32,
                    bottom: (s_card_y + s_card_h - padding_bottom) as i32,
                },
                default_color: glyphon::Color::rgb(self.theme.fg.r, self.theme.fg.g, self.theme.fg.b),
                custom_glyphs: &[],
            });
        }

        self.text_engine
            .prepare_areas(&self.device, &self.queue, areas);

        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.resize(self.window.inner_size());
                return true;
            }
            _ => return true,
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let ss = [
            self.surface_config.width as f32,
            self.surface_config.height as f32,
        ];

        // PASS 1: Render Text & Backgrounds to Offscreen Content Texture
        {
            let mut text_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Offscreen Text Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.wobble.content_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            self.text_engine.render(&mut text_pass);
        }

        // PASS 2: Render Wobbly Window Mesh with Texture Mapping
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main Swapchain Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(if self.is_maximized {
                            let lin = |c: f32| {
                                if self.text_engine.srgb_target {
                                    crate::render::srgb_to_linear(c)
                                } else {
                                    c
                                }
                            };
                            wgpu::Color {
                                r: lin(self.theme.tab_bar.r as f32 / 255.0) as f64,
                                g: lin(self.theme.tab_bar.g as f32 / 255.0) as f64,
                                b: lin(self.theme.tab_bar.b as f32 / 255.0) as f64,
                                a: 1.0,
                            }
                        } else {
                            wgpu::Color {
                                r: 0.0,
                                g: 0.0,
                                b: 0.0,
                                a: 0.0,
                            }
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            self.wobble.render(&self.queue, &mut rp, ss);
        }

        self.queue.submit(std::iter::once(enc.finish()));
        self.queue.present(output);

        if self.initial_trim_frames < 5 {
            self.initial_trim_frames += 1;
            if self.initial_trim_frames == 5 {
                crate::platform::trim_working_set();
            }
        }

        self.high_fps_animating = high_fps_animating;
        if high_fps_animating {
            self.next_blink_frame = None;
            self.window.request_redraw();
        } else if needs_blink_timer {
            // Schedule smooth 60 FPS animation for Kanagawa ink wave (and cursor blink)
            let interval = if self.is_focused { 16 } else { 50 };
            self.next_blink_frame = Some(now + Duration::from_millis(interval));
        } else {
            self.next_blink_frame = None;
        }
        true
    }
}
