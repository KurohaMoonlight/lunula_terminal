# 🌙 Luna Micro-Editor (`luna.exe`)

**`luna`** adalah editor teks berbasis terminal (*TUI micro-editor*) bawaan Lunula Terminal. Didesain sebagai alternatif modern untuk `nano` yang ultra-ringan (<400 KB), mandiri tanpa dependensi eksternal (*zero external heavy dependencies*), memiliki waktu start instan (<5 ms), dan mendukung pewarnaan sintaksis ANSI TrueColor yang otomatis tersinkronisasi dengan tema Lunula Terminal.

---

## ✨ Fitur Utama `luna`

1. **Ultra-Lightweight & Single Executable**:
   - Berukuran di bawah 400 KB dalam build rilis teroptimasi.
   - Menggunakan raw console native Windows Console API tanpa ketergantungan library runtime eksternal.
2. **Lexical Syntax Highlighting Cepat**:
   - Mengenali dan mewarnai token kode secara otomatis:
     - **Lua** (`.lua`)
     - **Rust** (`.rs`)
     - **Shell & PowerShell** (`.ps1`, `.psm1`, `.sh`, `.bash`, `.zsh`, `.cmd`, `.bat`)
     - **Config** (`.json`, `.toml`, `.yaml`, `.yml`, `.ini`, `.conf`, `.omp.json`, `Cargo.lock`)
     - **Python** (`.py`)
     - **Markdown** (`.md`)
3. **Sinkronisasi Tema Otomatis**:
   - Otomatis membaca konfigurasi dari `lunula.lua`.
   - Jika tema terminal diubah ke `catppuccin`, `tokyo_night`, `gruvbox`, `nord`, atau tema kustom di `themes/`, tampilan antarmuka dan warna syntax `luna` langsung berubah serasi.
4. **Fitur Pengeditan Modern**:
   - Undo bertingkat via `Ctrl + Z`.
   - Pencarian teks via `Ctrl + F`.
   - Konfirmasi penyimpanan otomatis saat keluar via `Ctrl + X`.
   - Scrolling horizontal otomatis untuk baris kode panjang tanpa merusak nomor baris atau format warna.

---

## 🚀 Cara Menjalankan

### Membuka Berkas di Direktori Aktif
```bash
luna nama_berkas.lua
```

### Membuka Berkas Baru
```bash
luna berkas_baru.rs
```

### Membuka Konfigurasi Lunula Terminal Langsung
Tekan tombol **Gear (⚙️)** di titlebar atau **`Ctrl + ,`**, lalu pilih **`[1] Edit Master Config`**. Terminal akan otomatis membuka berkas konfigurasi dengan path mutlak di `luna`.

---

## ⌨️ Pintasan Tombol (*Keybindings*) di `luna`

| Shortcut | Fungsi | Keterangan |
| :--- | :--- | :--- |
| `Ctrl + S` | **Simpan Berkas** | Menyimpan perubahan buffer ke disk |
| `Ctrl + X` | **Keluar (*Exit*)** | Keluar dari editor (meminta konfirmasi jika ada modifikasi) |
| `Ctrl + F` | **Cari Teks (*Find*)** | Membuka baris pencarian query teks |
| `Ctrl + Z` | **Urungkan (*Undo*)** | Mengembalikan perubahan teks sebelumnya secara bertahap |
| `Panah (← ↑ → ↓)` | **Navigasi Kursor** | Menggeser kursor karakter dan baris |
| `Home` / `End` | **Awal / Akhir Baris** | Melompat ke kolom pertama / terakhir pada baris |
| `Page Up` / `Page Down` | **Gulir Layar** | Menggulir satu layar penuh ke atas / ke bawah |
| `Enter` | **Baris Baru** | Memecah baris teks dengan indentasi |
| `Backspace` / `Del` | **Hapus Karakter** | Menghapus karakter di kiri / kanan kursor |

---

## 🔧 Menjadikan `luna` sebagai Editor Default

Karena `luna.exe` adalah executable mandiri, Anda dapat menggunakannya sebagai editor default untuk berbagai alat pengembang:

### 1. Di Git
Jadikan `luna` sebagai editor pesan commit:
```bash
git config --global core.editor "luna"
```

### 2. Di Profil PowerShell (`$PROFILE`)
Tambahkan baris berikut di `$PROFILE`:
```powershell
Set-Alias -Name edit -Value luna
$env:EDITOR = "luna"
```
Sekarang Anda cukup mengetik `edit file.txt` untuk membuka `luna` seketika!
