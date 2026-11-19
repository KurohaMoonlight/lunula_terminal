# 📜 Lunula Terminal — Guiding Engineering Principles & Architectural Manifesto

> *"Ultra-lightweight terminal, highly customizable, single executable, zero external runtime dependencies, laser-focused on speed, minimalism, animation, and security."*

---

## 1. Core Philosophy & Guiding Tenets

Lunula Terminal was engineered from first principles as an antidote to modern software bloat. Where modern terminal emulators frequently package hundred-megabyte runtimes, multi-process web browser rendering engines, and layers of scripting overhead, Lunula adheres to a strict ethos of lean, uncompromised systems programming:

1. **True Zero-Dependency Standalone Executable**:
   Lunula compiles to a single, self-contained native Windows binary. It does not require web runtime runtimes (Electron, WebView2), runtime DLLs, or external interpreters. Everything needed—from font rasterization and vector rendering to Lua scripting and VT parsing—is statically compiled.

2. **Zero-Cost Abstractions & Mechanical Sympathy**:
   Performance is not an afterthought tuned at the end; it is designed into every memory structure. Data layouts match modern CPU cache line boundaries, heap allocations are eliminated across hot loops (PTY parsing, SGR dispatch, render frame ticks), and row vectors are recycled in place.

3. **Native Windows-First Engineering**:
   Rather than treating Windows as an emulation layer or second-class citizen, Lunula embraces native Windows subsystems:
   - High-throughput Windows Pseudo Console (ConPTY) API bindings.
   - Native Win32 Shell API (`ShellExecuteW`, DPI awareness, window composition).
   - Direct loopback IPC sockets for high-bandwidth binary companion utilities (`limg`).

4. **Tactile, Organic Interactivity**:
   Terminal emulators should feel alive under human touch. Lunula bridges raw terminal output with dynamic GPU physics:
   - A 10×8 spring-mass jelly mesh simulating inertia, wobble, and momentum on window moves.
   - Reactive Kanagawa fluid wave dynamics that surge and oscillate with window velocity.
   - Viscous, critically-damped spring transitions for cursor navigation and sub-pixel scrolling.

5. **Security by Design (Zero Trust on Remote Streams)**:
   A terminal emulator is an execution boundary between untrusted remote hosts (SSH servers, container output, public git repos) and the local operating system. Lunula enforces strict boundary guards: no arbitrary shell invocations on URL clicks, strict OSC 52 clipboard exfiltration gates, and bounded memory consumption on escape sequences.

---

## 2. Memory Architecture & Zero-Allocation Pipelines

Lunula’s memory footprint is deterministically bounded through rigorous memory budgeting:

### 2.1. Packed 20-Byte Terminal Cell Representation
Every visible and scrollback character in `TerminalGrid` is stored as a `Cell` (`src/terminal/cell.rs`):
- `c: char` (4 bytes, Unicode scalar value).
- `style: CellStyle` (13 bytes):
  - `fg: Color` (4 bytes: RGB or palette index).
  - `bg: Color` (4 bytes: RGB or palette index).
  - `flags: u8` (1 byte bitflags packing `BOLD`, `DIM`, `ITALIC`, `UNDERLINE`, `STRIKETHROUGH`, `INVERSE`, `HIDDEN`, `WRAPPED`).
  - Alignment padding (4 bytes).
- `hyperlink_idx: u16` (2 bytes, indexing into a deduplicated, pruned URL table).
- Alignment padding (1 byte).
- **Total per Cell**: Exactly **20 bytes** (shrunk from 28 bytes), saving tens of megabytes of RAM across large scrollback buffers.

### 2.2. $O(1)$ Deque Scrollback with In-Place Row Recycling
- Scrollback is backed by `VecDeque<Vec<Cell>>`. Evicting the oldest row via `pop_front()` runs in strict **$O(1)$** time.
- Instead of deallocating evicted rows, Lunula reclaims the underlying `Vec<Cell>` heap memory, clears it via `.fill(Cell::default())`, and pushes it onto the bottom of the active screen matrix—achieving **0 heap allocations** during continuous streaming output.

### 2.3. Stack-Allocated SGR Dispatch & ConPTY Wake Coalescing
- Parsing ANSI Select Graphic Rendition (SGR) codes operates entirely on a stack-allocated buffer `[u16; 32]`, completely avoiding dynamic vector allocations per escape code.
- High-throughput PTY reads utilize atomic wake coalescing (`Arc<AtomicBool>`): high-frequency bursts from CLI compilers or log dumps buffer silently into an MPSC channel while waking the event loop at most once per burst cycle, eliminating UI thread starvation.

---

## 3. High-Fidelity Rendering Pipeline

Lunula utilizes a hardware-accelerated two-pass WGPU rendering architecture:

```
┌────────────────────────────────────────────────────────┐
│ ConPTY Stream -> VTE Parser -> TerminalGrid State      │
└───────────────────────────┬────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────┐
│ Pass 1: Offscreen Framebuffer (wobble.content_view)    │
│  ├── 1. Procedural Vector Quads (Box/Powerline Glyphs) │
│  ├── 2. Run-Length Merged Background Batches (BgRect) │
│  └── 3. Sub-Pixel Text Glyphs (glyphon + HarfBuzz)     │
└───────────────────────────┬────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────┐
│ Pass 2: Swapchain Jelly Mesh & Shader Composition      │
│  ├── 1. 10x8 Verlet Spring-Damper Mesh Deformation     │
│  ├── 2. Kanagawa Ink Fluid Wave Simulation             │
│  └── 3. Drop Shadows & Hardware Window Controls        │
└────────────────────────────────────────────────────────┘
```

