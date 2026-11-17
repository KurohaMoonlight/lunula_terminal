//! Terminal cell primitives, 16/256-color palettes, and bitflag-compacted `CellStyle` (20-byte `Cell`).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub fn ansi_16_color(idx: usize) -> ColorRgb {
    // Catppuccin Mocha 16-color palette
    const PALETTE: [ColorRgb; 16] = [
        ColorRgb { r: 69, g: 71, b: 90 },    // 0: Black (Surface 1)
        ColorRgb { r: 243, g: 139, b: 168 }, // 1: Red
        ColorRgb { r: 166, g: 227, b: 161 }, // 2: Green
        ColorRgb { r: 249, g: 226, b: 175 }, // 3: Yellow
        ColorRgb { r: 137, g: 180, b: 250 }, // 4: Blue
        ColorRgb { r: 245, g: 194, b: 231 }, // 5: Magenta (Pink)
        ColorRgb { r: 148, g: 226, b: 213 }, // 6: Cyan (Teal)
        ColorRgb { r: 186, g: 194, b: 222 }, // 7: White (Subtext 1)
        ColorRgb { r: 88, g: 91, b: 112 },   // 8: Bright Black (Surface 2)
        ColorRgb { r: 235, g: 160, b: 172 }, // 9: Bright Red (Maroon)
        ColorRgb { r: 166, g: 227, b: 161 }, // 10: Bright Green
        ColorRgb { r: 249, g: 226, b: 175 }, // 11: Bright Yellow
        ColorRgb { r: 137, g: 220, b: 235 }, // 12: Bright Blue (Sky)
        ColorRgb { r: 203, g: 166, b: 247 }, // 13: Bright Magenta (Mauve)
        ColorRgb { r: 137, g: 220, b: 235 }, // 14: Bright Cyan (Sky)
        ColorRgb { r: 205, g: 214, b: 244 }, // 15: Bright White (Text)
    ];
    PALETTE[idx.min(15)]
}

pub fn color_256(idx: u8) -> ColorRgb {
    if idx < 16 {
        return ansi_16_color(idx as usize);
    }
    if idx >= 232 {
        let gray = 8 + (idx - 232) * 10;
        return ColorRgb { r: gray, g: gray, b: gray };
    }
    let idx = idx - 16;
    let b_idx = idx % 6;
    let g_idx = (idx / 6) % 6;
    let r_idx = idx / 36;
    let to_val = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
    ColorRgb {
        r: to_val(r_idx),
        g: to_val(g_idx),
        b: to_val(b_idx),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorShape {
    Block,
    Underline,
    Beam,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorConfig {
    pub shape: CursorShape,
    pub blinking: bool,
    pub color: ColorRgb,
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self {
            shape: CursorShape::Block,
            blinking: true,
            color: ColorRgb { r: 180, g: 190, b: 254 }, // Catppuccin Lavender
        }
    }
}

pub const STYLE_BOLD: u8 = 1 << 0;
pub const STYLE_DIM: u8 = 1 << 1;
pub const STYLE_ITALIC: u8 = 1 << 2;
pub const STYLE_UNDERLINE: u8 = 1 << 3;
pub const STYLE_UNDERCURL: u8 = 1 << 4;
pub const STYLE_STRIKETHROUGH: u8 = 1 << 5;
pub const STYLE_REVERSE: u8 = 1 << 6;

/// Compact 13-byte cell style packing all 7 boolean attributes into a single `u8` bitfield.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CellStyle {
    pub fg: Option<ColorRgb>,
    pub bg: Option<ColorRgb>,
    pub underline_color: Option<ColorRgb>,
    pub flags: u8,
}

impl CellStyle {
    #[inline]
    pub const fn new() -> Self {
        Self {
            fg: None,
            bg: None,
            underline_color: None,
            flags: 0,
        }
    }

    #[inline]
    pub const fn with_fg(mut self, color: ColorRgb) -> Self {
        self.fg = Some(color);
        self
    }

    #[inline]
    pub const fn with_bold(mut self, val: bool) -> Self {
        if val {
            self.flags |= STYLE_BOLD;
        } else {
            self.flags &= !STYLE_BOLD;
        }
        self
    }

    #[inline]
    pub const fn with_dim(mut self, val: bool) -> Self {
        if val {
            self.flags |= STYLE_DIM;
        } else {
            self.flags &= !STYLE_DIM;
        }
        self
    }

    #[inline]
    pub const fn bold(&self) -> bool {
        (self.flags & STYLE_BOLD) != 0
    }

    #[inline]
    pub fn set_bold(&mut self, val: bool) {
        if val {
            self.flags |= STYLE_BOLD;
        } else {
            self.flags &= !STYLE_BOLD;
        }
    }

    #[inline]
    #[allow(dead_code)]
    pub const fn dim(&self) -> bool {
        (self.flags & STYLE_DIM) != 0
    }

    #[inline]
    pub fn set_dim(&mut self, val: bool) {
        if val {
            self.flags |= STYLE_DIM;
        } else {
            self.flags &= !STYLE_DIM;
        }
    }

    #[inline]
    pub const fn italic(&self) -> bool {
        (self.flags & STYLE_ITALIC) != 0
    }

    #[inline]
    pub fn set_italic(&mut self, val: bool) {
        if val {
            self.flags |= STYLE_ITALIC;
        } else {
            self.flags &= !STYLE_ITALIC;
        }
    }

    #[inline]
    pub const fn underline(&self) -> bool {
        (self.flags & STYLE_UNDERLINE) != 0
    }

    #[inline]
    pub fn set_underline(&mut self, val: bool) {
        if val {
            self.flags |= STYLE_UNDERLINE;
        } else {
            self.flags &= !STYLE_UNDERLINE;
        }
    }

    #[inline]
    pub const fn undercurl(&self) -> bool {
        (self.flags & STYLE_UNDERCURL) != 0
    }

    #[inline]
    pub fn set_undercurl(&mut self, val: bool) {
        if val {
            self.flags |= STYLE_UNDERCURL;
        } else {
            self.flags &= !STYLE_UNDERCURL;
        }
    }

    #[inline]
    pub const fn strikethrough(&self) -> bool {
        (self.flags & STYLE_STRIKETHROUGH) != 0
    }

    #[inline]
    pub fn set_strikethrough(&mut self, val: bool) {
        if val {
            self.flags |= STYLE_STRIKETHROUGH;
        } else {
            self.flags &= !STYLE_STRIKETHROUGH;
        }
    }

    #[inline]
    pub const fn reverse(&self) -> bool {
        (self.flags & STYLE_REVERSE) != 0
    }

    #[inline]
    pub fn set_reverse(&mut self, val: bool) {
        if val {
            self.flags |= STYLE_REVERSE;
        } else {
            self.flags &= !STYLE_REVERSE;
        }
    }
}

/// 20-byte terminal cell (reduced from 28 bytes via bitflag packing and field ordering).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub c: char,
    pub hyperlink_idx: u16,
    pub style: CellStyle,
}

impl Default for Cell {
    #[inline]
    fn default() -> Self {
        Self {
            c: ' ',
            hyperlink_idx: 0,
            style: CellStyle::new(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BgRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub color: ColorRgb,
    pub alpha: f32,
}
