//! GPU-accelerated procedural box-drawing, block elements, and Powerline glyphs with run-length quad merging.

use super::cell::{BgRect, ColorRgb};

/// Appends a `BgRect` while coalescing horizontally adjacent rects with identical vertical bounds and color.
#[inline]
pub fn push_bg_rect_merged(bg_rects: &mut Vec<BgRect>, rect: BgRect) {
    if let Some(last) = bg_rects.last_mut() {
        if last.color == rect.color
            && (last.alpha - rect.alpha).abs() < 1e-4
            && (last.y - rect.y).abs() < 0.1
            && (last.h - rect.h).abs() < 0.1
            && (last.x + last.w - rect.x).abs() < 0.5
        {
            last.w = (rect.x + rect.w) - last.x;
            return;
        }
    }
    bg_rects.push(rect);
}

pub fn render_procedural_glyph(
    c: char,
    x0: f32,
    x1: f32,
    y0: f32,
    y1: f32,
    fg: ColorRgb,
    bg_rects: &mut Vec<BgRect>,
) -> bool {
    let w = x1 - x0;
    let h = y1 - y0;
    let mid_x = ((x0 + x1) * 0.5).round();
    let mid_y = ((y0 + y1) * 0.5).round();
    let line_thick = (h * 0.12).max(1.5).round();
    let half_thick = (line_thick * 0.5).round();

    match c {
        // Horizontal box lines (coalesced across adjacent columns)
        '─' | '━' => {
            push_bg_rect_merged(
                bg_rects,
                BgRect {
                    x: x0,
                    y: mid_y - half_thick,
                    w,
                    h: line_thick,
                    color: fg,
                    alpha: 1.0,
                },
            );
            true
        }
        // Vertical box lines
        '│' | '┃' => {
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: y0,
                w: line_thick,
                h,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        // Corners
        '┌' | '┏' => {
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: mid_y - half_thick,
                w: x1 - mid_x + half_thick,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: mid_y - half_thick,
                w: line_thick,
                h: y1 - mid_y + half_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        '┐' | '┓' => {
            bg_rects.push(BgRect {
                x: x0,
                y: mid_y - half_thick,
                w: mid_x - x0 + half_thick,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: mid_y - half_thick,
                w: line_thick,
                h: y1 - mid_y + half_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        '└' | '┗' => {
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: mid_y - half_thick,
                w: x1 - mid_x + half_thick,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: y0,
                w: line_thick,
                h: mid_y - y0 + half_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        '┘' | '┛' => {
            bg_rects.push(BgRect {
                x: x0,
                y: mid_y - half_thick,
                w: mid_x - x0 + half_thick,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: y0,
                w: line_thick,
                h: mid_y - y0 + half_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        // Intersections
        '├' | '┣' => {
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: y0,
                w: line_thick,
                h,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: mid_y - half_thick,
                w: x1 - mid_x + half_thick,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        '┤' | '┫' => {
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: y0,
                w: line_thick,
                h,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: x0,
                y: mid_y - half_thick,
                w: mid_x - x0 + half_thick,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        '┬' | '┳' => {
            bg_rects.push(BgRect {
                x: x0,
                y: mid_y - half_thick,
                w,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: mid_y - half_thick,
                w: line_thick,
                h: y1 - mid_y + half_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        '┴' | '┻' => {
            bg_rects.push(BgRect {
                x: x0,
                y: mid_y - half_thick,
                w,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: y0,
                w: line_thick,
                h: mid_y - y0 + half_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        '┼' | '╋' => {
            bg_rects.push(BgRect {
                x: x0,
                y: mid_y - half_thick,
                w,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: y0,
                w: line_thick,
                h,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        // Powerline right arrow solid (\u{E0B0}) - 6 steps instead of 12
        '\u{E0B0}' => {
            let steps = 6;
            for k in 0..steps {
                let t = k as f32 / steps as f32;
                let sx0 = x0 + t * w;
                let sx1 = x0 + (k + 1) as f32 / steps as f32 * w;
                let half_h = (1.0 - t) * (h * 0.5);
                bg_rects.push(BgRect {
                    x: sx0,
                    y: mid_y - half_h,
                    w: (sx1 - sx0 + 0.5).max(1.0),
                    h: (half_h * 2.0).max(1.0),
                    color: fg,
                    alpha: 1.0,
                });
            }
            true
        }
        // Powerline left arrow solid (\u{E0B2})
        '\u{E0B2}' => {
            let steps = 6;
            for k in 0..steps {
                let t = k as f32 / steps as f32;
                let sx0 = x0 + t * w;
                let sx1 = x0 + (k + 1) as f32 / steps as f32 * w;
                let half_h = t * (h * 0.5);
                bg_rects.push(BgRect {
                    x: sx0,
                    y: mid_y - half_h,
                    w: (sx1 - sx0 + 0.5).max(1.0),
                    h: (half_h * 2.0).max(1.0),
                    color: fg,
                    alpha: 1.0,
                });
            }
            true
        }
        // Powerline left bubble round cap (\u{E0B6})
        '\u{E0B6}' => {
            let steps = 6;
            for k in 0..steps {
                let t = k as f32 / steps as f32;
                let sx0 = x0 + t * w;
                let sx1 = x0 + (k + 1) as f32 / steps as f32 * w;
                let arc = (t * (2.0 - t)).max(0.0).sqrt();
                let half_h = arc * (h * 0.5);
                bg_rects.push(BgRect {
                    x: sx0,
                    y: mid_y - half_h,
                    w: (sx1 - sx0 + 0.5).max(1.0),
                    h: (half_h * 2.0).max(1.0),
                    color: fg,
                    alpha: 1.0,
                });
            }
            true
        }
        // Powerline right bubble round cap (\u{E0B4})
        '\u{E0B4}' => {
            let steps = 6;
            for k in 0..steps {
                let t = 1.0 - (k as f32 / steps as f32);
                let sx0 = x0 + (k as f32 / steps as f32) * w;
                let sx1 = x0 + ((k + 1) as f32 / steps as f32) * w;
                let arc = (t * (2.0 - t)).max(0.0).sqrt();
                let half_h = arc * (h * 0.5);
                bg_rects.push(BgRect {
                    x: sx0,
                    y: mid_y - half_h,
                    w: (sx1 - sx0 + 0.5).max(1.0),
                    h: (half_h * 2.0).max(1.0),
                    color: fg,
                    alpha: 1.0,
                });
            }
            true
        }
        // Rounded corners
        '╭' => {
            bg_rects.push(BgRect {
                x: mid_x + half_thick,
                y: mid_y - half_thick,
                w: x1 - (mid_x + half_thick),
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: mid_y + half_thick,
                w: line_thick,
                h: y1 - (mid_y + half_thick),
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick * 0.5,
                y: mid_y - half_thick * 0.5,
                w: line_thick,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        '╮' => {
            bg_rects.push(BgRect {
                x: x0,
                y: mid_y - half_thick,
                w: (mid_x - half_thick - x0).max(1.0),
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: mid_y + half_thick,
                w: line_thick,
                h: y1 - (mid_y + half_thick),
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick * 0.5,
                y: mid_y - half_thick * 0.5,
                w: line_thick,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        '╯' => {
            bg_rects.push(BgRect {
                x: x0,
                y: mid_y - half_thick,
                w: (mid_x - half_thick - x0).max(1.0),
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: y0,
                w: line_thick,
                h: (mid_y - half_thick - y0).max(1.0),
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick * 0.5,
                y: mid_y - half_thick * 0.5,
                w: line_thick,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        '╰' => {
            bg_rects.push(BgRect {
                x: mid_x + half_thick,
                y: mid_y - half_thick,
                w: x1 - (mid_x + half_thick),
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick,
                y: y0,
                w: line_thick,
                h: (mid_y - half_thick - y0).max(1.0),
                color: fg,
                alpha: 1.0,
            });
            bg_rects.push(BgRect {
                x: mid_x - half_thick * 0.5,
                y: mid_y - half_thick * 0.5,
                w: line_thick,
                h: line_thick,
                color: fg,
                alpha: 1.0,
            });
            true
        }
        // Double lines
        '═' => {
            let offset = (line_thick * 0.9).max(2.0);
            let d_thick = (line_thick * 0.7).max(1.0);
            bg_rects.push(BgRect { x: x0, y: mid_y - offset, w, h: d_thick, color: fg, alpha: 1.0 });
            bg_rects.push(BgRect { x: x0, y: mid_y + offset - d_thick, w, h: d_thick, color: fg, alpha: 1.0 });
            true
        }
        '║' => {
            let offset = (line_thick * 0.9).max(2.0);
            let d_thick = (line_thick * 0.7).max(1.0);
            bg_rects.push(BgRect { x: mid_x - offset, y: y0, w: d_thick, h, color: fg, alpha: 1.0 });
            bg_rects.push(BgRect { x: mid_x + offset - d_thick, y: y0, w: d_thick, h, color: fg, alpha: 1.0 });
            true
        }
        // Solid block elements (coalesced across adjacent columns)
        '█' => {
            push_bg_rect_merged(bg_rects, BgRect { x: x0, y: y0, w, h, color: fg, alpha: 1.0 });
            true
        }
        '▀' => {
            push_bg_rect_merged(bg_rects, BgRect { x: x0, y: y0, w, h: (h * 0.5).round(), color: fg, alpha: 1.0 });
            true
        }
        '▄' => {
            push_bg_rect_merged(bg_rects, BgRect { x: x0, y: mid_y, w, h: y1 - mid_y, color: fg, alpha: 1.0 });
            true
        }
        '▌' => {
            bg_rects.push(BgRect { x: x0, y: y0, w: (w * 0.5).round(), h, color: fg, alpha: 1.0 });
            true
        }
        '▐' => {
            bg_rects.push(BgRect { x: mid_x, y: y0, w: x1 - mid_x, h, color: fg, alpha: 1.0 });
            true
        }
        '░' => {
            push_bg_rect_merged(bg_rects, BgRect { x: x0, y: y0, w, h, color: fg, alpha: 0.25 });
            true
        }
        '▒' => {
            push_bg_rect_merged(bg_rects, BgRect { x: x0, y: y0, w, h, color: fg, alpha: 0.50 });
            true
        }
        '▓' => {
            push_bg_rect_merged(bg_rects, BgRect { x: x0, y: y0, w, h, color: fg, alpha: 0.75 });
            true
        }
        // Thin powerlines
        '\u{E0B1}' => {
            let line_w = (line_thick * 0.8).max(1.0);
            let steps = 6;
            for k in 0..steps {
                let t = k as f32 / steps as f32;
                let sx0 = x0 + t * w;
                let half_h = (1.0 - t) * (h * 0.5);
                bg_rects.push(BgRect { x: sx0, y: mid_y - half_h, w: line_w, h: line_w, color: fg, alpha: 1.0 });
                bg_rects.push(BgRect { x: sx0, y: mid_y + half_h - line_w, w: line_w, h: line_w, color: fg, alpha: 1.0 });
            }
            true
        }
        '\u{E0B3}' => {
            let line_w = (line_thick * 0.8).max(1.0);
            let steps = 6;
            for k in 0..steps {
                let t = k as f32 / steps as f32;
                let sx0 = x0 + t * w;
                let half_h = t * (h * 0.5);
                bg_rects.push(BgRect { x: sx0, y: mid_y - half_h, w: line_w, h: line_w, color: fg, alpha: 1.0 });
                bg_rects.push(BgRect { x: sx0, y: mid_y + half_h - line_w, w: line_w, h: line_w, color: fg, alpha: 1.0 });
            }
            true
        }
        _ => false,
    }
}
