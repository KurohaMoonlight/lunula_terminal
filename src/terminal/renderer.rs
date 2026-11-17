//! Span and background quad generation with run-length merging for `TerminalGrid`.

use super::cell::{BgRect, Cell, CellStyle, ColorRgb, CursorShape};
use super::procedural::{push_bg_rect_merged, render_procedural_glyph};
use super::TerminalGrid;

impl TerminalGrid {
    pub fn render_spans_and_backgrounds(
        &self,
        cell_w: f32,
        cell_h: f32,
        pad_x: f32,
        pad_y: f32,
    ) -> (Vec<(String, CellStyle)>, Vec<BgRect>) {
        let mut spans = Vec::with_capacity(self.rows * 2);
        let mut bg_rects = Vec::with_capacity(self.rows * 2);

        let base_offset = (self.current_scroll_y / cell_h.max(1.0)).floor() as usize;
        let effective_base = if self.current_scroll_y > 0.0 {
            base_offset
        } else {
            self.scroll_offset
        };
        let show_cursor = self.cursor_visible && effective_base == 0;
        let sb_len = self.scrollback.len();
        let total_rows = sb_len + self.rows;
        let start_abs_idx = total_rows.saturating_sub(effective_base + self.rows);

        for r in 0..self.rows {
            if r > 0 {
                spans.push(("\n".to_string(), CellStyle::default()));
            }

            let abs_r = start_abs_idx + r;
            let row = self.visible_row_at(r, effective_base);
            let content_len = row
                .iter()
                .rposition(|cell| cell.c != ' ' || cell.style.bg.is_some())
                .map(|i| i + 1)
                .unwrap_or(0);
            let mut end = content_len;
            if show_cursor && r == self.cursor_y && self.cursor_x >= end {
                end = self.cursor_x + 1;
            }

            let mut cur_text = String::with_capacity(end);
            let mut cur_style = CellStyle::default();

            let row_y = pad_y + r as f32 * cell_h;

            for c in 0..end {
                let cell = if c < row.len() { row[c] } else { Cell::default() };
                let is_cursor = show_cursor && r == self.cursor_y && c == self.cursor_x;
                let is_selected = c < content_len
                    && self
                        .selection
                        .as_ref()
                        .map_or(false, |s| !s.is_empty() && s.contains(c, abs_r));

                let mut eff_style = cell.style;
                if is_selected {
                    eff_style.bg = Some(self.selection_color);
                    eff_style.fg = Some(ColorRgb { r: 245, g: 245, b: 255 });
                } else if eff_style.reverse() {
                    std::mem::swap(&mut eff_style.fg, &mut eff_style.bg);
                }

                // Pixel snap to eliminate 1px gap between Oh My Posh bubble/powerline segments
                let x0 = (pad_x + c as f32 * cell_w).round();
                let x1 = (pad_x + (c + 1) as f32 * cell_w).round();
                let y0 = row_y.round();
                let y1 = (row_y + cell_h).round();

                // 1. Cell background (Run-Length Merged across adjacent matching columns)
                if let Some(bg) = eff_style.bg {
                    push_bg_rect_merged(
                        &mut bg_rects,
                        BgRect {
                            x: x0,
                            y: y0,
                            w: x1 - x0,
                            h: y1 - y0,
                            color: bg,
                            alpha: 1.0,
                        },
                    );
                }

                // 2. Block cursor cell styling (invert text to dark for readability)
                if is_cursor && self.cursor_config.shape == CursorShape::Block {
                    eff_style.fg = Some(ColorRgb { r: 17, g: 17, b: 27 });
                }

                // 3. Underline decoration (Run-Length Merged)
                if eff_style.underline() {
                    let u_color = eff_style
                        .underline_color
                        .or(eff_style.fg)
                        .unwrap_or(ColorRgb { r: 205, g: 214, b: 244 });
                    push_bg_rect_merged(
                        &mut bg_rects,
                        BgRect {
                            x: x0,
                            y: y1 - 2.0,
                            w: x1 - x0,
                            h: 1.5,
                            color: u_color,
                            alpha: 1.0,
                        },
                    );
                }

                // 4. Strikethrough decoration (Run-Length Merged)
                if eff_style.strikethrough() {
                    let s_color = eff_style.fg.unwrap_or(ColorRgb { r: 205, g: 214, b: 244 });
                    push_bg_rect_merged(
                        &mut bg_rects,
                        BgRect {
                            x: x0,
                            y: y0 + (y1 - y0) * 0.5 - 0.75,
                            w: x1 - x0,
                            h: 1.5,
                            color: s_color,
                            alpha: 1.0,
                        },
                    );
                }

                // 5. Undercurl decoration (Neovim LSP diagnostic wavy underline)
                if eff_style.undercurl() {
                    let c_color = eff_style
                        .underline_color
                        .unwrap_or(ColorRgb { r: 243, g: 139, b: 168 });
                    let w = x1 - x0;
                    let mid_x = x0 + w * 0.5;
                    bg_rects.push(BgRect {
                        x: x0,
                        y: y1 - 3.5,
                        w: w * 0.5,
                        h: 1.5,
                        color: c_color,
                        alpha: 1.0,
                    });
                    bg_rects.push(BgRect {
                        x: mid_x,
                        y: y1 - 1.5,
                        w: w * 0.5,
                        h: 1.5,
                        color: c_color,
                        alpha: 1.0,
                    });
                }

                // Text glyph character is 100% pure (no fake cursor characters)
                let mut ch = cell.c;
                let glyph_color = eff_style.fg.unwrap_or(ColorRgb { r: 205, g: 214, b: 244 });
                if render_procedural_glyph(ch, x0, x1, y0, y1, glyph_color, &mut bg_rects) {
                    ch = ' ';
                }

                // Group consecutive matching styles
                if cur_text.is_empty() {
                    cur_style = eff_style;
                    cur_text.push(ch);
                } else if cur_style == eff_style {
                    cur_text.push(ch);
                } else {
                    spans.push((cur_text, cur_style));
                    cur_style = eff_style;
                    cur_text = String::with_capacity(end - c);
                    cur_text.push(ch);
                }
            }

            if !cur_text.is_empty() {
                spans.push((cur_text, cur_style));
            }
        }

        (spans, bg_rects)
    }
}
