<p align="center">
  <img src="assets/icon.png" alt="Lunula Terminal Logo" width="160" height="160" />
</p>

<h1 align="center">🌙 Lunula Terminal</h1>

<p align="center">
  <strong>Ultra-Lightweight GPU Terminal Emulator with Elastic Wobble Physics & Built-in Developer Suite</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Release-v0.1.0-magenta?style=flat-square" alt="Version" />
  <img src="https://img.shields.io/badge/Platform-Windows%20x86__64-blue?style=flat-square&logo=windows" alt="Platform" />
  <img src="https://img.shields.io/badge/Renderer-WGPU%20%2B%20Glyphon-cyan?style=flat-square" alt="Renderer" />
  <img src="https://img.shields.io/badge/Binary%20Size-%3C10%20MB-emerald?style=flat-square" alt="Binary Size" />
  <img src="https://img.shields.io/badge/Unit%20Tests-49%20Passing-green?style=flat-square" alt="Tests" />
  <img src="https://img.shields.io/badge/Hot--Reload-Lua%205.4-purple?style=flat-square&logo=lua" alt="Lua" />
</p>

---

**Lunula Terminal** adalah emulator terminal modern berbasis GPU (`wgpu` + `glyphon`) yang dirancang dengan filosofi *ultra-lightweight*, estetika minimalis, dan animasi fisika elastis (*jelly wobble*). Lunula hadir sebagai satu kesatuan suite mandiri (*single executable*) berukuran kurang dari 10 MB, tanpa dependensi runtime berat, lengkap dengan text editor modal bawaan (**luna**), system fetcher 3D (**lfetch**), penampil gambar terminal (**limg**), dan package/theme manager (**lpm**).

---

## 💎 Fitur Unggulan

