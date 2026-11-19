# 🏛️ Lunula Terminal Modular Architecture

This document details the internal architecture of `lunula` following its modular refactoring, detailing component interactions and data pipelines.

---

## 1. Data Flow & Render Pipeline Diagram

1. **Input & PTY Layer (`src/pty/mod.rs`)**:
   * Each `Pane` owns a `PtyManager` instance running a Windows ConPTY shell process (`powershell.exe -NoLogo`).
   * A background `conpty-reader` thread reads byte output from the master pipe into a 16 KB buffer and dispatches chunks through an `mpsc::channel`.
   * To prevent message flooding into the main event loop during high-throughput CLI output, `conpty-reader` employs wake coalescing (`wake_pending: Arc<AtomicBool>`), ensuring `proxy.send_event(())` is called at most once per read burst until the main thread finishes draining the channel.

2. **Terminal State Machine (`src/terminal/`)**:
   * `TerminalGrid` (`src/terminal/mod.rs`) maintains the active screen grid (`cells: Vec<Vec<Cell>>`) and historical scrollback (`scrollback: VecDeque<Vec<Cell>>`).
   * Incoming byte streams feed into an ANSI/VT100 parser (`src/terminal/parser.rs`) implementing `vte::Perform`.
   * Each cell (`Cell` in `src/terminal/cell.rs`) occupies exactly **20 bytes**, storing a `char`, a `u16` hyperlink index, and a packed `CellStyle` (13 bytes with 7 boolean attributes compressed into a single `u8` bitflags).

3. **Workspace & Layout Tree (`src/pane/`)**:
   * `TabManager` (`src/pane/mod.rs`) manages the `Tab` collection and a lookup table `HashMap<usize, Pane>`.
   * Each `Tab` holds a binary tree `PaneNode` (`src/pane/tree.rs`) dividing screen space recursively across `SplitDirection::Vertical` or `SplitDirection::Horizontal`.
   * On every `drain_all_ptys` cycle, all panes across all tabs (including background tabs) drain their PTY pipes to prevent memory buildup in channels. Text shaping is strictly performed for panes currently visible on the active tab (`visible_leaves`).

4. **Text Shaping & Procedural Vector Geometry (`src/terminal/renderer.rs`, `src/terminal/procedural.rs`, `src/render/mod.rs`)**:
   * Whenever `TerminalGrid` content changes or scrolling crosses an integer row boundary (`base_offset != last_shaped_base_offset`), `Pane::sync_render_cache` extracts colored text spans and background rectangles (`BgRect`).
   * Box-drawing characters (`─`, `│`, `╭`, `╮`, `╯`, `╰`), block elements (`█`, `░`, `▒`, `▓`), and Powerline symbols (`\u{E0B0}`, `\u{E0B2}`, etc.) are rendered procedurally as GPU vector geometry to eliminate pixel gaps at any font scale.
   * `push_bg_rect_merged` consolidates horizontally adjacent background rectangles of identical color and height into single unified quads (*Run-Length Merging*).

5. **Two-Pass WGPU Rendering & Wobble Physics (`src/wobble/`, `src/app/render.rs`)**:
   * **Pass 1 (Offscreen Content Texture)**: `TextEngine` draws all `BgRect` quads and text glyphs (`glyphon`) to the offscreen texture `wobble.content_view`. Sub-pixel scroll offsets (`scroll_subpixel_y`) are applied directly to `BgRect` vertical coordinates and `TextArea.top`.
   * **Pass 2 (Swapchain Jelly Mesh)**: `WobbleWindow` projects the offscreen texture onto an interactive 10×8 spring mesh (`GRID_COLS × GRID_ROWS`) alongside drop shadows and window control buttons in `wobble.wgsl`.

---

## 2. Reactive Kanagawa Waves

`WaveState` (`src/wobble/physics.rs`) models fluid dynamics inside a shallow tray: moving the window activates an underdamped oscillator pushing fluid backward (`slosh`), vertical bobbing (`bob`), and accumulating kinetic energy (`energy`). Upon release, fluid surges forward and oscillates toward equilibrium (~1.5s settling time). Uniforms `wave` and `wave2` are sent to `kanagawa_ink_wave` in `wobble.wgsl`, altering wave tilt, swell heights, spray intensity, and flow velocity. Configurable via `wobble.wave_reactivity` in `lunula.lua`.
