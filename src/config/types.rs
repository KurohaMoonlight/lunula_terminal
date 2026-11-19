//! Data structures representing strongly-typed sections of `lunula.lua`.

#[derive(Clone, Debug)]
pub struct WindowConfig {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub width_percent: f32,
    pub height_percent: f32,
    pub lock_size: bool,
    pub opacity: f32,
    pub title: String,
    pub animate_titlebar: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            width_percent: 0.62,
            height_percent: 0.68,
            lock_size: false,
            opacity: 0.95,
            title: "Lunula Terminal".to_string(),
            animate_titlebar: true,
        }
    }
}

impl WindowConfig {
    pub fn get_effective_dimensions(&self, monitor_w: u32, monitor_h: u32) -> (u32, u32) {
        if self.lock_size {
            (self.width.unwrap_or(980), self.height.unwrap_or(640))
        } else {
            let def_w = ((monitor_w as f32) * self.width_percent).round() as u32;
            let def_h = ((monitor_h as f32) * self.height_percent).round() as u32;
            (
                self.width.unwrap_or(def_w.max(640)),
                self.height.unwrap_or(def_h.max(480)),
            )
        }
    }
}

#[derive(Clone, Debug)]
pub struct WobbleConfig {
    pub stiffness: f32,
    pub damping: f32,
    pub max_deform: f32,
    pub spring_shear: f32,
    /// Seberapa kuat ombak Kanagawa di titlebar bereaksi saat window digeser (0 = mati).
    pub wave_reactivity: f32,
}

