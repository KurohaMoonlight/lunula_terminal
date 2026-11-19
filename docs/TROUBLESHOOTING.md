# 🔍 Panduan Pemecahan Masalah & Optimasi (*Troubleshooting*)

Dokumen ini berisi panduan penanganan masalah umum serta kiat optimasi saat menggunakan **Lunula Terminal**.

---

## 1. Ikon / Simbol Nerd Font Muncul Kotak-Kotak (Missing Glyphs)

### Penyebab:
Font yang digunakan di `lunula.lua` belum terpasang di sistem Windows Anda atau bukan varian *Nerd Font* (yang memuat glif khusus Powerline, Git branch, icon folder, dll.).

### Solusi:
1. Unduh font monospace bervarian Nerd Font (misalnya: **JetBrainsMono Nerd Font**, **0xProto Nerd Font Mono**, atau **FiraCode Nerd Font**) dari [Nerd Fonts Releases](https://www.nerdfonts.com/).
2. Pasang font tersebut di Windows (klik kanan `.ttf` / `.otf` $\rightarrow$ *Install for all users*).
3. Buka `lunula.lua` (via tombol Gear ⚙️ atau `Ctrl + ,`), lalu sesuaikan namanya:
   ```lua
   terminal = {
       font_family = "JetBrainsMono Nerd Font",
       font_size = 16.0,
   }
   ```
4. Tekan `Ctrl + Shift + R` untuk memuat ulang konfigurasi.

---

## 2. Menghemat Daya Baterai Laptop

Secara default, Lunula Terminal menyalakan animasi gelombang ombak Kanagawa 60 FPS di titlebar dan fisika getaran jelly window. Di desktop atau saat dicolokkan ke adaptor daya, fitur ini menggunakan sangat sedikit daya GPU. Namun saat bepergian dengan baterai laptop, Anda dapat mengaktifkan profil hemat daya:

### Solusi Hemat Baterai di `lunula.lua`:
```lua
window = {
    animate_titlebar = false, -- Matikan animasi ombak live di titlebar (beralih ke idle wait mode)
}
wobble = {
    wave_reactivity = 0.0,    -- Matikan reaktivitas ombak terhadap gerakan drag
}
```
Ketika `animate_titlebar = false`, Lunula Terminal akan memasuki mode tidur pulas (*event-driven wait*), menggunakan 0% CPU dan 0% GPU ketika tidak ada input ketikan atau output shell.

---

## 3. Karakter Khusus / Bahasa Asing (IME & Emojis)

Lunula Terminal mendukung penuh masukan teks Unicode, IME (Japanese, Chinese, Korean, dll.), dan pengetik emoji native Windows (`Win + .`):
- Jendela IME dan kandidat kata otomatis diletakkan tepat di bawah posisi kursor aktif terminal.
- Jika kursor berpindah, Lunula Terminal otomatis memperbarui area kursor IME ke Windows Subsystem.

---

## 4. Masalah Terminal Crash Saat Menjalankan Perintah Tertentu

### Diagnostik:
Jalankan Lunula Terminal melalui Command Prompt atau PowerShell biasa untuk melihat log error:
```powershell
$env:RUST_LOG="info"
.\target\release\lunula_terminal.exe
```

### Tips Umum:
- Pastikan Windows 10/11 Anda memiliki ConPTY yang mutakhir (disertakan secara default di Windows 10 build 1809 ke atas).
- Jika perintah shell keluar secara tiba-tiba, split pane otomatis menutup secara aman dan rapi tanpa menyebabkan crash memori.