* ⚡ **GPU-Accelerated Rendering**: Rendering font sub-pixel berkecepatan tinggi via `wgpu` dan `glyphon` dengan dukungan refresh rate 120/144Hz, *zero-allocation per frame*, dan konsumsi CPU minimal.
* 🍮 **Elastic Jelly Wobble Physics**: Simulasi pegas 2D (*mass-spring-damper*) dinamis pada pergerakan dan drag jendela, memberikan karakter visual yang hidup dan menyenangkan.
* 🪟 **Tiling Binary Split Panes & Tabs**: Buka multi-tab dan bagi layar secara vertikal maupun horizontal sesuka hati dengan navigasi keyboard intuitif (`Alt + Arrows`).
* ⚙️ **Modular Lua Configuration**: Konfigurasi modular terpisah yang dapat di-*hot-reload* seketika saat disimpan:
  * [`lunula.lua`](file:///e:/My%20Projects/lunula_terminal/lunula.lua) — Pengaturan umum, fisika wobble, font, dan shell.
  * [`colors.lua`](file:///e:/My%20Projects/lunula_terminal/colors.lua) — Kustomisasi 16 palet ANSI dan override warna UI.
  * [`keybinds.lua`](file:///e:/My%20Projects/lunula_terminal/keybinds.lua) — Pemetaan shortcut keyboard kustom secara dinamis.
* 🌙 **Micro-Editor Bawaan (`luna`)**: Text editor modal ultra-cepat (<400 KB) dengan *lexical syntax highlighting* (Rust, Lua, Python, JS/TS, Go, JSON) dan antarmuka TUI berbasis palet Kanagawa Wave.
* 🔍 **Alacritty-Style Vi Mode & Regex Hints**: Jelajahi riwayat scrollback tanpa mouse, lakukan pencarian regex, visual selection, copy/yank, serta pemindaian tautan URL, hash commit, dan path file secara instan (`Ctrl+Shift+U`).
* 🖼️ **Kitty Graphics Protocol**: Dukungan tampilan gambar grafis resolusi penuh langsung di dalam terminal melalui utilitas pendamping `limg`.
* 🛡️ **Zero Runtime Dependencies**: Seluruh dependensi terkompilasi statis (*self-contained*), tidak memerlukan instalasi runtime eksternal.

---

## 🚀 Panduan Instalasi & Penggunaan

### Opsi 1: Unduh Paket Rilis Standalone (Rekomendasi)
Unduh paket rilis resmi dari tab [GitHub Releases](https://github.com/lunula-terminal/releases) atau gunakan arsip di folder [dist/](file:///e:/My%20Projects/lunula_terminal/dist/):

1. Unduh dan ekstrak `lunula-terminal-v0.1.0-windows-x86_64.zip`.
2. **Instalasi Otomatis**: Cukup *double-click* file `install.bat`.
   * Memasang Lunula Terminal ke `%LOCALAPPDATA%\Programs\Lunula\bin`.
   * Mendaftarkan perintah `lunula`, `luna`, `lfetch`, `limg`, dan `lpm` ke `PATH` Windows.
   * Menyiapkan template konfigurasi aman di `%APPDATA%\lunula\`.
   * Membuat pintasan (*shortcut*) di Start Menu dan Desktop.
3. **Mode Portable**: Tidak ingin menginstal? Cukup jalankan `Lunula.bat` (atau `bin\lunula_terminal.exe`) langsung dari folder mana pun!

### Opsi 2: Kompilasi Mandiri dari Source Code
Pastikan Anda telah memasang Rust toolchain (stable):

```bash
# Clone repositori
git clone https://github.com/lunula-terminal/lunula_terminal.git
cd lunula_terminal

# Jalankan seluruh unit test
cargo test

# Bangun seluruh biner rilis teroptimasi
cargo build --release

# Jalankan terminal
./target/release/lunula_terminal.exe
```

---

## 🧰 Paket Utilitas Pendamping (CLI Suite)

| Eksekutabel | Ukuran | Deskripsi & Fungsi |
| :--- | :--- | :--- |
| **`lunula_terminal.exe`** | ~9.6 MB | Emulator terminal GPU utama dengan ConPTY & animasi wobble |
| **`lunula.exe`** | Alias | Pintasan CLI cepat untuk meluncurkan terminal |
| **`luna.exe`** | ~400 KB | Modal code editor bergaya nano dengan syntax highlighting |
| **`lfetch.exe`** | ~800 KB | System fetcher dengan render logo bulan 3D raymarched |
| **`limg.exe`** | ~330 KB | Penampil gambar inline Kitty Graphics protocol di terminal |
| **`lpm.exe`** | ~430 KB | Package & theme manager untuk mengunduh tema kustom |

---

## ⌨️ Pintasan Keyboard Utama

| Shortcut | Aksi / Fungsi |
| :--- | :--- |
| `Ctrl + Shift + T` | Buka Tab baru |
| `Ctrl + Tab` / `Ctrl + Shift + Tab` | Pindah ke Tab berikutnya / sebelumnya |
| `Ctrl + 1..9` | Lompat langsung ke Tab 1 hingga 9 |
| `Ctrl + Shift + E` | Split Pane Vertikal (kiri / kanan) |
| `Ctrl + Shift + D` | Split Pane Horizontal (atas / bawah) |
| `Alt + Panah (← ↑ → ↓)` | Pindah fokus navigasi antar Split Pane |
| `Ctrl + Shift + W` | Tutup Split Pane aktif / Tab aktif |
| `Ctrl + C` / `Ctrl + Shift + C` | Salin teks seleksi (atau kirim `SIGINT` ke proses) |
| `Ctrl + V` / `Klik Kanan` | Tempel (*paste*) teks dari Clipboard |
| `Ctrl + Shift + F` | Pencarian cepat teks riwayat (*Quick Search*) |
| `Ctrl + Shift + Space` | Beralih ke **Vi Navigation Mode** |
| `Ctrl + Shift + U` | Pindai & sorot **Regex Hints** (URL, path, git hash) |
| `Ctrl + ,` / `Tombol Gear ⚙️` | Buka **Settings & Configuration Hub** |
| `Shift + F1` / `Tombol ?` | Buka jendela referensi bantuan (*Help Modal*) |
| `Ctrl + Shift + R` | Muat ulang konfigurasi seketika (*Hot-Reload*) |
| `Ctrl + Shift + P` | Muat ulang profil PowerShell (`. $PROFILE`) |
| `F11` | Beralih mode Layar Penuh (*Fullscreen / Maximize*) |

> [!TIP]
> Seluruh pintasan keyboard di atas dapat diubah dengan mudah melalui berkas [`keybinds.lua`](file:///e:/My%20Projects/lunula_terminal/keybinds.lua) dan akan otomatis ter-update pada jendela bantuan (**Help Modal**)!

---

## 📚 Dokumentasi Terperinci (`docs/`)

Untuk panduan mendalam, silakan baca dokumentasi teknis di folder [`docs/`](file:///e:/My%20Projects/lunula_terminal/docs/):

* ⚙️ **[Panduan Konfigurasi Lengkap (CONFIGURATION.md)](file:///e:/My%20Projects/lunula_terminal/docs/CONFIGURATION.md)**: Rincian parameter Lua, kalibrasi fisika pegas, dan pembuatan tema warna kustom.
* ⌨️ **[Daftar Keybindings & Navigasi (KEYBINDINGS.md)](file:///e:/My%20Projects/lunula_terminal/docs/KEYBINDINGS.md)**: Panduan lengkap Vi mode, Regex hints, dan pemetaan pintasan keyboard.
* 🌙 **[Panduan Editor luna (LUNA_EDITOR.md)](file:///e:/My%20Projects/lunula_terminal/docs/LUNA_EDITOR.md)**: Fitur editing, pencarian cepat, integrasi Git core editor, dan sistem syntax highlighting.
* 🧰 **[Utilitas Pendamping CLI (CLI_COMPANIONS.md)](file:///e:/My%20Projects/lunula_terminal/docs/CLI_COMPANIONS.md)**: Panduan penggunaan `lfetch`, `limg`, dan `lpm`.
* 🔍 **[Pemecahan Masalah & Solusi (TROUBLESHOOTING.md)](file:///e:/My%20Projects/lunula_terminal/docs/TROUBLESHOOTING.md)**: Penanganan font Nerd Font, optimalisasi GPU hemat daya, dan diagnostik.
* 🏗️ **[Arsitektur Teknis (ARCHITECTURE.md)](file:///e:/My%20Projects/lunula_terminal/docs/ARCHITECTURE.md)**: Penjelasan arsitektur internal modul, alur pipeline WGPU, dan integrasi ConPTY.
* ⚡ **[Optimasi Performa & Memori (OPTIMIZATIONS.md)](file:///e:/My%20Projects/lunula_terminal/docs/OPTIMIZATIONS.md)**: Rincian 7 strategi optimasi memori dan *zero-allocation render pass*.

---

## 🏛️ Prinsip Inti (`principle.md`)

```text
"ultra-lightweight terminal, highly customisable, single executable,
 no dependencies, focus on speed, minimalism, animation"
```

1. **Kecepatan Maksimal**: Latensi input mendekati nol (*zero-latency keystrokes*), pipeline render langsung ke hardware GPU.
2. **Kustomisasi Fleksibel**: Pengguna memegang kendali penuh atas tampilan, warna, tombol pintas, dan fisika melalui Lua.
3. **Satu Kesatuan Biner Mandiri**: Tidak ada ketergantungan pada pustaka runtime eksternal; biner siap pakai di komputer mana pun.
4. **Estetika & Karakter**: Menggabungkan fungsionalitas terminal hacker yang tangguh dengan animasi fisika modern yang halus.

---

## 📄 Lisensi

Didistribusikan di bawah lisensi MIT / Apache-2.0. Silakan gunakan, modifikasi, dan distribusikan secara bebas.
Dikembangkan dengan ❤️ untuk komunitas developer dan terminal enthusiast.
