//! Help & Shortcuts reference overlay builder (Dynamic keybindings).

use crate::config::KeybindsConfig;
use crate::render::TextEngine;
use crate::terminal::{CellStyle, ColorRgb};
use crate::theme::Theme;

pub fn build_help_buffer(
    text_engine: &mut TextEngine,
    buffer: &mut glyphon::Buffer,
    keybinds: &KeybindsConfig,
    theme: &Theme,
) -> f32 {
    let mut spans = Vec::with_capacity(48);

    let header_style = CellStyle::new()
        .with_fg(ColorRgb {
            r: theme.syntax_type.r,
            g: theme.syntax_type.g,
            b: theme.syntax_type.b,
        })
        .with_bold(true);

    let section_style = CellStyle::new()
        .with_fg(ColorRgb {
            r: theme.syntax_keyword.r,
            g: theme.syntax_keyword.g,
            b: theme.syntax_keyword.b,
        })
        .with_bold(true);

    let key_style = CellStyle::new()
        .with_fg(ColorRgb {
            r: theme.gutter_active.r,
            g: theme.gutter_active.g,
            b: theme.gutter_active.b,
        })
        .with_bold(true);

    let desc_style = CellStyle::new().with_fg(ColorRgb {
        r: theme.fg.r,
        g: theme.fg.g,
        b: theme.fg.b,
    });

    let sub_style = CellStyle::new()
        .with_fg(ColorRgb {
            r: theme.gutter_fg.r,
            g: theme.gutter_fg.g,
            b: theme.gutter_fg.b,
        })
        .with_dim(true);

    let border_style = CellStyle::new().with_fg(ColorRgb {
        r: theme.border.r,
        g: theme.border.g,
        b: theme.border.b,
    });

    spans.push((
        "  🌙 LUNULA TERMINAL ── Shortcuts & Features Reference\n".to_string(),
        header_style,
    ));
    spans.push((
        "  Scroll with Mouse Wheel or ↑ ↓ / PgUp PgDn • Press [ESC] or [Q] to close\n\n".to_string(),
        sub_style,
    ));

    spans.push(("  🪟 SPLIT PANES\n".to_string(), section_style));
    spans.push((format!("  {:<20}", keybinds.split_vertical), key_style));
    spans.push(("Split Vertically (side-by-side)\n".to_string(), desc_style));
    spans.push((format!("  {:<20}", keybinds.split_horizontal), key_style));
    spans.push(("Split Horizontally (top / bottom)\n".to_string(), desc_style));
    spans.push((format!("  {:<20}", keybinds.close_pane), key_style));
    spans.push((
        "Close Active Pane / Split (or exit if last)\n".to_string(),
        desc_style,
    ));
    spans.push(("  Alt + Arrows        ".to_string(), key_style));
    spans.push((
        "Navigate focus between panes (← ↑ → ↓)\n".to_string(),
        desc_style,
    ));
    spans.push(("  exit                ".to_string(), key_style));
    spans.push((
        "Type 'exit' in shell to close split pane\n".to_string(),
        desc_style,
    ));
    spans.push(("  [✕] button          ".to_string(), key_style));
    spans.push((
        "Click close badge in top-right of split pane\n\n".to_string(),
        desc_style,
    ));

    spans.push(("  📑 TABS\n".to_string(), section_style));
    spans.push((format!("  {:<20}", keybinds.new_tab), key_style));
    spans.push(("New Tab\n".to_string(), desc_style));
    spans.push((format!("  {:<20}", keybinds.next_tab), key_style));
    spans.push(("Switch to Next Tab\n".to_string(), desc_style));
    spans.push((format!("  {:<20}", keybinds.prev_tab), key_style));
    spans.push(("Switch to Previous Tab\n".to_string(), desc_style));
    spans.push(("  Ctrl + 1..9         ".to_string(), key_style));
    spans.push(("Jump directly to Tab 1 through 9\n\n".to_string(), desc_style));

    spans.push(("  ⚡ WINDOW, SETTINGS & RELOAD\n".to_string(), section_style));
    spans.push((format!("  {:<20}", keybinds.toggle_settings), key_style));
    spans.push(("Open Settings Hub (Gear button)\n".to_string(), desc_style));
    spans.push((format!("  {:<20}", keybinds.reload_config), key_style));
    spans.push(("Hot Reload configuration (lunula.lua)\n".to_string(), desc_style));
    spans.push((format!("  {:<20}", keybinds.reload_profile), key_style));
    spans.push((
        "Hot Reload PowerShell profile (. $PROFILE)\n".to_string(),
        desc_style,
    ));
    spans.push((format!("  {:<20}", keybinds.toggle_help), key_style));
    spans.push(("Toggle this Help reference menu\n".to_string(), desc_style));
    spans.push((format!("  {:<20}", keybinds.toggle_fullscreen), key_style));
    spans.push(("Toggle Fullscreen / Maximize\n".to_string(), desc_style));
    spans.push(("  Drag Titlebar       ".to_string(), key_style));
    spans.push((
        "Move window with jelly wobble physics\n\n".to_string(),
        desc_style,
    ));

    spans.push(("  📋 CLIPBOARD & SELECTION\n".to_string(), section_style));
    spans.push(("  Left-Click + Drag   ".to_string(), key_style));
    spans.push((
        "Select text (auto-clamped to text content)\n".to_string(),
        desc_style,
    ));
    spans.push(("  Right-Click         ".to_string(), key_style));
    spans.push(("Copy selection / Paste from clipboard\n".to_string(), desc_style));
    spans.push(("  Ctrl + C            ".to_string(), key_style));
    spans.push((
        "Copy selection (or send interrupt SIGINT)\n".to_string(),
        desc_style,
    ));
    spans.push(("  Ctrl + V            ".to_string(), key_style));
    spans.push(("Paste clipboard text\n\n".to_string(), desc_style));

    spans.push(("  🔍 VI MODE, SEARCH & HINTS\n".to_string(), section_style));
    spans.push((format!("  {:<20}", keybinds.search), key_style));
    spans.push(("Quick Search across scrollback buffer\n".to_string(), desc_style));
    spans.push(("  Ctrl + Shift + Space".to_string(), key_style));
    spans.push(("Toggle Alacritty-style Vi Mode\n".to_string(), desc_style));
    spans.push(("  h / j / k / l       ".to_string(), key_style));
    spans.push(("Move Vi cursor (← ↓ ↑ → into scrollback)\n".to_string(), desc_style));
    spans.push(("  v / V               ".to_string(), key_style));
    spans.push(("Toggle Visual / Visual Line selection\n".to_string(), desc_style));
    spans.push(("  y                   ".to_string(), key_style));
    spans.push(("Yank / copy selection to clipboard\n".to_string(), desc_style));
    spans.push(("  /                   ".to_string(), key_style));
    spans.push(("Search scrollback (n / N next/prev match)\n".to_string(), desc_style));
    spans.push(("  Ctrl + Shift + U / u".to_string(), key_style));
    spans.push(("Scan & highlight Regex Hints (URLs/paths/hashes)\n\n".to_string(), desc_style));

    spans.push((
        "  ─────────────────────────────────────────────────────────────\n".to_string(),
        border_style,
    ));
    spans.push((
        "  💡 Tips: Scroll with Mouse Wheel, Arrows (↑ ↓), or drag scrollbar\n".to_string(),
        key_style,
    ));
    spans.push((
        "  Press [ESC], [F1], [Q], or click outside / [✕] to close this guide\n".to_string(),
        sub_style,
    ));

    text_engine.shape_buffer(buffer, &spans);
    let line_count = spans.iter().map(|(t, _)| t.matches('\n').count()).sum::<usize>() + 2;
    line_count as f32 * text_engine.line_height + 40.0
}