impl Default for WobbleConfig {
    fn default() -> Self {
        Self {
            stiffness: 420.0,
            damping: 7.0,
            max_deform: 55.0,
            spring_shear: 300.0,
            wave_reactivity: 1.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TerminalConfig {
    pub font_family: String,
    pub font_size: f32,
    pub max_scrollback: usize,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            font_family: "JetBrainsMono Nerd Font".to_string(),
            font_size: 14.5,
            max_scrollback: 5000,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CursorConfig {
    pub shape: String,
    pub blinking: bool,
    pub color: [u8; 3],
    pub trail: bool,
    pub trail_stiffness: f32,
    pub trail_damping: f32,
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self {
            shape: "block".to_string(),
            blinking: true,
            color: [127, 180, 202], // Kanagawa springBlue (#7fb4ca)
            trail: true,
            trail_stiffness: 550.0,
            trail_damping: 28.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ScrollConfig {
    pub smooth: bool,
    pub speed: f32,
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self {
            smooth: true,
            speed: 60.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TabsConfig {
    pub show_single_tab: bool,
    pub height: f32,
    pub dim_inactive_panes: bool,
    pub dim_opacity: f32,
}

impl Default for TabsConfig {
    fn default() -> Self {
        Self {
            show_single_tab: false,
            height: 32.0,
            dim_inactive_panes: true,
            dim_opacity: 0.35,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ColorsConfig {
    pub selection: [u8; 3],
    pub background: Option<[u8; 3]>,
    pub foreground: Option<[u8; 3]>,
}

impl Default for ColorsConfig {
    fn default() -> Self {
        Self {
            selection: [45, 79, 103], // Kanagawa waveBlue2 (#2d4f67)
            background: None,
            foreground: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FetchConfig {
    pub animation: u8,
    pub live: bool,
}

impl Default for FetchConfig {
    fn default() -> Self {
        Self {
            animation: 1,
            live: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeybindsConfig {
    pub new_tab: String,
    pub close_pane: String,
    pub next_tab: String,
    pub prev_tab: String,
    pub split_vertical: String,
    pub split_horizontal: String,
    pub search: String,
    pub reload_config: String,
    pub reload_profile: String,
    pub toggle_help: String,
    pub toggle_settings: String,
    pub toggle_fullscreen: String,
}

impl Default for KeybindsConfig {
    fn default() -> Self {
        Self {
            new_tab: "Ctrl+Shift+T".to_string(),
            close_pane: "Ctrl+Shift+W".to_string(),
            next_tab: "Ctrl+Tab".to_string(),
            prev_tab: "Ctrl+Shift+Tab".to_string(),
            split_vertical: "Ctrl+Shift+E".to_string(),
            split_horizontal: "Ctrl+Shift+D".to_string(),
            search: "Ctrl+Shift+F".to_string(),
            reload_config: "Ctrl+Shift+R".to_string(),
            reload_profile: "Ctrl+Shift+P".to_string(),
            toggle_help: "Shift+F1".to_string(),
            toggle_settings: "Ctrl+,".to_string(),
            toggle_fullscreen: "F11".to_string(),
        }
    }
}

pub fn matches_shortcut(
    shortcut_str: &str,
    ctrl: bool,
    shift: bool,
    alt: bool,
    code_opt: Option<winit::keyboard::KeyCode>,
    ch_opt: Option<char>,
) -> bool {
    let parts: Vec<&str> = shortcut_str.split('+').map(|s| s.trim()).collect();
    if parts.is_empty() {
        return false;
    }

    let mut req_ctrl = false;
    let mut req_shift = false;
    let mut req_alt = false;
    let mut key_part = "";

    for part in parts {
        let lower = part.to_lowercase();
        match lower.as_str() {
            "ctrl" | "control" => req_ctrl = true,
            "shift" => req_shift = true,
            "alt" => req_alt = true,
            _ => key_part = part,
        }
    }

    if req_ctrl != ctrl || req_shift != shift || req_alt != alt {
        return false;
    }

    let key_lower = key_part.to_lowercase();
    match key_lower.as_str() {
        "tab" => code_opt == Some(winit::keyboard::KeyCode::Tab) || ch_opt == Some('\t'),
        "f1" => code_opt == Some(winit::keyboard::KeyCode::F1),
        "f11" => code_opt == Some(winit::keyboard::KeyCode::F11),
        "escape" | "esc" => {
            code_opt == Some(winit::keyboard::KeyCode::Escape) || ch_opt == Some('\x1b')
        }
        "enter" | "return" => {
            code_opt == Some(winit::keyboard::KeyCode::Enter)
                || code_opt == Some(winit::keyboard::KeyCode::NumpadEnter)
                || ch_opt == Some('\r')
                || ch_opt == Some('\n')
        }
        "," | "comma" => {
            ch_opt == Some(',') || code_opt == Some(winit::keyboard::KeyCode::Comma)
        }
        "space" => {
            code_opt == Some(winit::keyboard::KeyCode::Space) || ch_opt == Some(' ')
        }
        single if single.chars().count() == 1 => {
            let target_char = single.chars().next().unwrap();
            if let Some(c) = ch_opt {
                if c.to_ascii_lowercase() == target_char.to_ascii_lowercase() {
                    return true;
                }
                // Handle ASCII control characters when Ctrl is held (1..=26 -> 'a'..='z')
                let byte = c as u32;
                if (1..=26).contains(&byte) {
                    let ctrl_char = (b'a' + (byte as u8) - 1) as char;
                    if ctrl_char == target_char.to_ascii_lowercase() {
                        return true;
                    }
                }
            }
            if target_char.is_ascii_alphabetic() {
                match target_char.to_ascii_uppercase() {
                    'A' => code_opt == Some(winit::keyboard::KeyCode::KeyA),
                    'B' => code_opt == Some(winit::keyboard::KeyCode::KeyB),
                    'C' => code_opt == Some(winit::keyboard::KeyCode::KeyC),
                    'D' => code_opt == Some(winit::keyboard::KeyCode::KeyD),
                    'E' => code_opt == Some(winit::keyboard::KeyCode::KeyE),
                    'F' => code_opt == Some(winit::keyboard::KeyCode::KeyF),
                    'G' => code_opt == Some(winit::keyboard::KeyCode::KeyG),
                    'H' => code_opt == Some(winit::keyboard::KeyCode::KeyH),
                    'I' => code_opt == Some(winit::keyboard::KeyCode::KeyI),
                    'J' => code_opt == Some(winit::keyboard::KeyCode::KeyJ),
                    'K' => code_opt == Some(winit::keyboard::KeyCode::KeyK),
                    'L' => code_opt == Some(winit::keyboard::KeyCode::KeyL),
                    'M' => code_opt == Some(winit::keyboard::KeyCode::KeyM),
                    'N' => code_opt == Some(winit::keyboard::KeyCode::KeyN),
                    'O' => code_opt == Some(winit::keyboard::KeyCode::KeyO),
                    'P' => code_opt == Some(winit::keyboard::KeyCode::KeyP),
                    'Q' => code_opt == Some(winit::keyboard::KeyCode::KeyQ),
                    'R' => code_opt == Some(winit::keyboard::KeyCode::KeyR),
                    'S' => code_opt == Some(winit::keyboard::KeyCode::KeyS),
                    'T' => code_opt == Some(winit::keyboard::KeyCode::KeyT),
                    'U' => code_opt == Some(winit::keyboard::KeyCode::KeyU),
                    'V' => code_opt == Some(winit::keyboard::KeyCode::KeyV),
                    'W' => code_opt == Some(winit::keyboard::KeyCode::KeyW),
                    'X' => code_opt == Some(winit::keyboard::KeyCode::KeyX),
                    'Y' => code_opt == Some(winit::keyboard::KeyCode::KeyY),
                    'Z' => code_opt == Some(winit::keyboard::KeyCode::KeyZ),
                    _ => false,
                }
            } else {
                false
            }
        }
        _ => false,
    }
}

#[derive(Clone, Debug)]
pub struct ShellConfig {
    pub program: String,
    pub args: Vec<String>,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            program: "powershell.exe".to_string(),
            args: vec!["-NoLogo".to_string()],
        }
    }
}

#[derive(Clone, Debug)]
pub struct LunulaConfig {
    pub theme: String,
    pub window: WindowConfig,
    pub wobble: WobbleConfig,
    pub terminal: TerminalConfig,
    pub cursor: CursorConfig,
    pub scroll: ScrollConfig,
    pub tabs: TabsConfig,
    pub colors: ColorsConfig,
    pub keybinds: KeybindsConfig,
    pub fetch: FetchConfig,
    pub shell: ShellConfig,
}

impl Default for LunulaConfig {
    fn default() -> Self {
        Self {
            theme: "kanagawa".to_string(),
            window: WindowConfig::default(),
            wobble: WobbleConfig::default(),
            terminal: TerminalConfig::default(),
            cursor: CursorConfig::default(),
            scroll: ScrollConfig::default(),
            tabs: TabsConfig::default(),
            colors: ColorsConfig::default(),
            keybinds: KeybindsConfig::default(),
            fetch: FetchConfig::default(),
            shell: ShellConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::KeyCode;

    #[test]
    fn test_matches_shortcut_basic() {
        assert!(matches_shortcut("Ctrl+Shift+T", true, true, false, Some(KeyCode::KeyT), None));
        assert!(matches_shortcut("Ctrl+Shift+T", true, true, false, None, Some('t')));
        assert!(matches_shortcut("Ctrl+Shift+T", true, true, false, None, Some('\x14')));
        assert!(!matches_shortcut("Ctrl+Shift+T", true, false, false, Some(KeyCode::KeyT), None));
        assert!(matches_shortcut("Ctrl+,", true, false, false, Some(KeyCode::Comma), None));
        assert!(matches_shortcut("Shift+F1", false, true, false, Some(KeyCode::F1), None));
        assert!(matches_shortcut("F11", false, false, false, Some(KeyCode::F11), None));
        assert!(matches_shortcut("Ctrl+Tab", true, false, false, Some(KeyCode::Tab), None));
        assert!(matches_shortcut("Ctrl+Shift+Tab", true, true, false, Some(KeyCode::Tab), None));
    }
}
