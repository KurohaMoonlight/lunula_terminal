//! Lua-based configuration loader (`lunula.lua`) with fallback resolution and hot-reload support.

pub mod defaults;
pub mod types;

pub use defaults::{
    DEFAULT_COLORS_LUA, DEFAULT_CYBERPUNK_THEME_LUA, DEFAULT_KEYBINDS_LUA, DEFAULT_LUA_CONFIG,
};
pub use types::*;

use defaults::parse_color_value;
use mlua::{Lua, Table, Value};
use std::fs;
use std::path::{Path, PathBuf};

impl LunulaConfig {
    /// Searches standard locations for `lunula.lua` or `config.lua` and returns canonical absolute path.
    pub fn find_config_path() -> PathBuf {
        let local = Path::new("lunula.lua");
        if local.exists() {
            if let Ok(canon) = local.canonicalize() {
                return canon;
            }
            if let Ok(cur) = std::env::current_dir() {
                return cur.join("lunula.lua");
            }
            return local.to_path_buf();
        }

        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                let exe_local = parent.join("lunula.lua");
                if exe_local.exists() {
                    if let Ok(canon) = exe_local.canonicalize() {
                        return canon;
                    }
                    return exe_local;
                }
            }
        }

        if let Ok(appdata) = std::env::var("APPDATA") {
            let appdata_cfg = Path::new(&appdata).join("lunula").join("config.lua");
            if appdata_cfg.exists() {
                if let Ok(canon) = appdata_cfg.canonicalize() {
                    return canon;
                }
                return appdata_cfg;
            }
        }

        if let Ok(userprofile) = std::env::var("USERPROFILE") {
            let user_cfg = Path::new(&userprofile)
                .join(".config")
                .join("lunula")
                .join("config.lua");
            if user_cfg.exists() {
                if let Ok(canon) = user_cfg.canonicalize() {
                    return canon;
                }
                return user_cfg;
            }
        }

        if let Ok(cur) = std::env::current_dir() {
            return cur.join("lunula.lua");
        }

        PathBuf::from("lunula.lua")
    }

    /// Returns the absolute folder path containing `lunula.lua`.
    #[allow(dead_code)]
    pub fn get_config_dir() -> PathBuf {
        let cfg = Self::find_config_path();
        cfg.parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Finds absolute path of `luna.exe` companion, falling back to PATH `"luna"`.
    pub fn get_luna_exe() -> PathBuf {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let candidate = dir.join("luna.exe");
                if candidate.exists() {
                    return candidate;
                }
                let candidate2 = dir.join("luna");
                if candidate2.exists() {
                    return candidate2;
                }
            }
        }
        PathBuf::from("luna")
    }

    /// Loads `lunula.lua` from disk, or generates the default configuration file if missing.
    pub fn load() -> Self {
        let config_path = Self::find_config_path();
        if !config_path.exists() {
            let _ = fs::write(&config_path, DEFAULT_LUA_CONFIG);
        }

        let mut config = match fs::read_to_string(&config_path) {
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
        };

        // Companion keybinds.lua loader
        if let Some(config_dir) = config_path.parent() {
            let keybinds_path = config_dir.join("keybinds.lua");
            if keybinds_path.exists() {
                if let Ok(content) = fs::read_to_string(&keybinds_path) {
                    Self::parse_keybinds_content(&content, &mut config.keybinds);
                }
            }
        }

        config
    }

    /// Fast parser for keybindings content from Lua script.
    pub fn parse_keybinds_content(content: &str, keybinds: &mut KeybindsConfig) {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("--") {
                continue;
            }
            let parts: Vec<&str> = trimmed.splitn(2, '=').collect();
            if parts.len() == 2 {
                let k = parts[0].trim().trim_matches(|c| c == '[' || c == ']' || c == '"' || c == '\'');
                let v = parts[1].trim().trim_matches(|c| c == ',' || c == ';' || c == ' ' || c == '"' || c == '\'');
                if !k.is_empty() && !v.is_empty() {
                    apply_keybind_pair(keybinds, k, v);
                }
            }
        }
    }

    /// Evaluates a Lua configuration script into a validated `LunulaConfig`.
    pub fn parse_lua(script: &str) -> mlua::Result<Self> {
        let lua = Lua::new();
        let value: Value = lua.load(script).eval()?;

        let mut config = Self::default();

        if let Value::Table(table) = value {
            if let Ok(theme_str) = table.get::<String>("theme") {
                if !theme_str.trim().is_empty() {
                    config.theme = theme_str;
                }
            }

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
                if let Ok(anim) = w_val.get::<bool>("animate_titlebar") {
                    config.window.animate_titlebar = anim;
                }
            }

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
                if let Ok(wave_reactivity) = w_val.get::<f32>("wave_reactivity") {
                    config.wobble.wave_reactivity = wave_reactivity.clamp(0.0, 3.0);
                }
            }

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
                if let Ok(acr) = t_val.get::<bool>("allow_clipboard_read") {
                    config.terminal.allow_clipboard_read = acr;
                }
            }

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

            if let Ok(sc_val) = table.get::<Table>("scroll") {
                if let Ok(sm) = sc_val.get::<bool>("smooth") {
                    config.scroll.smooth = sm;
                }
                if let Ok(sp) = sc_val.get::<f32>("speed") {
                    config.scroll.speed = sp.max(5.0);
                }
            }

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

            if let Ok(c_val) = table.get::<Table>("colors") {
                if let Ok(val) = c_val.get::<Value>("selection") {
                    if let Some(rgb) = parse_color_value(&val) {
                        config.colors.selection = rgb;
                    }
                }
                if let Ok(val) = c_val.get::<Value>("background") {
                    config.colors.background = parse_color_value(&val);
                }
                if let Ok(val) = c_val.get::<Value>("foreground") {
                    config.colors.foreground = parse_color_value(&val);
                }
            }

            if let Ok(f_val) = table.get::<Table>("fetch") {
                if let Ok(anim) = f_val.get::<u8>("animation") {
                    if anim == 1 || anim == 2 {
                        config.fetch.animation = anim;
                    }
                }
                if let Ok(live) = f_val.get::<bool>("live") {
                    config.fetch.live = live;
                }
            }

            if let Ok(s_val) = table.get::<Table>("shell") {
                if let Ok(prog) = s_val.get::<String>("program") {
                    if !prog.trim().is_empty() {
                        config.shell.program = prog;
                    }
                }
                if let Ok(args_table) = s_val.get::<Table>("args") {
                    let mut parsed_args = Vec::new();
                    for val in args_table.sequence_values::<String>() {
                        if let Ok(arg) = val {
                            parsed_args.push(arg);
                        }
                    }
                    config.shell.args = parsed_args;
                }
            }

            if let Ok(kb_val) = table.get::<Table>("keybinds") {
                for pair in kb_val.pairs::<String, String>() {
                    if let Ok((k, v)) = pair {
                        apply_keybind_pair(&mut config.keybinds, &k, &v);
                    }
                }
            }
        }

        Ok(config)
    }
}

