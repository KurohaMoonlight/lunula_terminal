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
}

impl Default for ColorsConfig {
    fn default() -> Self {
        Self {
            selection: [45, 79, 103], // Kanagawa waveBlue2 (#2d4f67)
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

#[derive(Clone, Debug, Default)]
pub struct LunulaConfig {
    pub window: WindowConfig,
    pub wobble: WobbleConfig,
    pub terminal: TerminalConfig,
    pub cursor: CursorConfig,
    pub scroll: ScrollConfig,
    pub tabs: TabsConfig,
    pub colors: ColorsConfig,
    pub fetch: FetchConfig,
}
