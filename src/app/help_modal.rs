//! Help & Shortcuts reference overlay builder (`F1` / `Ctrl+Shift+H`).

use crate::render::TextEngine;
use crate::terminal::{CellStyle, ColorRgb};

pub fn build_help_buffer(text_engine: &mut TextEngine, buffer: &mut glyphon::Buffer) -> f32 {
    let mut spans = Vec::with_capacity(48);

    let header_style = CellStyle::new()
        .with_fg(ColorRgb { r: 126, g: 156, b: 216 }) // Kanagawa crystalBlue (#7E9CD8)
        .with_bold(true);
    let section_style = CellStyle::new()
        .with_fg(ColorRgb { r: 230, g: 195, b: 132 }) // Kanagawa boatYellow2 (#E6C384)
        .with_bold(true);
    let key_style = CellStyle::new()
        .with_fg(ColorRgb { r: 152, g: 187, b: 108 }) // Kanagawa springGreen (#98BB6C)
        .with_bold(true);
    let desc_style = CellStyle::new().with_fg(ColorRgb { r: 220, g: 215, b: 186 }); // Kanagawa fujiWhite (#DCD7BA)
    let sub_style = CellStyle::new()
        .with_fg(ColorRgb { r: 114, g: 113, b: 105 }) // Kanagawa fujiGray (#727169)
        .with_dim(true);
    let border_style = CellStyle::new().with_fg(ColorRgb { r: 54, g: 54, b: 70 }); // Kanagawa sumiInk5 (#363646)

    spans.push((
        "  🌙 LUNULA TERMINAL ── Shortcuts & Features Reference\n".to_string(),
        header_style,
    ));
    spans.push((
        "  Scroll with Mouse Wheel or ↑ ↓ / PgUp PgDn • Press [ESC] or [Q] to close\n\n".to_string(),
        sub_style,
    ));

    spans.push(("  🪟 SPLIT PANES\n".to_string(), section_style));
    spans.push(("  Ctrl + Shift + E    ".to_string(), key_style));
    spans.push(("Split Vertically (side-by-side)\n".to_string(), desc_style));
    spans.push(("  Ctrl + Shift + D    ".to_string(), key_style));
    spans.push(("Split Horizontally (top / bottom)\n".to_string(), desc_style));
    spans.push(("  Ctrl + Shift + W    ".to_string(), key_style));
    spans.push((
        "Close Active Pane / Split (or exit if last)\n".to_string(),
        desc_style,
    ));
    spans.push(("  Ctrl + W            ".to_string(), key_style));
    spans.push((
        "Close Active Split Pane (when multiple panes open)\n".to_string(),
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
    spans.push(("  Ctrl + Shift + T    ".to_string(), key_style));
    spans.push(("New Tab\n".to_string(), desc_style));
    spans.push(("  Ctrl + Tab          ".to_string(), key_style));
    spans.push(("Switch to Next Tab\n".to_string(), desc_style));
    spans.push(("  Ctrl + Shift + Tab  ".to_string(), key_style));
    spans.push(("Switch to Previous Tab\n".to_string(), desc_style));
    spans.push(("  Ctrl + 1..9         ".to_string(), key_style));
    spans.push(("Jump directly to Tab 1 through 9\n\n".to_string(), desc_style));

    spans.push(("  ⚡ WINDOW & HOT RELOAD\n".to_string(), section_style));
    spans.push(("  F11                 ".to_string(), key_style));
    spans.push(("Toggle Fullscreen / Maximize\n".to_string(), desc_style));
    spans.push(("  Ctrl + Shift + R    ".to_string(), key_style));
    spans.push(("Hot Reload configuration (lunula.lua)\n".to_string(), desc_style));
    spans.push(("  Ctrl + Shift + P    ".to_string(), key_style));
    spans.push((
        "Hot Reload PowerShell profile (. $PROFILE)\n".to_string(),
        desc_style,
    ));
    spans.push(("  F1 / Ctrl+Shift+H   ".to_string(), key_style));
    spans.push(("Toggle this Help reference menu\n".to_string(), desc_style));
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