fn apply_keybind_pair(keybinds: &mut KeybindsConfig, k: &str, v: &str) {
    let k_clean = k.trim();
    let v_clean = v.trim();

    let (action, shortcut) = if is_known_action(v_clean) {
        (v_clean, k_clean)
    } else {
        (k_clean, v_clean)
    };

    match action.to_lowercase().as_str() {
        "new_tab" => keybinds.new_tab = shortcut.to_string(),
        "close_pane" => keybinds.close_pane = shortcut.to_string(),
        "next_tab" => keybinds.next_tab = shortcut.to_string(),
        "prev_tab" => keybinds.prev_tab = shortcut.to_string(),
        "split_vertical" => keybinds.split_vertical = shortcut.to_string(),
        "split_horizontal" => keybinds.split_horizontal = shortcut.to_string(),
        "search" => keybinds.search = shortcut.to_string(),
        "reload_config" => keybinds.reload_config = shortcut.to_string(),
        "reload_profile" => keybinds.reload_profile = shortcut.to_string(),
        "toggle_help" => keybinds.toggle_help = shortcut.to_string(),
        "toggle_settings" => keybinds.toggle_settings = shortcut.to_string(),
        "toggle_fullscreen" => keybinds.toggle_fullscreen = shortcut.to_string(),
        _ => {}
    }
}

