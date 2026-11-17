# ⚡ Detail Teknis Optimasi Lunula Terminal

Dokumen ini merangkum 7 optimasi arsitektural dan algoritmik yang telah diterapkan pada `lunula_terminal`.

---

## 1. Eliminasi Infinite Render Loop pada `PresentMode::Mailbox` (`src/app/render.rs` & `src/app/mod.rs`)
* **Akar Masalah:** Sebelumnya, saat kursor berkedip (`cursor.blinking = true`), variabel `is_animating` disetel ke `true` setiap *frame* dan langsung memanggil `self.window.request_redraw()`. Karena `PresentMode::Mailbox` tidak memblokir pada VSync, terminal berputar pada ribuan FPS tanpa henti saat *idle*.
* **Implementasi:** Status animasi dibagi menjadi dua:
  1. `high_fps_animating: bool` — aktif hanya saat jendela sedang di-*drag*/*wobble*, *smooth scroll* sedang bergerak, atau *cursor trail* sedang meluncur (`dist > 0.5`).
  2. `next_blink_frame: Option<Instant>` — saat terminal *idle* dan jendela fokus, kedipan kursor dijadwalkan menggunakan `ControlFlow::WaitUntil(now + Duration::from_millis(33))` (~30 FPS) di `ApplicationHandler::about_to_wait`, serta dihentikan sepenuhnya ketika jendela tidak fokus (`!self.is_focused`).

---

## 2. Caching HarfBuzz Shaping pada Status Bar & Sub-Pixel Scroll (`src/app/render.rs` & `src/pane/item.rs`)
* **Akar Masalah:**
  1. `bar_center_buffer` (`"username :  branch"`) sebelumnya di-*shape* ulang dengan HarfBuzz pada setiap pemanggilan `render()`.
  2. Saat *smooth scroll* berjalan, `update_smooth_scroll` menandai `terminal.dirty = true` setiap *frame* meskipun `base_offset` (indeks baris bulat) belum berpindah baris.
* **Implementasi:**
  1. Menambahkan flag `bar_center_dirty: bool` pada `GpuState` sehingga `bar_center_buffer` hanya di-*shape* saat inisialisasi, saat cabang Git berubah, atau saat ukuran *font* diperbarui.
  2. Menambahkan `last_shaped_base_offset: Option<usize>` dan `scroll_subpixel_y: f32` pada `Pane`. `sync_render_cache` hanya menjalankan `render_spans_and_backgrounds` + `shape_buffer` jika ada teks baru (`terminal.dirty`) atau indeks baris bulat `base_offset` berubah. Sisa fraksi piksel (`current_scroll_y % cell_h`) diterapkan langsung di GPU pada `TextArea.top` dan `BgRect.y`.

---

## 3. Struktur Scrollback `VecDeque<Vec<Cell>>` $O(1)$ & Daur Ulang Baris Zero-Alloc (`src/terminal/mod.rs` & `src/terminal/parser.rs`)
* **Akar Masalah:** Penggunaan `Vec<Vec<Cell>>` pada `scrollback` menyebabkan `self.scrollback.remove(0)` menggeser 5.000 elemen vektor di memori setiap kali terjadi *newline*, lalu membuang vektor baris lama dan mengalokasikan `vec![Cell::default(); self.cols]` baru di *heap*.
* **Implementasi:**
  1. Mengubah `scrollback` menjadi `VecDeque<Vec<Cell>>` sehingga operasi `pop_front()` dan `push_back()` berjalan dalam waktu konstan **$O(1)$**.
  2. Ketika `scrollback.len() >= max_scrollback`, buffer `Vec<Cell>` yang keluar dari `pop_front()` langsung dibersihkan dengan `.fill(Cell::default())` dan dimasukkan kembali ke baris paling bawah `self.cells` (**0 alokasi heap**).
  3. Operasi `CSI L` (*Insert Lines*), `CSI M` (*Delete Lines*), dan `scroll_region_down` juga mendaur ulang baris yang dihapus tanpa alokasi baru, serta menggunakan `copy_within` + `fill` yang tervektorisasi SIMD untuk `CSI P` / `CSI @` / `CSI J` / `CSI K`.

---

## 4. Pemadatan Memori `Cell` (28 Byte → 20 Byte) & Parser SGR Stack (`src/terminal/cell.rs` & `src/terminal/parser.rs`)
* **Akar Masalah:** `CellStyle` menyimpan 7 *field* `bool` terpisah yang membuat ukuran `CellStyle` menjadi 19 byte dan `Cell` menjadi 28 byte. Selain itu, `csi_dispatch` untuk aksi `'m'` mengalokasikan `Vec<u16>` baru di *heap* pada setiap sekuens warna ANSI.
* **Implementasi:**
  1. Mengemas ke-7 atribut boolean (`bold`, `dim`, `italic`, `underline`, `undercurl`, `strikethrough`, `reverse`) ke dalam satu bitfield `flags: u8` pada `CellStyle` (13 byte) sehingga ukuran `Cell` menyusut menjadi **20 byte** (diuji secara otomatis oleh `test_cell_memory_compaction`).
  2. Mengganti `Vec<u16>` pada SGR `'m'` dengan array statis di *stack* (`let mut flat = [0u16; 32];`).

---

## 5. Run-Length Merging `BgRect` & Scratch Vertex Buffer (`src/terminal/procedural.rs`, `src/terminal/renderer.rs`, `src/render/mod.rs`)
* **Akar Masalah:** Setiap sel berwarna latar atau terseleksi menghasilkan 1 `BgRect` (6 *vertex*) terpisah, dan `TextEngine::update_bg_rects` mengalokasikan `Vec<BgVertex>` baru setiap *frame*.
* **Implementasi:**
  1. Fungsi `push_bg_rect_merged` menggabungkan `BgRect` yang bersebelahan secara horizontal pada baris yang sama jika memiliki warna, *alpha*, posisi `y`, dan tinggi `h` yang identik.
  2. `TextEngine` menyimpan `scratch_vertices: Vec<BgVertex>` permanen yang digunakan ulang setiap *frame* tanpa alokasi baru.

---

## 6. Pengurasan PTY Lintas Tab, Atomic EOF, & Wake Coalescing (`src/pty/mod.rs` & `src/pane/mod.rs`)
* **Akar Masalah:** `TabManager::drain_all_ptys` sebelumnya hanya menguras PTY pada tab yang sedang aktif, sehingga tab *background* yang menjalankan proses berisik menumpuk ribuan `Vec<u8>` di RAM. Selain itu, `reap_exited_panes` memanggil syscall `child.try_wait()` setiap *frame*.
* **Implementasi:**
  1. `drain_all_ptys` kini menguras `pty.rx` untuk **seluruh pane di semua tab**, tetapi hanya menjalankan *shaping* `sync_render_cache` untuk pane yang terlihat di tab aktif.
  2. `PtyManager` menggunakan `exited: Arc<AtomicBool>` dan `wake_pending: Arc<AtomicBool>` untuk deteksi keluarnya proses dalam $O(1)$ dan penggabungan sinyal bangun (*wake coalescing*).

---

## 7. Early-Exit Fisika Wobble, Static Index Buffer, & `MemoryHints::MemoryUsage` (`src/wobble/` & `src/app/state.rs`)
* **Akar Masalah:** `WobbleWindow::update_physics()` menjalankan 888 evaluasi pegas setiap *frame* walaupun jendela diam, dan `WobbleWindow::render()` membangun ulang `indices: Vec<u16>` setiap *frame*.
* **Implementasi:**
  1. Menambahkan *early-exit* di awal `update_physics()` saat `!self.is_dragging && !self.is_wobbling()`.
  2. Memindahkan pembuatan `index_buffer` (756 indeks `u16`) satu kali saja ke `WobbleWindow::new()`, serta mengunggah `vertex_buffer` hanya ketika `self.mesh_dirty == true`.
  3. Mengaktifkan `wgpu::MemoryHints::MemoryUsage` saat meminta `wgpu::Device` untuk menekan ukuran *working set* driver grafis.
