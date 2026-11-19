# ⚡ Lunula Terminal Technical Optimization Log

This document details the 7 architectural and algorithmic optimizations implemented in `lunula` to achieve minimum memory consumption, high responsiveness, and zero-allocation frame loops.

---

## 1. Infinite Render Loop Elimination in `PresentMode::Mailbox` (`src/app/render.rs` & `src/app/mod.rs`)
* **Root Cause:** When cursor blinking was active (`cursor.blinking = true`), `is_animating` was set to `true` on every frame, immediately triggering `self.window.request_redraw()`. Because `PresentMode::Mailbox` does not block on VSync, the terminal spun at thousands of FPS while idle.
* **Implementation:** The animation state was bifurcated:
  1. `high_fps_animating: bool` — Active strictly during window drags/wobbles, active smooth scrolling, or cursor physics transitions (`dist > 0.5`).
  2. `next_blink_frame: Option<Instant>` — When idle and focused, cursor blinks are paced via `ControlFlow::WaitUntil(now + Duration::from_millis(33))` (~30 FPS) in `ApplicationHandler::about_to_wait`, and shut off completely when unfocused (`!self.is_focused`).

---

## 2. HarfBuzz Shaping Caching on Status Bar & Sub-Pixel Scrolling (`src/app/render.rs` & `src/pane/item.rs`)
* **Root Cause:**
  1. `bar_center_buffer` (`"username :  branch"`) was re-shaped by HarfBuzz on every single `render()` call.
  2. During smooth scrolling, `update_smooth_scroll` flagged `terminal.dirty = true` every frame even when `base_offset` (integer row index) had not advanced.
* **Implementation:**
  1. Added `bar_center_dirty: bool` to `GpuState`, ensuring `bar_center_buffer` is shaped only on startup, on Git branch change, or on font resize.
  2. Added `last_shaped_base_offset: Option<usize>` and `scroll_subpixel_y: f32` to `Pane`. `sync_render_cache` executes `render_spans_and_backgrounds` + `shape_buffer` only when text updates (`terminal.dirty`) or integer row offset changes. Sub-pixel fractional offsets (`current_scroll_y % cell_h`) are applied directly in GPU uniforms to `TextArea.top` and `BgRect.y`.

---

## 3. $O(1)$ `VecDeque<Vec<Cell>>` Scrollback & Zero-Alloc Row Recycling (`src/terminal/mod.rs` & `src/terminal/parser.rs`)
* **Root Cause:** A `Vec<Vec<Cell>>` scrollback caused `self.scrollback.remove(0)` to shift 5,000 vector elements in memory on every newline, deallocating old row vectors and allocating new `vec![Cell::default(); self.cols]` on the heap.
* **Implementation:**
  1. Replaced scrollback backing store with `VecDeque<Vec<Cell>>`, making `pop_front()` and `push_back()` constant time **$O(1)$**.
  2. When `scrollback.len() >= max_scrollback`, popped rows from `pop_front()` are wiped via `.fill(Cell::default())` and pushed directly back into the bottom of `self.cells` (**0 heap allocations**).
  3. Operations `CSI L` (*Insert Lines*), `CSI M` (*Delete Lines*), and `scroll_region_down` recycle evicted row vectors with zero heap allocations, using SIMD-vectorized `copy_within` + `fill` for `CSI P` / `CSI @` / `CSI J` / `CSI K`.

---

## 4. `Cell` Memory Compaction (28 Bytes → 20 Bytes) & Stack SGR Parser (`src/terminal/cell.rs` & `src/terminal/parser.rs`)
* **Root Cause:** `CellStyle` stored 7 individual `bool` fields, inflating `CellStyle` to 19 bytes and `Cell` to 28 bytes. Additionally, `csi_dispatch` for `'m'` allocated a new `Vec<u16>` on the heap for every ANSI escape sequence.
* **Implementation:**
  1. Compressed all 7 boolean attributes (bold, dim, italic, underline, strikethrough, inverse, hidden) into a single packed `u8` bitflags (`flags: u8`), shrinking `CellStyle` to 13 bytes and `Cell` to **20 bytes**.
  2. For a 10,000-row scrollback buffer, this saves over **20 megabytes** of RAM.
  3. Replaced `Vec<u16>` in SGR dispatch with a stack-allocated fixed array `[u16; 32]`, completely eliminating heap allocations when parsing ANSI color sequences.

---

## 5. Scratch Vertex Vector Reuse & Dynamic Buffers (`src/render/mod.rs`)
* **Root Cause:** Background quad rendering allocated a fresh `Vec<BackgroundVertex>` on every frame and recreated a static `wgpu::Buffer` via `create_buffer_init`.
* **Implementation:**
  1. Added `scratch_vertices: Vec<BackgroundVertex>` to `TextEngine`, retaining capacity across frames via `.clear()`.
  2. Created a dynamic GPU vertex buffer with `wgpu::BufferUsages::COPY_DST`. Whenever vertex count exceeds buffer capacity, the buffer grows exponentially ($2\times$ growth); otherwise, vertices are transferred via zero-overhead `queue.write_buffer`.

---

## 6. Static Wobble Index Buffer & Selective Vertex Updates (`src/wobble/mod.rs`)
* **Root Cause:** `WobbleWindow` uploaded identical triangle indices (`GRID_COLS * GRID_ROWS * 6` indices) to the GPU on every frame, allocating an index vector each frame.
* **Implementation:**
  1. Pre-generated a static `wgpu::BufferUsages::INDEX` on startup that remains constant throughout the application lifecycle.
  2. `WobbleWindow::render` uploads vertices only when `self.mesh_dirty == true` or when the window is deforming, bypassing GPU writes while at rest.

---

## 7. Selective ConPTY Reader Thread Wake Coalescing (`src/pty/mod.rs`)
* **Root Cause:** Fast CLI programs (e.g., `cat large_file.log`, `cargo build`) emit hundreds of 4 KB chunks per second. Waking the main event loop on every individual read caused event loop starvation and UI frame drops.
* **Implementation:**
  * Added `wake_pending: Arc<AtomicBool>` to `PtyManager`.
  * The background reader thread performs an atomic compare-exchange: if `wake_pending` is already `true`, the chunk is queued silently into the `mpsc::channel` without emitting an event loop wake signal.
  * When the main thread processes the event, it drains all queued chunks and atomically resets `wake_pending` to `false`.