fn is_known_action(act: &str) -> bool {
    matches!(
        act.to_lowercase().as_str(),
        "new_tab"
            | "close_pane"
            | "next_tab"
            | "prev_tab"
            | "split_vertical"
            | "split_horizontal"
            | "search"
            | "reload_config"
            | "reload_profile"
            | "toggle_help"
            | "toggle_settings"
            | "toggle_fullscreen"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_default_config() {
        let cfg = LunulaConfig::parse_lua(DEFAULT_LUA_CONFIG).expect("parse default");
        assert_eq!(cfg.window.width, None);
        assert_eq!(cfg.window.height, None);
        assert_eq!(cfg.window.width_percent, 0.62);
        assert!((cfg.window.opacity - 0.95).abs() < 1e-4);
        assert_eq!(cfg.wobble.stiffness, 420.0);
        assert_eq!(cfg.terminal.font_family, "0xProto Nerd Font Mono");
        assert_eq!(cfg.terminal.font_size, 14.5);
        assert!(!cfg.terminal.allow_clipboard_read);
        assert_eq!(cfg.cursor.shape, "block");
        assert!(cfg.cursor.trail);
        assert_eq!(cfg.colors.selection, [45, 79, 103]);
        assert_eq!(cfg.fetch.animation, 1);
        assert!(cfg.fetch.live);
    }

    #[test]
    fn test_parse_custom_config() {
        let script = r##"
        return {
            window = { width = 1280, height = 720, opacity = 0.88, title = "Custom Lunula", lock_size = true },
            wobble = { stiffness = 250.0, damping = 12.0 },
            terminal = { font_size = 18.0, max_scrollback = 10000, allow_clipboard_read = true },
            cursor = { shape = "beam", blinking = false, color = "#ff79c6" },
            colors = { selection = "#ff5577" },
            fetch = { animation = 2, live = false },
            shell = { program = "pwsh.exe", args = { "-NoProfile" } },
        }
        "##;
        let cfg = LunulaConfig::parse_lua(script).expect("parse custom");
        assert_eq!(cfg.shell.program, "pwsh.exe");
        assert_eq!(cfg.shell.args, vec!["-NoProfile"]);
        assert_eq!(cfg.window.width, Some(1280));
        assert_eq!(cfg.window.height, Some(720));
        assert!(cfg.window.lock_size);
        assert!((cfg.window.opacity - 0.88).abs() < 1e-4);
        assert_eq!(cfg.window.title, "Custom Lunula");
        assert_eq!(cfg.wobble.stiffness, 250.0);
        assert_eq!(cfg.wobble.damping, 12.0);
        assert_eq!(cfg.terminal.font_size, 18.0);
        assert_eq!(cfg.terminal.max_scrollback, 10000);
        assert!(cfg.terminal.allow_clipboard_read);
        assert_eq!(cfg.cursor.shape, "beam");
        assert!(!cfg.cursor.blinking);
        assert_eq!(cfg.cursor.color, [255, 121, 198]);
        assert_eq!(cfg.colors.selection, [255, 85, 119]);
        assert_eq!(cfg.fetch.animation, 2);
        assert!(!cfg.fetch.live);
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
        assert_eq!(cfg.window.height, None);
        assert_eq!(cfg.wobble.stiffness, 420.0);
    }
}
