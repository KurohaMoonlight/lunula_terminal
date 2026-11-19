//! Default Lua configuration template and color parsing helpers.

use mlua::Value;

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
