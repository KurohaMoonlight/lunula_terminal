# 🏛️ Arsitektur Modular Lunula Terminal

Dokumen ini menjelaskan arsitektur internal `lunula_terminal` setelah refaktorisasi modular serta bagaimana setiap komponen berinteraksi.

---

## 1. Diagram Alur Data & Pipeline Render

1. **Input & PTY Layer (`src/pty/mod.rs`)**:
   - Setiap `Pane` memiliki satu instansi `PtyManager` yang menjalankan proses shell (`powershell.exe -NoLogo`) di atas **Windows ConPTY**.
   - Thread latar `conpty-reader` membaca *output* byte dari *master pipe* ke dalam buffer 16 KB dan mengirimkannya melalui kanal `mpsc::channel`.
   - Untuk mencegah banjir pesan ke *event loop* utama saat menangani *output* berkecepatan tinggi, `conpty-reader` menggunakan `wake_pending: Arc<AtomicBool>` (*wake coalescing*) sehingga `proxy.send_event(())` hanya dipanggil satu kali per gelombang bacaan sampai *main thread* menguras kanal tersebut.

2. **Terminal State Machine (`src/terminal/`)**:
   - `TerminalGrid` (`src/terminal/mod.rs`) menyimpan *grid* layar aktif (`cells: Vec<Vec<Cell>>`) dan riwayat *scrollback* (`scrollback: VecDeque<Vec<Cell>>`).
   - Sekuens byte dari PTY diumpankan ke parser ANSI/VT100 (`src/terminal/parser.rs`) yang mengimplementasikan `vte::Perform`.
   - Setiap sel (`Cell` di `src/terminal/cell.rs`) berukuran tepat **20 byte**, menyimpan karakter `char`, indeks hyperlink `u16`, dan `CellStyle` (13 byte dengan 7 atribut boolean yang dipadatkan ke dalam bitflags `u8`).

3. **Workspace & Layout Tree (`src/pane/`)**:
   - `TabManager` (`src/pane/mod.rs`) mengelola daftar `Tab` dan tabel `HashMap<usize, Pane>`.
   - Setiap `Tab` memiliki pohon biner `PaneNode` (`src/pane/tree.rs`) yang membagi ruang layar secara rekursif menjadi `SplitDirection::Vertical` atau `SplitDirection::Horizontal`.
   - Pada setiap siklus `drain_all_ptys`, seluruh pane di semua tab (termasuk tab *background*) dikuras byte PTY-nya agar tidak terjadi penumpukan memori di kanal, namun proses *text shaping* hanya dijalankan untuk pane yang sedang tampil di tab aktif (`visible_leaves`).

4. **Text Shaping & Procedural Vector Generation (`src/terminal/renderer.rs`, `src/terminal/procedural.rs`, `src/render/mod.rs`)**:
   - Ketika isi `TerminalGrid` berubah atau *scroll* melintasi batas baris bulat (`base_offset != last_shaped_base_offset`), `Pane::sync_render_cache` mengekstrak *span* teks berwarna dan daftar persegi latar (`BgRect`).
   - Karakter *box-drawing* (`─`, `│`, `╭`, `╮`, `╯`, `╰`), elemen blok (`█`, `░`, `▒`, `▓`), dan simbol Powerline (`\u{E0B0}`, `\u{E0B2}`, `\u{E0B4}`, `\u{E0B6}`) digambar secara prosedural sebagai geometri vektor GPU agar bebas celah piksel pada ukuran *font* berapa pun.
   - `push_bg_rect_merged` menggabungkan persegi latar yang bersebelahan secara horizontal dengan warna dan tinggi yang sama menjadi satu `BgRect` tunggal (*Run-Length Merging*).

5. **Two-Pass WGPU Rendering & Wobble Physics (`src/wobble/`, `src/app/render.rs`)**:
   - **Pass 1 (Offscreen Content Texture)**: `TextEngine` menggambar seluruh `BgRect` dan *glyph* teks (`glyphon`) ke tekstur *offscreen* `wobble.content_view`. Pergeseran *smooth scroll* sub-piksel (`scroll_subpixel_y`) diterapkan langsung pada koordinat vertikal `BgRect` dan `TextArea.top`.
   - **Pass 2 (Swapchain Jelly Mesh)**: `WobbleWindow` memetakan tekstur *offscreen* tersebut ke atas jaring pegas 10×8 (`GRID_COLS × GRID_ROWS`) dengan *drop shadow* dan tombol kontrol jendela di `wobble.wgsl`.
