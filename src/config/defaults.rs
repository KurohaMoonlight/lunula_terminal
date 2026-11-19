//! Default Lua configuration template and color parsing helpers.

use mlua::Value;

pub const DEFAULT_LUA_CONFIG: &str = r##"-- ==========================================================
--  Lunula Terminal Configuration (lunula.lua)
--  Customizable physics, visuals, fonts, cursor, and hot reload
-- ==========================================================

return {
    -- Pilihan tema tampilan terminal & luna editor:
    -- "kanagawa" (default), "tokyo_night", "catppuccin", "gruvbox", "nord"
    theme = "kanagawa",

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
        animate_titlebar = true, -- Animasi 60 FPS gelombang ombak Kanagawa di titlebar (false = hemat baterai laptop)
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
        -- Reaktivitas ombak Kanagawa di titlebar saat window digeser (0 = mati, 1 = normal, maks 3)
        wave_reactivity = 1.0,
    },
    terminal = {
        font_family = "0xProto Nerd Font Mono",
        font_size = 14.5,
        max_scrollback = 5000,
        -- Keamanan: Izinkan aplikasi PTY/remote membaca clipboard via OSC 52 query (default: false demi keamanan)
        allow_clipboard_read = false,
    },
    cursor = {
        -- Bentuk kursor default: "block", "beam", atau "underline"
        shape = "block",
        -- Efek berkedip lembut (sine smooth blinking)
        blinking = true,
        -- Warna kursor (hex atau {r, g, b})
        color = "#7fb4ca", -- Kanagawa springBlue
        -- Kitty-style elastic cursor trail (ekor smear elastis saat kursor melompat)
        trail = true,
        trail_stiffness = 550.0,
        trail_damping = 48.0,
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
        selection = "#2d4f67", -- Kanagawa waveBlue2
    },
    fetch = {
        -- Pilihan animasi lfetch: 1 atau 2
        -- 1: 3D Raymarched Moon & Great Wave Breaking Crest + Foam Claw (38x15)
        -- 2: 3D Analytical Moon dengan kawah berotasi & rolling Kanagawa swells (44x17)
        animation = 1,
        -- Mode default: true = animasi interaktif live, false = cetak 1 frame instan (< 5ms)
        live = true,
    },
    shell = {
        -- Program shell utama (e.g. "powershell.exe", "pwsh.exe", "cmd.exe", "wsl.exe", "bash.exe")
        program = "powershell.exe",
        -- Argumen tambahan shell
        args = { "-NoLogo" },
    },
}
"##;

pub const DEFAULT_COLORS_LUA: &str = r##"-- ==========================================================
--  Lunula Terminal - Custom Colors & 16-Color ANSI Palette
--  (colors.lua)
-- ==========================================================

return {
    -- Override warna UI utama (jika diisi, menimpa theme aktif):
    -- background = "#1F1F28",
    -- foreground = "#DCD7BA",
    -- selection  = "#2D4F67",
    -- cursor     = "#7FB4CA",
    -- tab_bar    = "#16161D",
    -- border     = "#363646",

    -- 16 ANSI Palette Colors (0..15):
    -- Menentukan warna output perintah CLI di shell (ls, git, htop, dll.)
    ansi = {
        [0]  = "#16161D", -- Black (sumiInk0)
        [1]  = "#C34043", -- Red (autumnRed)
        [2]  = "#76946A", -- Green (autumnGreen)
        [3]  = "#C0A36E", -- Yellow (boatYellow1)
        [4]  = "#7E9CD8", -- Blue (crystalBlue)
        [5]  = "#957FB8", -- Magenta (oniViolet)
        [6]  = "#6A9589", -- Cyan (waveAqua1)
        [7]  = "#C8C093", -- White (oldWhite)
        [8]  = "#727169", -- Bright Black (fujiGray)
        [9]  = "#E82424", -- Bright Red (samuraiRed)
        [10] = "#98BB6C", -- Bright Green (springGreen)
        [11] = "#E6C384", -- Bright Yellow (boatYellow2)
        [12] = "#7FB4CA", -- Bright Blue (springBlue)
        [13] = "#D27E99", -- Bright Magenta (sakuraPink)
        [14] = "#7AA89F", -- Bright Cyan (waveAqua2)
        [15] = "#DCD7BA", -- Bright White (fujiWhite)
    },
}
"##;

pub const DEFAULT_KEYBINDS_LUA: &str = r##"-- ==========================================================
--  Lunula Terminal - Keyboard Shortcuts Configuration
--  (keybinds.lua)
-- ==========================================================

return {
    -- Format: ["Shortcut"] = "Action"
    -- Actions: "new_tab", "close_pane", "next_tab", "prev_tab",
    --          "split_vertical", "split_horizontal", "search",
    --          "reload_config", "toggle_fullscreen", "toggle_help", "toggle_settings"

    ["Ctrl+Shift+T"]   = "new_tab",
    ["Ctrl+Shift+W"]   = "close_pane",
    ["Ctrl+Tab"]       = "next_tab",
    ["Ctrl+Shift+Tab"] = "prev_tab",
    ["Ctrl+Shift+E"]   = "split_vertical",
    ["Ctrl+Shift+D"]   = "split_horizontal",
    ["Ctrl+Shift+F"]   = "search",
    ["Ctrl+Shift+R"]   = "reload_config",
    ["Ctrl+Shift+H"]   = "toggle_help",
    ["Ctrl+,"]         = "toggle_settings",
    ["F11"]            = "toggle_fullscreen",
}
"##;

pub const DEFAULT_CYBERPUNK_THEME_LUA: &str = r##"-- ==========================================================
--  Custom Theme: Cyberpunk Neon
--  (themes/cyberpunk.lua)
-- ==========================================================

return {
    name = "cyberpunk",
    bg = "#0f051d",
    fg = "#00ff9f",
    selection = "#ff007f",
    cursor = "#00e5ff",
    tab_bar = "#090212",
    tab_active = "#1e0b36",
    border = "#ff007f",
    syntax = {
        keyword = "#ff007f",
        type = "#00e5ff",
        string = "#ffe600",
        number = "#ff8c00",
        comment = "#5c5670",
        punctuation = "#e0e0e0",
        header = "#ff007f",
    },
    ansi = {
        [0]  = "#0f051d",
        [1]  = "#ff0055",
        [2]  = "#00ff9f",
        [3]  = "#ffe600",
        [4]  = "#00e5ff",
        [5]  = "#ff007f",
        [6]  = "#00ffff",
        [7]  = "#ffffff",
        [8]  = "#5c5670",
        [9]  = "#ff2a6d",
        [10] = "#05d9e8",
        [11] = "#ffea00",
        [12] = "#00f0ff",
        [13] = "#f706cf",
        [14] = "#01f9c6",
        [15] = "#fdfdfd",
    },
}
"##;

/// Parses a Lua color value given either as `"#RRGGBB"` string or `{r, g, b}` table.
pub fn parse_color_value(val: &Value) -> Option<[u8; 3]> {
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
