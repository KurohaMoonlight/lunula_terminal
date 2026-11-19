# 🌙 Lunula Terminal

**Lunula Terminal** adalah emulator terminal *ultra-lightweight* berbasis GPU (`wgpu` + `glyphon`) dengan fisika jendela elastis (*jelly wobble*), *smooth sub-pixel scrolling*, *cursor trail* pegas, dukungan multi-tab serta *split pane*, dan kustomisasi langsung melalui Lua (`lunula.lua`).

---

## ✨ Prinsip Utama (`principle.md`)
> *ultra-lightweight terminal, highly customisable, single executable, no dependencies, focus on speed, minimalism, animation*

---

## 📚 Dokumentasi Lengkap (`docs/`)

Lunula Terminal dilengkapi panduan terperinci di direktori `docs/`:

- ⚙️ **[Panduan Konfigurasi Lengkap (CONFIGURATION.md)](file:///e:/My%20Projects/lunula_terminal/docs/CONFIGURATION.md)**: Panduan parameter `lunula.lua`, kustomisasi 16 palet ANSI `colors.lua`, dan pembuatan tema kustom di `themes/`.
- ⌨️ **[Cheat Sheet Navigasi & Shortcuts (KEYBINDINGS.md)](file:///e:/My%20Projects/lunula_terminal/docs/KEYBINDINGS.md)**: Daftar lengkap shortcut split panes, tabs, quick search, Vi copy mode, dan kustomisasi `keybinds.lua`.
- 🌙 **[Panduan Micro-Editor luna (LUNA_EDITOR.md)](file:///e:/My%20Projects/lunula_terminal/docs/LUNA_EDITOR.md)**: Cara kerja editor bawaan (<400 KB), syntax highlighting multi-bahasa, dan integrasi sebagai Git default editor.
- 🧰 **[Utilitas Pendamping CLI (CLI_COMPANIONS.md)](file:///e:/My%20Projects/lunula_terminal/docs/CLI_COMPANIONS.md)**: Panduan penggunaan `lfetch` (3D raymarched fetch tool) dan `limg` (inline Kitty graphics image viewer).
- 🔍 **[Pemecahan Masalah & Optimasi (TROUBLESHOOTING.md)](file:///e:/My%20Projects/lunula_terminal/docs/TROUBLESHOOTING.md)**: Solusi font Nerd Font missing glyphs, profil hemat baterai laptop, dan input IME / Emojis.
- 🏗️ **[Arsitektur Teknis (ARCHITECTURE.md)](file:///e:/My%20Projects/lunula_terminal/docs/ARCHITECTURE.md)**: Arsitektur internal modul, alur data WGPU, dan integrasi Windows ConPTY.
- ⚡ **[Optimasi Performa & Memori (OPTIMIZATIONS.md)](file:///e:/My%20Projects/lunula_terminal/docs/OPTIMIZATIONS.md)**: Rincian teknis 7 strategi optimasi footprint memori & zero-allocation render pass.

---

## 📂 Struktur Folder Proyek (Modular)

Kode sumber telah direfaktor dari berkas monolitik (>2.300 baris per berkas) menjadi arsitektur modular dengan tanggung jawab tunggal (*Single Responsibility Principle*):

```text
lunula_terminal/
├── Cargo.toml                  # Manifes paket & profil rilis teroptimasi (LTO thin, strip)
├── Cargo.lock                  # Kunci versi dependensi
├── lunula.lua                  # Konfigurasi pengguna (hot-reload aktif)
├── principle.md                # Prinsip desain inti Lunula Terminal
├── README.md                   # Dokumentasi utama proyek & panduan pengguna
├── docs/
│   ├── ARCHITECTURE.md         # Dokumentasi arsitektur modul & alur data internal
│   └── OPTIMIZATIONS.md        # Catatan teknis 7 optimasi performa & memori
└── src/
    ├── main.rs                 # Entry point minimalis (init logger, config, EventLoop)
    ├── app/                    # Orkestrasi aplikasi, state WGPU, event loop, & input
    │   ├── mod.rs              # LunulaApp & ApplicationHandler (WaitUntil blink pacing)
    │   ├── state.rs            # GpuState, inisialisasi WGPU (MemoryHints::MemoryUsage), resize
    │   ├── render.rs           # Pipeline render 2-pass, status bar cache, sub-pixel scroll
    │   ├── input_keyboard.rs   # Shortcut keyboard, navigasi pane/tab, forwarding PTY
    │   ├── input_mouse.rs      # Mouse click/drag, smooth wheel scroll, seleksi teks, URL
    │   └── help_modal.rs       # Builder buffer modal referensi bantuan (F1 / Ctrl+Shift+H)
    ├── config/                 # Mesin konfigurasi Lua (mlua)
    │   ├── mod.rs              # Loader lunula.lua, pencarian path, & unit test
    │   ├── types.rs            # Definisi struct konfigurasi bertipe kuat
    │   └── defaults.rs         # Template DEFAULT_LUA_CONFIG & parser warna hex/RGB
    ├── terminal/               # Emulator VT100/ANSI & manajemen grid sel
    │   ├── mod.rs              # TerminalGrid, VecDeque scrollback O(1) + daur ulang baris
    │   ├── cell.rs             # Cell (20 byte), CellStyle (bitflags u8), palet ANSI/256
    │   ├── parser.rs           # Implementasi vte::Perform & parser SGR stack zero-alloc
    │   ├── renderer.rs         # Ekstraksi span teks & Run-Length Merging BgRect
    │   ├── procedural.rs       # Geometri vektor GPU untuk Box-Drawing, Block, & Powerline
    │   └── selection.rs        # Seleksi kata/baris & deteksi hyperlink OSC 8 / plaintext
    ├── pane/                   # Manajemen multi-tab & binary split tree
    │   ├── mod.rs              # TabManager, drain seluruh PTY (termasuk tab background)
    │   ├── tree.rs             # PaneNode binary split tree & kalkulasi navigasi arah
    │   └── item.rs             # State Pane tunggal, fisika kursor, & cache shaping baris
    ├── pty/                    # Antarmuka ConPTY Windows & background reader thread
    │   └── mod.rs              # PtyManager dengan AtomicBool exited & wake coalescing
    ├── luna/                   # Built-in micro-editor nano-style zero-dependency (luna.exe)
    │   ├── mod.rs              # Loop event console & orchestrator
    │   ├── buffer.rs           # TextBuffer dengan undo Ctrl+Z & search Ctrl+F
    │   ├── console.rs          # Win32 Console Raw Mode guard & event parser
    │   ├── syntax.rs           # Zero-alloc lexical syntax highlighter Kanagawa Wave
    │   └── ui.rs               # TUI frame renderer dengan gutter nomor baris
    ├── render/                 # Mesin teks Glyphon & pipeline background quad WGPU
    │   ├── mod.rs              # TextEngine dengan scratch_vertices zero-alloc per frame
    │   └── shaders/
    │       ├── bg.wgsl         # Shader WGSL untuk background quad & dekorasi
    │       └── wobble.wgsl     # Shader WGSL untuk efek jelly window & tombol kontrol
    ├── theme/                  # Multi-theme engine (Kanagawa, Tokyo Night, Catppuccin, Gruvbox, Nord)
    │   └── mod.rs              # Struct Theme, ColorRgb, ANSI TrueColor generator & parser Lua
    ├── wobble/                 # Simulasi pegas 2D & offscreen texture mapping
    │   ├── mod.rs              # WobbleWindow dengan static index buffer & dirty vertex upload
    │   └── physics.rs          # Solver mass-spring-damper dengan early-exit saat diam
    └── platform/               # Integrasi native Win32 FFI bebas crate eksternal
        ├── mod.rs              # GetCursorPos, GetLocalTime, & pembacaan branch .git/HEAD
        └── clipboard.rs        # Win32 Clipboard API (CF_UNICODETEXT) get_text & set_text
```

---

## ⌨️ Daftar Shortcut Utama

| Shortcut | Fungsi |
| :--- | :--- |
| `Ctrl + Shift + T` | Buka Tab baru |
| `Ctrl + Tab` / `Ctrl + Shift + Tab` | Pindah ke Tab berikutnya / sebelumnya |
| `Ctrl + 1..9` | Lompat langsung ke Tab 1 hingga 9 |
| `Ctrl + Shift + E` | Split Pane Vertikal (kiri / kanan) |
| `Ctrl + Shift + D` | Split Pane Horizontal (atas / bawah) |
| `Alt + Panah (← ↑ → ↓)` | Pindah fokus antar Split Pane |
| `Ctrl + Shift + W` | Tutup Split Pane aktif / Tab aktif |
| `Ctrl + C` / `Ctrl + Shift + C` | Salin teks yang diseleksi (atau kirim `SIGINT` jika tidak ada seleksi) |
| `Ctrl + V` / `Ctrl + Shift + V` / `Klik Kanan` | Tempel (*paste*) teks dari Clipboard |
| `Ctrl + Klik Kiri` | Buka tautan URL (`http://`, `https://`, atau OSC 8) di peramban |
| `Ctrl + Shift + F` | Pencarian cepat teks di scrollback (*Quick Search*) |
| `Ctrl + Shift + R` | *Hot-reload* berkas konfigurasi `lunula.lua` |
| `Ctrl + Shift + P` | Muat ulang profil PowerShell (`. $PROFILE`) |
| `Ctrl + ,` / `Tombol Gear ⚙️` | Buka Settings Modal & Configuration Hub |
| `Shift + F1` / `Ctrl + Shift + H` | Buka / tutup jendela panduan *shortcut* |
| `F11` | Beralih mode *Fullscreen* / *Maximize* |

---

## 🛠️ Cara Membangun & Menjalankan

```bash
# Jalankan seluruh unit test
cargo test

# Bangun kedua binary rilis teroptimasi (lunula_terminal.exe + luna.exe)
cargo build --release

# Jalankan editor bawaan langsung dari terminal:
luna lunula.lua
```
