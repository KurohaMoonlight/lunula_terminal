//! Multi-theme system for Lunula Terminal & Luna Editor.
//! Built-in authentic palettes: Kanagawa Wave (default), Tokyo Night,
//! Catppuccin Mocha, Gruvbox Dark, and Nord, with custom overrides from `lunula.lua`.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl ColorRgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn to_fg_ansi(&self) -> String {
        format!("\x1b[38;2;{};{};{}m", self.r, self.g, self.b)
    }

    pub fn to_bg_ansi(&self) -> String {
        format!("\x1b[48;2;{};{};{}m", self.r, self.g, self.b)
    }

    pub fn parse_hex(hex: &str) -> Option<Self> {
        let s = hex.trim().trim_start_matches('#');
        if s.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some(Self { r, g, b })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Theme {
    pub name: String,
    pub bg: ColorRgb,
    pub fg: ColorRgb,
    pub selection: ColorRgb,
    pub cursor: ColorRgb,
    pub tab_bar: ColorRgb,
    pub tab_active: ColorRgb,
    pub border: ColorRgb,
    pub gutter_fg: ColorRgb,
    pub gutter_active: ColorRgb,
    pub syntax_keyword: ColorRgb,
    pub syntax_type: ColorRgb,
    pub syntax_string: ColorRgb,
    pub syntax_number: ColorRgb,
    pub syntax_comment: ColorRgb,
    pub syntax_punctuation: ColorRgb,
    pub syntax_header: ColorRgb,
    pub ansi_16: [ColorRgb; 16],
}

impl Theme {
    /// 1. Kanagawa Wave (Default authentic pigment palette)
    pub fn kanagawa() -> Self {
        Self {
            name: "kanagawa".to_string(),
            bg: ColorRgb::new(31, 31, 40),          // sumiInk3 #1F1F28
            fg: ColorRgb::new(220, 215, 186),      // fujiWhite #DCD7BA
            selection: ColorRgb::new(45, 79, 103),  // waveBlue2 #2D4F67
            cursor: ColorRgb::new(127, 180, 202),   // springBlue #7FB4CA
            tab_bar: ColorRgb::new(22, 22, 29),     // sumiInk0 #16161D
            tab_active: ColorRgb::new(42, 42, 55),  // sumiInk4 #2A2A37
            border: ColorRgb::new(54, 54, 70),      // sumiInk5 #363646
            gutter_fg: ColorRgb::new(114, 113, 105), // fujiGray #727169
            gutter_active: ColorRgb::new(230, 195, 132), // carpYellow #E6C384
            syntax_keyword: ColorRgb::new(149, 127, 184), // oniViolet #957FB8
            syntax_type: ColorRgb::new(127, 180, 202),    // springBlue #7FB4CA
            syntax_string: ColorRgb::new(152, 187, 108),  // springGreen #98BB6C
            syntax_number: ColorRgb::new(255, 160, 102),  // surimiOrange #FFA066
            syntax_comment: ColorRgb::new(114, 113, 105), // fujiGray #727169
            syntax_punctuation: ColorRgb::new(156, 154, 142), // sumiInk6 #9C9A8E
            syntax_header: ColorRgb::new(230, 195, 132),  // carpYellow #E6C384
            ansi_16: [
                ColorRgb::new(22, 22, 29),    // 0: sumiInk0
                ColorRgb::new(195, 64, 67),   // 1: autumnRed
                ColorRgb::new(118, 148, 106), // 2: autumnGreen
                ColorRgb::new(192, 163, 110), // 3: boatYellow1
                ColorRgb::new(126, 156, 216), // 4: crystalBlue
                ColorRgb::new(149, 127, 184), // 5: oniViolet
                ColorRgb::new(106, 149, 137), // 6: waveAqua1
                ColorRgb::new(200, 192, 147), // 7: oldWhite
                ColorRgb::new(114, 113, 105), // 8: fujiGray
                ColorRgb::new(232, 36, 36),   // 9: samuraiRed
                ColorRgb::new(152, 187, 108), // 10: springGreen
                ColorRgb::new(230, 195, 132), // 11: boatYellow2
                ColorRgb::new(127, 180, 202), // 12: springBlue
                ColorRgb::new(210, 126, 153), // 13: sakuraPink
                ColorRgb::new(122, 168, 159), // 14: waveAqua2
                ColorRgb::new(220, 215, 186), // 15: fujiWhite
            ],
        }
    }

    /// 2. Tokyo Night (Storm)
    pub fn tokyo_night() -> Self {
        Self {
            name: "tokyo_night".to_string(),
            bg: ColorRgb::new(36, 40, 59),          // #24283B
            fg: ColorRgb::new(192, 202, 245),      // #C0CAF5
            selection: ColorRgb::new(54, 74, 130),  // #364A82
            cursor: ColorRgb::new(192, 202, 245),   // #C0CAF5
            tab_bar: ColorRgb::new(31, 35, 53),     // #1F2335
            tab_active: ColorRgb::new(41, 46, 66),  // #292E42
            border: ColorRgb::new(65, 72, 104),     // #414868
            gutter_fg: ColorRgb::new(86, 95, 137),  // #565F89
            gutter_active: ColorRgb::new(224, 175, 104), // #E0AF68
            syntax_keyword: ColorRgb::new(187, 154, 247), // #BB9AF7
            syntax_type: ColorRgb::new(42, 195, 222),     // #2AC3DE
            syntax_string: ColorRgb::new(158, 206, 106),  // #9ECE6A
            syntax_number: ColorRgb::new(255, 158, 100),  // #FF9E64
            syntax_comment: ColorRgb::new(86, 95, 137),   // #565F89
            syntax_punctuation: ColorRgb::new(137, 221, 255), // #89DDFF
            syntax_header: ColorRgb::new(122, 162, 247),  // #7AA2F7
            ansi_16: [
                ColorRgb::new(29, 32, 48),    // 0: Black
                ColorRgb::new(247, 118, 142), // 1: Red
                ColorRgb::new(158, 206, 106), // 2: Green
                ColorRgb::new(224, 175, 104), // 3: Yellow
                ColorRgb::new(122, 162, 247), // 4: Blue
                ColorRgb::new(187, 154, 247), // 5: Magenta
                ColorRgb::new(125, 207, 255), // 6: Cyan
                ColorRgb::new(192, 202, 245), // 7: White
                ColorRgb::new(86, 95, 137),   // 8: Bright Black
                ColorRgb::new(247, 118, 142), // 9: Bright Red
                ColorRgb::new(158, 206, 106), // 10: Bright Green
                ColorRgb::new(224, 175, 104), // 11: Bright Yellow
                ColorRgb::new(122, 162, 247), // 12: Bright Blue
                ColorRgb::new(187, 154, 247), // 13: Bright Magenta
                ColorRgb::new(125, 207, 255), // 14: Bright Cyan
                ColorRgb::new(192, 202, 245), // 15: Bright White
            ],
        }
    }

    /// 3. Catppuccin (Mocha)
    pub fn catppuccin() -> Self {
        Self {
            name: "catppuccin".to_string(),
            bg: ColorRgb::new(30, 30, 46),          // #1E1E2E
            fg: ColorRgb::new(205, 214, 244),      // #CDD6F4
            selection: ColorRgb::new(69, 71, 90),   // #45475A
            cursor: ColorRgb::new(245, 224, 220),   // #F5E0DC
            tab_bar: ColorRgb::new(24, 24, 37),     // #181825
            tab_active: ColorRgb::new(49, 50, 68),  // #313244
            border: ColorRgb::new(88, 91, 112),     // #585B70
            gutter_fg: ColorRgb::new(108, 112, 134), // #6C7086
            gutter_active: ColorRgb::new(249, 226, 175), // #F9E2AF
            syntax_keyword: ColorRgb::new(203, 166, 247), // Mauve #CBA6F7
            syntax_type: ColorRgb::new(137, 180, 250),    // Blue #89B4FA
            syntax_string: ColorRgb::new(166, 227, 161),  // Green #A6E3A1
            syntax_number: ColorRgb::new(250, 179, 135),  // Peach #FAB387
            syntax_comment: ColorRgb::new(108, 112, 134), // Overlay0 #6C7086
            syntax_punctuation: ColorRgb::new(147, 153, 178), // Overlay2 #9399B2
            syntax_header: ColorRgb::new(249, 226, 175),  // Yellow #F9E2AF
            ansi_16: [
                ColorRgb::new(69, 71, 90),    // 0: Surface1
                ColorRgb::new(243, 139, 168), // 1: Red
                ColorRgb::new(166, 227, 161), // 2: Green
                ColorRgb::new(249, 226, 175), // 3: Yellow
                ColorRgb::new(137, 180, 250), // 4: Blue
                ColorRgb::new(245, 194, 231), // 5: Pink
                ColorRgb::new(148, 226, 213), // 6: Teal
                ColorRgb::new(186, 194, 222), // 7: Subtext1
                ColorRgb::new(88, 91, 112),   // 8: Surface2
                ColorRgb::new(243, 139, 168), // 9: Red
                ColorRgb::new(166, 227, 161), // 10: Green
                ColorRgb::new(249, 226, 175), // 11: Yellow
                ColorRgb::new(137, 180, 250), // 12: Blue
                ColorRgb::new(245, 194, 231), // 13: Pink
                ColorRgb::new(148, 226, 213), // 14: Teal
                ColorRgb::new(166, 173, 200), // 15: Subtext0
            ],
        }
    }

    /// 4. Gruvbox Dark
    pub fn gruvbox() -> Self {
        Self {
            name: "gruvbox".to_string(),
            bg: ColorRgb::new(40, 40, 40),          // #282828
            fg: ColorRgb::new(235, 219, 178),      // #EBDBB2
            selection: ColorRgb::new(80, 73, 69),   // #504945
            cursor: ColorRgb::new(235, 219, 178),   // #EBDBB2
            tab_bar: ColorRgb::new(29, 32, 33),     // #1D2021
            tab_active: ColorRgb::new(60, 56, 54),  // #3C3836
            border: ColorRgb::new(102, 92, 84),     // #665C54
            gutter_fg: ColorRgb::new(146, 131, 116), // #928374
            gutter_active: ColorRgb::new(250, 189, 47), // #FABD2F
            syntax_keyword: ColorRgb::new(251, 73, 52),  // #FB4934
            syntax_type: ColorRgb::new(131, 165, 152),   // #83A598
            syntax_string: ColorRgb::new(184, 187, 38),  // #B8BB26
            syntax_number: ColorRgb::new(211, 134, 155), // #D3869B
            syntax_comment: ColorRgb::new(146, 131, 116), // #928374
            syntax_punctuation: ColorRgb::new(189, 174, 147), // #BDAE93
            syntax_header: ColorRgb::new(250, 189, 47),  // #FABD2F
            ansi_16: [
                ColorRgb::new(40, 40, 40),    // 0
                ColorRgb::new(204, 36, 29),   // 1
                ColorRgb::new(152, 151, 26),  // 2
                ColorRgb::new(215, 153, 33),  // 3
                ColorRgb::new(69, 133, 136),  // 4
                ColorRgb::new(177, 98, 134),  // 5
                ColorRgb::new(104, 157, 106), // 6
                ColorRgb::new(168, 153, 132), // 7
                ColorRgb::new(146, 131, 116), // 8
                ColorRgb::new(251, 73, 52),   // 9
                ColorRgb::new(184, 187, 38),  // 10
                ColorRgb::new(250, 189, 47),  // 11
                ColorRgb::new(131, 165, 152), // 12
                ColorRgb::new(211, 134, 155), // 13
                ColorRgb::new(142, 192, 124), // 14
                ColorRgb::new(235, 219, 178), // 15
            ],
        }
    }

    /// 5. Nord (Frost)
    pub fn nord() -> Self {
        Self {
            name: "nord".to_string(),
            bg: ColorRgb::new(46, 52, 64),          // #2E3440
            fg: ColorRgb::new(236, 239, 244),      // #ECEFF4
            selection: ColorRgb::new(67, 76, 94),   // #434C5E
            cursor: ColorRgb::new(216, 222, 233),   // #D8DEE9
            tab_bar: ColorRgb::new(36, 41, 51),     // #242933
            tab_active: ColorRgb::new(59, 66, 82),  // #3B4252
            border: ColorRgb::new(76, 86, 106),     // #4C566A
            gutter_fg: ColorRgb::new(97, 110, 136), // #616E88
            gutter_active: ColorRgb::new(235, 203, 139), // #EBCB8B
            syntax_keyword: ColorRgb::new(129, 161, 193), // #81A1C1
            syntax_type: ColorRgb::new(136, 192, 208),    // #88C0D0
            syntax_string: ColorRgb::new(163, 190, 140),  // #A3BE8C
            syntax_number: ColorRgb::new(180, 142, 173),  // #B48EAD
            syntax_comment: ColorRgb::new(97, 110, 136),  // #616E88
            syntax_punctuation: ColorRgb::new(216, 222, 233), // #D8DEE9
            syntax_header: ColorRgb::new(235, 203, 139),  // #EBCB8B
            ansi_16: [
                ColorRgb::new(46, 52, 64),    // 0
                ColorRgb::new(191, 97, 106),  // 1
                ColorRgb::new(163, 190, 140), // 2
                ColorRgb::new(235, 203, 139), // 3
                ColorRgb::new(129, 161, 193), // 4
                ColorRgb::new(180, 142, 173), // 5
                ColorRgb::new(136, 192, 208), // 6
                ColorRgb::new(229, 233, 240), // 7
                ColorRgb::new(76, 86, 106),   // 8
                ColorRgb::new(191, 97, 106),  // 9
                ColorRgb::new(163, 190, 140), // 10
                ColorRgb::new(235, 203, 139), // 11
                ColorRgb::new(129, 161, 193), // 12
                ColorRgb::new(180, 142, 173), // 13
                ColorRgb::new(143, 188, 187), // 14
                ColorRgb::new(236, 239, 244), // 15
            ],
        }
    }

    /// Returns raw RGB array for the 16 ANSI colors.
    pub fn ansi_16_raw(&self) -> [[u8; 3]; 16] {
        let mut res = [[0u8; 3]; 16];
        for i in 0..16 {
            res[i] = [self.ansi_16[i].r, self.ansi_16[i].g, self.ansi_16[i].b];
        }
        res
    }

    /// Resolves a theme by name with fallback to Kanagawa.
    pub fn from_name(name: &str) -> Self {
        let clean = name.trim().to_lowercase().replace('-', "_");
        match clean.as_str() {
            "tokyo_night" | "tokyonight" | "storm" => Self::tokyo_night(),
            "catppuccin" | "catppuccin_mocha" | "mocha" => Self::catppuccin(),
            "gruvbox" | "gruvbox_dark" => Self::gruvbox(),
            "nord" => Self::nord(),
            "kanagawa" | "default" => Self::kanagawa(),
            other => {
                let candidates = [
                    PathBuf::from(format!("themes/{}.lua", other)),
                    std::env::current_exe()
                        .ok()
                        .and_then(|p| p.parent().map(|d| d.join(format!("themes/{}.lua", other))))
                        .unwrap_or_else(|| PathBuf::from("")),
                ];
                for cand in &candidates {
                    if !cand.as_os_str().is_empty() && cand.exists() {
                        if let Ok(content) = fs::read_to_string(cand) {
                            return Self::from_lua_content(&content);
                        }
                    }
                }
                Self::kanagawa()
            }
        }
    }

    /// Searches standard paths for `lunula.lua` and loads the active theme.
    pub fn load_from_config() -> Self {
        let paths = [
            PathBuf::from("lunula.lua"),
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("lunula.lua")))
                .unwrap_or_else(|| PathBuf::from("lunula.lua")),
            std::env::var("APPDATA")
                .map(|p| Path::new(&p).join("lunula").join("config.lua"))
                .unwrap_or_else(|_| PathBuf::from("")),
            std::env::var("USERPROFILE")
                .map(|p| Path::new(&p).join(".config").join("lunula").join("config.lua"))
                .unwrap_or_else(|_| PathBuf::from("")),
        ];

        for path in &paths {
            if path.as_os_str().is_empty() || !path.exists() {
                continue;
            }
            if let Ok(content) = fs::read_to_string(path) {
                return Self::from_lua_content(&content);
            }
        }

        Self::kanagawa()
    }

    /// Fast zero-dependency parser for theme and color overrides in Lua scripts.
    pub fn from_lua_content(content: &str) -> Self {
        let mut theme_name = "kanagawa".to_string();

        // 1. Scan for theme = "name"
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("--") {
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("theme") {
                let rest = rest.trim_start();
                if let Some(val) = rest.strip_prefix('=') {
                    let val = val.trim().trim_matches(|c| c == ',' || c == ';' || c == ' ' || c == '"' || c == '\'');
                    if !val.is_empty() {
                        theme_name = val.to_string();
                        break;
                    }
                }
            }
        }

        let mut theme = Self::from_name(&theme_name);

        // 2. Scan for optional overrides in colors = { ... }
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("--") {
                continue;
            }
            if let Some((k, v)) = extract_key_val(trimmed) {
                if let Some(c) = ColorRgb::parse_hex(&v) {
                    match k.as_str() {
                        "selection" => theme.selection = c,
                        "background" | "bg" => theme.bg = c,
                        "foreground" | "fg" => theme.fg = c,
                        "cursor" => theme.cursor = c,
                        "tab_bar" => theme.tab_bar = c,
                        "tab_active" => theme.tab_active = c,
                        "border" => theme.border = c,
                        "keyword" | "syntax_keyword" => theme.syntax_keyword = c,
                        "type" | "syntax_type" => theme.syntax_type = c,
                        "string" | "syntax_string" => theme.syntax_string = c,
                        "number" | "syntax_number" => theme.syntax_number = c,
                        "comment" | "syntax_comment" => theme.syntax_comment = c,
                        _ => {}
                    }
                }
            }
        }

        // 3. Scan for optional 16 ANSI colors: [0] = "#...", [1] = "#...", etc.
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("--") {
                continue;
            }
            if let Some(idx_start) = trimmed.find('[') {
                if let Some(idx_end) = trimmed.find(']') {
                    if idx_start < idx_end {
                        if let Ok(idx) = trimmed[idx_start + 1..idx_end].trim().parse::<usize>() {
                            if idx < 16 {
                                if let Some(eq_pos) = trimmed.find('=') {
                                    let val = trimmed[eq_pos + 1..].trim().trim_matches(|c| {
                                        c == ',' || c == ';' || c == ' ' || c == '"' || c == '\''
                                    });
                                    if let Some(c) = ColorRgb::parse_hex(val) {
                                        theme.ansi_16[idx] = c;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        theme
    }
}

fn extract_key_val(line: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = line.splitn(2, '=').collect();
    if parts.len() != 2 {
        return None;
    }
    let key = parts[0].trim().trim_start_matches("syntax.").trim();
    let val = parts[1].trim().trim_matches(|c| c == ',' || c == ';' || c == ' ' || c == '"' || c == '\'');
    if val.starts_with('#') {
        Some((key.to_string(), val.to_string()))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_presets() {
        assert_eq!(Theme::from_name("kanagawa").name, "kanagawa");
        assert_eq!(Theme::from_name("tokyo_night").name, "tokyo_night");
        assert_eq!(Theme::from_name("catppuccin").name, "catppuccin");
        assert_eq!(Theme::from_name("gruvbox").name, "gruvbox");
        assert_eq!(Theme::from_name("nord").name, "nord");
    }

    #[test]
    fn test_parse_theme_lua() {
        let lua = r##"
            return {
                theme = "catppuccin",
                colors = {
                    selection = "#123456",
                }
            }
        "##;
        let t = Theme::from_lua_content(lua);
        assert_eq!(t.name, "catppuccin");
        assert_eq!(t.selection, ColorRgb::new(0x12, 0x34, 0x56));
    }
}
