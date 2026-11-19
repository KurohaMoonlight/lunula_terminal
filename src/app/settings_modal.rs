//! Settings & Configuration Hub overlay builder (Gear button / `Ctrl+,`).

use crate::render::TextEngine;
use crate::terminal::{CellStyle, ColorRgb};
use crate::theme::Theme;

pub const SETTINGS_ITEMS_COUNT: usize = 6;

pub struct SettingsItem {
    pub key: &'static str,
    pub title: &'static str,
    pub desc: &'static str,
    pub icon: &'static str,
}

pub const SETTINGS_ITEMS: [SettingsItem; SETTINGS_ITEMS_COUNT] = [
    SettingsItem {
        key: "1",
        title: "Edit Master Config",
        desc: "lunula.lua — General, Window, Physics & Shell",
        icon: "📄",
    },
    SettingsItem {
        key: "2",
        title: "Edit Colors & ANSI",
        desc: "colors.lua — 16 ANSI Palette & UI Overrides",
        icon: "🎨",
    },
    SettingsItem {
        key: "3",
        title: "Edit Keybindings",
        desc: "keybinds.lua — Custom keyboard shortcuts",
        icon: "⌨️ ",
    },
    SettingsItem {
        key: "4",
        title: "Explore Themes Folder",
        desc: "themes/ — Create or customize your own themes",
        icon: "🎭",
    },
    SettingsItem {
        key: "5",
        title: "Hot-Reload Configuration",
        desc: "Reload lunula.lua immediately without restarting",
        icon: "🔄",
    },
    SettingsItem {
        key: "6",
        title: "Open Config Directory in Explorer",
        desc: "Browse configuration folder in Windows Explorer",
        icon: "📂",
    },
];

pub fn build_settings_buffer(
    text_engine: &mut TextEngine,
    buffer: &mut glyphon::Buffer,
    selected_idx: usize,
    theme: &Theme,
    config_dir_display: &str,
) -> f32 {
    let mut spans = Vec::with_capacity(64);

    let header_style = CellStyle::new()
        .with_fg(ColorRgb {
            r: theme.syntax_type.r,
            g: theme.syntax_type.g,
            b: theme.syntax_type.b,
        })
        .with_bold(true);

    let sub_style = CellStyle::new()
        .with_fg(ColorRgb {
            r: theme.gutter_fg.r,
            g: theme.gutter_fg.g,
            b: theme.gutter_fg.b,
        })
        .with_dim(true);

    let title_style = CellStyle::new()
        .with_fg(ColorRgb {
            r: theme.fg.r,
            g: theme.fg.g,
            b: theme.fg.b,
        })
        .with_bold(true);

    let desc_style = CellStyle::new().with_fg(ColorRgb {
        r: theme.gutter_fg.r,
        g: theme.gutter_fg.g,
        b: theme.gutter_fg.b,
    });

    let selected_title_style = CellStyle::new()
        .with_fg(ColorRgb {
            r: theme.syntax_keyword.r,
            g: theme.syntax_keyword.g,
            b: theme.syntax_keyword.b,
        })
        .with_bold(true);

    let selected_desc_style = CellStyle::new().with_fg(ColorRgb {
        r: theme.syntax_string.r,
        g: theme.syntax_string.g,
        b: theme.syntax_string.b,
    });

    let key_style = CellStyle::new()
        .with_fg(ColorRgb {
            r: theme.gutter_active.r,
            g: theme.gutter_active.g,
            b: theme.gutter_active.b,
        })
        .with_bold(true);

    let clean_path = config_dir_display
        .strip_prefix(r"\\?\")
        .unwrap_or(config_dir_display);

    spans.push((
        "  ⚙️  LUNULA SETTINGS & CONFIGURATION MANAGER\n".to_string(),
        header_style,
    ));
    spans.push((
        format!("  Config Path: {}\n", clean_path),
        sub_style,
    ));
    spans.push((
        "  Select [1-6], Navigate with ↑ ↓ / j k, [Enter] to Execute, [Esc] to Close\n\n".to_string(),
        sub_style,
    ));

    for (i, item) in SETTINGS_ITEMS.iter().enumerate() {
        let is_selected = i == selected_idx;
        let prefix = if is_selected { "  ▶ [" } else { "    [" };
        let k_style = if is_selected { selected_title_style } else { key_style };
        let t_style = if is_selected { selected_title_style } else { title_style };
        let d_style = if is_selected { selected_desc_style } else { desc_style };

        spans.push((prefix.to_string(), k_style));
        spans.push((format!("{}] ", item.key), k_style));
        spans.push((format!("{} {} ", item.icon, item.title), t_style));
        spans.push((format!("— {}\n\n", item.desc), d_style));
    }

    let line_count = spans
        .iter()
        .map(|(s, _)| s.matches('\n').count())
        .sum::<usize>();
    text_engine.shape_buffer(buffer, &spans);
    line_count as f32 * text_engine.line_height + 40.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_items_count() {
        assert_eq!(SETTINGS_ITEMS.len(), SETTINGS_ITEMS_COUNT);
        assert_eq!(SETTINGS_ITEMS[0].title, "Edit Master Config");
        assert_eq!(SETTINGS_ITEMS[1].title, "Edit Colors & ANSI");
        assert_eq!(SETTINGS_ITEMS[2].title, "Edit Keybindings");
        assert_eq!(SETTINGS_ITEMS[3].title, "Explore Themes Folder");
        assert_eq!(SETTINGS_ITEMS[4].title, "Hot-Reload Configuration");
        assert_eq!(SETTINGS_ITEMS[5].title, "Open Config Directory in Explorer");
    }
}
