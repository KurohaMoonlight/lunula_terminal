use mlua::{Lua, Table, Value};
use std::fs;
use std::path::{Path, PathBuf};

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
            width: None, // None = auto dynamically calculated from monitor
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
}

impl Default for WobbleConfig {
    fn default() -> Self {
        Self {
            stiffness: 420.0,
            damping: 7.0,
            max_deform: 55.0,
            spring_shear: 300.0,
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
    pub shape: String, // "block", "beam", "underline"
    pub blinking: bool,
    pub color: [u8; 3], // RGB
    pub trail: bool,
    pub trail_stiffness: f32,
    pub trail_damping: f32,
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self {
            shape: "block".to_string(),
            blinking: true,
            color: [180, 190, 254], // Catppuccin Lavender (#b4befe)
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
            selection: [88, 91, 112], // Catppuccin Mocha Surface 2 (#585b70)
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
}

pub const DEFAULT_LUA_CONFIG: &str = r##"-- ==========================================================
--  Lunula Terminal Configuration (lunula.lua)
--  Customizable physics, visuals, fonts, cursor, and hot reload
-- ==========================================================

return {
    window = {
        -- Lebar & tinggi: "auto" atau angka pixel (e.g. 1100, 700)
        -- Jika "auto", Lunula otomatis menyesuaikan resolusi monitor aktif.
        width = "auto",
        height = "auto",
        width_percent = 0.62,   -- 62% lebar layar saat auto
        height_percent = 0.68,  -- 68% tinggi layar saat auto
        lock_size = false,      -- Jika true, ukuran tidak otomatis menyesuaikan layar
        opacity = 0.95,         -- Transparansi jelly window (0.1 - 1.0)
        title = "Lunula Terminal",
    },
    wobble = {
        -- Kekakuan spring: Lebih tinggi = lebih kaku, Lebih rendah = lebih bergetar/jelly
        stiffness = 420.0,
        -- Damping: Kecepatan getaran berhenti
        damping = 7.0,
        -- Deformasi elastis maksimal saat drag window (pixel)
        max_deform = 55.0,
        -- Shear diagonal stiffness
        spring_shear = 300.0,
    },
    terminal = {
        font_family = "JetBrainsMono Nerd Font",
        font_size = 14.5,
        max_scrollback = 5000,
    },
    cursor = {
        -- Bentuk kursor default: "block", "beam", atau "underline"
        shape = "block",
        -- Efek berkedip lembut (sine smooth blinking)
        blinking = true,
        -- Warna kursor (hex atau {r, g, b})
        color = "#b4befe", -- Catppuccin Lavender
        -- Kitty-style elastic cursor trail (ekor smear elastis saat kursor melompat)
        trail = true,
        trail_stiffness = 550.0,
        trail_damping = 28.0,
    },
    scroll = {
        -- Smooth sub-pixel scrolling (interpolasi pixel Y-offset 60-120 FPS)
        smooth = true,
        speed = 60.0,
    },
    tabs = {
        -- Tampilkan bar tab meski hanya ada 1 tab (false = sembunyikan jika hanya 1 tab)
        show_single_tab = false,
        -- Tinggi tab bar dalam pixel (default: 32.0)
        height = 32.0,
        -- Efek redup / dimming pada split pane yang tidak aktif
        dim_inactive_panes = true,
        -- Tingkat kegelapan redup pada pane tidak aktif (0.0 = terang normal, 0.35 = redup elegan)
        dim_opacity = 0.35,
    },
    colors = {
        -- Warna highlight teks yang di-select mouse
        selection = "#585b70",
    },
}
"##;

impl LunulaConfig {
    pub fn find_config_path() -> Option<PathBuf> {
        let local = Path::new("lunula.lua");
        if local.exists() {
            return Some(local.to_path_buf());
        }

        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                let exe_local = parent.join("lunula.lua");
                if exe_local.exists() {
                    return Some(exe_local);
                }
            }
        }

        if let Ok(appdata) = std::env::var("APPDATA") {
            let appdata_cfg = Path::new(&appdata).join("lunula").join("config.lua");
            if appdata_cfg.exists() {
                return Some(appdata_cfg);
            }
        }

        if let Ok(userprofile) = std::env::var("USERPROFILE") {
            let user_cfg = Path::new(&userprofile)
                .join(".config")
                .join("lunula")
                .join("config.lua");
            if user_cfg.exists() {
                return Some(user_cfg);
            }
        }

        None
    }

    pub fn load() -> Self {
        let config_path = match Self::find_config_path() {
            Some(path) => path,
            None => {
                let _ = fs::write("lunula.lua", DEFAULT_LUA_CONFIG);
                PathBuf::from("lunula.lua")
            }
        };

        match fs::read_to_string(&config_path) {
            Ok(script) => match Self::parse_lua(&script) {
                Ok(cfg) => {
                    log::info!("Loaded configuration from {}", config_path.display());
                    cfg
                }
                Err(err) => {
                    log::warn!(
                        "Error parsing {}: {}. Using default configuration.",
                        config_path.display(),
                        err
                    );
                    Self::default()
                }
            },
            Err(err) => {
                log::warn!("Could not read configuration file: {}. Using default.", err);
                Self::default()
            }
        }
    }

    pub fn parse_lua(script: &str) -> mlua::Result<Self> {
        let lua = Lua::new();
        let value: Value = lua.load(script).eval()?;

        let mut config = Self::default();

        if let Value::Table(table) = value {
            // Window table
            if let Ok(w_val) = table.get::<Table>("window") {
                if let Ok(val) = w_val.get::<Value>("width") {
                    match val {
                        Value::Integer(w) if w > 0 => config.window.width = Some(w as u32),
                        Value::Number(w) if w > 0.0 => config.window.width = Some(w as u32),
                        _ => config.window.width = None,
                    }
                }
                if let Ok(val) = w_val.get::<Value>("height") {
                    match val {
                        Value::Integer(h) if h > 0 => config.window.height = Some(h as u32),
                        Value::Number(h) if h > 0.0 => config.window.height = Some(h as u32),
                        _ => config.window.height = None,
                    }
                }
                if let Ok(wp) = w_val.get::<f32>("width_percent") {
                    config.window.width_percent = wp.clamp(0.2, 1.0);
                }
                if let Ok(hp) = w_val.get::<f32>("height_percent") {
                    config.window.height_percent = hp.clamp(0.2, 1.0);
                }
                if let Ok(lock) = w_val.get::<bool>("lock_size") {
                    config.window.lock_size = lock;
                }
                if let Ok(op) = w_val.get::<f32>("opacity") {
                    config.window.opacity = op.clamp(0.1, 1.0);
                }
                if let Ok(title) = w_val.get::<String>("title") {
                    config.window.title = title;
                }
            }

            // Wobble table
            if let Ok(w_val) = table.get::<Table>("wobble") {
                if let Ok(stiffness) = w_val.get::<f32>("stiffness") {
                    config.wobble.stiffness = stiffness.max(10.0);
                }
                if let Ok(damping) = w_val.get::<f32>("damping") {
                    config.wobble.damping = damping.max(0.1);
                }
                if let Ok(max_deform) = w_val.get::<f32>("max_deform") {
                    config.wobble.max_deform = max_deform.max(5.0);
                }
                if let Ok(spring_shear) = w_val.get::<f32>("spring_shear") {
                    config.wobble.spring_shear = spring_shear.max(10.0);
                }
            }

            // Terminal table
            if let Ok(t_val) = table.get::<Table>("terminal") {
                if let Ok(ff) = t_val.get::<String>("font_family") {
                    config.terminal.font_family = ff;
                }
                if let Ok(fs) = t_val.get::<f32>("font_size") {
                    config.terminal.font_size = fs.max(6.0);
                }
                if let Ok(sb) = t_val.get::<usize>("max_scrollback") {
                    config.terminal.max_scrollback = sb.max(100);
                }
            }

            // Cursor table
            if let Ok(cur_val) = table.get::<Table>("cursor") {
                if let Ok(s) = cur_val.get::<String>("shape") {
                    config.cursor.shape = s.to_lowercase();
                }
                if let Ok(b) = cur_val.get::<bool>("blinking") {
                    config.cursor.blinking = b;
                }
                if let Ok(val) = cur_val.get::<Value>("color") {
                    if let Some(rgb) = parse_color_value(&val) {
                        config.cursor.color = rgb;
                    }
                }
                if let Ok(t) = cur_val.get::<bool>("trail") {
                    config.cursor.trail = t;
                }
                if let Ok(stiff) = cur_val.get::<f32>("trail_stiffness") {
                    config.cursor.trail_stiffness = stiff.max(10.0);
                }
                if let Ok(damp) = cur_val.get::<f32>("trail_damping") {
                    config.cursor.trail_damping = damp.max(1.0);
                }
            }

            // Scroll table
            if let Ok(sc_val) = table.get::<Table>("scroll") {
                if let Ok(sm) = sc_val.get::<bool>("smooth") {
                    config.scroll.smooth = sm;
                }
                if let Ok(sp) = sc_val.get::<f32>("speed") {
                    config.scroll.speed = sp.max(5.0);
                }
            }

            // Tabs table
            if let Ok(tb_val) = table.get::<Table>("tabs") {
                if let Ok(single) = tb_val.get::<bool>("show_single_tab") {
                    config.tabs.show_single_tab = single;
                }
                if let Ok(h) = tb_val.get::<f32>("height") {
                    config.tabs.height = h.clamp(20.0, 60.0);
                }
                if let Ok(dim) = tb_val.get::<bool>("dim_inactive_panes") {
                    config.tabs.dim_inactive_panes = dim;
                }
                if let Ok(op) = tb_val.get::<f32>("dim_opacity") {
                    config.tabs.dim_opacity = op.clamp(0.0, 0.9);
                }
            }

            // Colors table
            if let Ok(c_val) = table.get::<Table>("colors") {
                if let Ok(val) = c_val.get::<Value>("selection") {
                    if let Some(rgb) = parse_color_value(&val) {
                        config.colors.selection = rgb;
                    }
                }
            }
        }

        Ok(config)
    }
}

fn parse_color_value(val: &Value) -> Option<[u8; 3]> {
    match val {
        Value::String(s) => {
            let str_val = s.to_str().ok()?;
            let hex = str_val.trim_start_matches('#');
            if hex.len() == 6 {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some([r, g, b])
            } else {
                None
            }
        }
        Value::Table(t) => {
            let r: u8 = t.get(1).ok()?;
            let g: u8 = t.get(2).ok()?;
            let b: u8 = t.get(3).ok()?;
            Some([r, g, b])
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_default_config() {
        let cfg = LunulaConfig::parse_lua(DEFAULT_LUA_CONFIG).expect("parse default");
        assert_eq!(cfg.window.width, None); // auto
        assert_eq!(cfg.window.height, None); // auto
        assert_eq!(cfg.window.width_percent, 0.62);
        assert!((cfg.window.opacity - 0.95).abs() < 1e-4);
        assert_eq!(cfg.wobble.stiffness, 420.0);
        assert_eq!(cfg.terminal.font_family, "JetBrainsMono Nerd Font");
        assert_eq!(cfg.terminal.font_size, 14.5);
        assert_eq!(cfg.cursor.shape, "block");
        assert!(cfg.cursor.trail);
        assert_eq!(cfg.colors.selection, [88, 91, 112]);
    }

    #[test]
    fn test_parse_custom_config() {
        let script = r##"
        return {
            window = { width = 1280, height = 720, opacity = 0.88, title = "Custom Lunula", lock_size = true },
            wobble = { stiffness = 250.0, damping = 12.0 },
            terminal = { font_size = 18.0, max_scrollback = 10000 },
            cursor = { shape = "beam", blinking = false, color = "#ff79c6" },
            colors = { selection = "#ff5577" },
        }
        "##;
        let cfg = LunulaConfig::parse_lua(script).expect("parse custom");
        assert_eq!(cfg.window.width, Some(1280));
        assert_eq!(cfg.window.height, Some(720));
        assert!(cfg.window.lock_size);
        assert!((cfg.window.opacity - 0.88).abs() < 1e-4);
        assert_eq!(cfg.window.title, "Custom Lunula");
        assert_eq!(cfg.wobble.stiffness, 250.0);
        assert_eq!(cfg.wobble.damping, 12.0);
        assert_eq!(cfg.terminal.font_size, 18.0);
        assert_eq!(cfg.terminal.max_scrollback, 10000);
        assert_eq!(cfg.cursor.shape, "beam");
        assert!(!cfg.cursor.blinking);
        assert_eq!(cfg.cursor.color, [255, 121, 198]);
        assert_eq!(cfg.colors.selection, [255, 85, 119]);
    }

    #[test]
    fn test_parse_partial_config_falls_back() {
        let script = r#"
        return {
            window = { width = 1100 }
        }
        "#;
        let cfg = LunulaConfig::parse_lua(script).expect("parse partial");
        assert_eq!(cfg.window.width, Some(1100));
        assert_eq!(cfg.window.height, None); // default auto
        assert_eq!(cfg.wobble.stiffness, 420.0); // default
    }
}