1. **Procedural Geometry for Box & Powerline Symbols**:
   Characters like `─`, `│`, `╭`, `╯`, `█`, `░`, and Powerline chevrons (``, ``) bypass font glyph rasterization entirely. They are rendered procedurally as exact geometric vector quads, guaranteeing razor-sharp lines with zero pixel seams at any display scaling factor or DPI.

2. **Run-Length Quad Merging**:
   Contiguous horizontal spans of text sharing identical background colors are coalesced into a single quad (`BgRect`) before hitting the GPU vertex buffer, slashing draw call overhead.

3. **Sub-Pixel Motion & Dirty Caching**:
   Text shaping buffers are strictly cached: status bars are shaped only upon state mutation, and terminal panes are shaped only when characters or integer row bounds change. Fractional offsets (`scroll_subpixel_y`) are fed directly into shader uniforms, delivering silky 144Hz+ smooth scrolling without re-rasterizing text.

---

## 4. Security Architecture & Threat Defense

Terminal emulators operate on untrusted remote input. Lunula implements defense-in-depth against malicious escape sequences:

| Threat Vector | Vulnerability | Lunula Defense Architecture |
|---|---|---|
| **OSC 8 Hyperlink Injection** | Execution of arbitrary commands via shell metacharacters (`&`, `\|`, `^`, `%`) or dangerous schemes (`file://`, `ms-appinstaller:`) passed to `cmd.exe`. | **Bypasses `cmd.exe` completely.** URLs are validated against a strict `http://` / `https://` allowlist and launched directly via the native Win32 `ShellExecuteW` FFI API with safe wide-character conversions. |
| **OSC 52 Clipboard Exfiltration** | Malicious remote SSH servers or CLI tools querying clipboard contents (`OSC 52 ; c ; ?`) to steal passwords, tokens, or private keys. | **Hardened by default.** Remote clipboard read queries are strictly ignored unless explicitly opted-in via `allow_clipboard_read = true` in `lunula.lua`. Writes are sanitized. |
| **Hyperlink Table Memory Exhaustion** | Flooding millions of OSC 8 URLs into scrollback to exhaust system memory. | **Bounded Generational Compaction.** URL storage is automatically pruned and compacted when capacity exceeds thresholds (512 entries with 128 dead indices) and during scrollback purge (`CSI 3 J`). |
| **Resource & Thread Leaks** | Abandoned IPC threads and listening TCP sockets remaining open when panes are closed. | **RAII Drop Cleanup.** `PtyManager` implements `Drop`, performing an instantaneous loopback self-connect to unblock `listener.accept()` and cleanly join worker threads. |

---

## 5. Terminal Compatibility, Standards & Typography

Lunula provides a comprehensive implementation of modern terminal standards:

- **Soft-Wrap Tracking & Full Line Reflow**: Logical lines wrapped across narrow panes preserve their wrapped state. Resizing the window reflows long wrapped lines smoothly without permanent truncation.
- **Complex Unicode & Grapheme Clusters**: Full support for zero-width characters, combining diacritics, variation selectors (U+FE0F), and emoji Zero-Width Joiner (ZWJ) sequences.
- **Modern DEC Private Modes**:
  - `?2026`: Synchronized output for flicker-free atomic TUI updates.
  - `?1004`: Window focus reporting (delivering `<ESC>[I` and `<ESC>[O` to Neovim and tmux).
  - `?2004`: Bracketed paste mode protection.
- **Dynamic Theme Querying**:
  - `OSC 10 / 11 / 12`: Terminal responds with live RGB color values matching the active Lua theme rather than stale hardcoded defaults.
- **IME & File Drop Integration**:
  - Windows IME composition overlays (`Ime::Preedit`) rendered directly at the cursor insertion point.
  - Native file drag-and-drop (`DroppedFile`), automatically quoting and inserting file paths into the active PTY.

---

## 6. The Companion Developer Ecosystem

Lunula includes a suite of fast, lightweight developer tools sharing the same design ethos:

- **`lunula`**: The main GPU-accelerated terminal emulator.
- **`luna`**: A lightweight modal text editor built right into the repository with vim-inspired keybindings, zero external dependencies, and instant startup.
- **`lfetch`**: A sub-millisecond system information tool displaying hardware, OS, memory, and desktop stats in a beautiful Kanagawa-themed output.
- **`limg`**: High-fidelity terminal image viewer communicating over Lunula’s high-speed local IPC socket to render images directly in the terminal window.
- **`lpm`**: A minimalist package and plugin manager for themes and extensions.

---

## 7. Developer Invariants & Quality Standards

All contributions to Lunula must uphold these engineering invariants:
1. **Always build cleanly**: No unhandled compiler errors or unchecked warnings.
2. **Every optimization must be verified**: Performance claims must be backed by zero allocations or deterministic algorithmic improvements.
3. **Security is non-negotiable**: Input validation must be performed before passing data across OS, file, or network boundaries.
4. **CI Green**: Complete test suite and strict Clippy checks must pass on Windows platforms before any release.
