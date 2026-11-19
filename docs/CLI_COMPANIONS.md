# 🧰 Program Pendamping CLI Bawaan (*CLI Companions*)

Lunula Terminal hadir dengan rangkaian program utilitas mandiri (*companion binaries*) yang dirancang khusus untuk memaksimalkan kapabilitas GPU terminal tanpa memerlukan dependensi pihak ketiga yang berat:

---

## 1. 🌙 `lfetch` — 3D Raymarched Terminal Fetch Tool

`lfetch` adalah alternatif *ultra-lightweight* untuk `neofetch` / `fastfetch` yang menampilkan informasi sistem (OS, Kernel, CPU, GPU, Memori, Uptime, Resolusi) disertai render ASCII 3D beranimasi halus:

### Mode Tampilan:
- **Mode 1**: 3D Raymarched Moon & Great Wave Kanagawa (ombak bergulung dengan buih air dinamis).
- **Mode 2**: 3D Analytical Moon berotasi dengan kalkulasi iluminasi kawah dan gelombang laut Kanagawa.

### Penggunaan CLI:
```bash
# Menjalankan animasi live 60 FPS di terminal (tekan q untuk keluar)
lfetch

# Menampilkan 1 frame statis secara instan (<5 ms) cocok untuk startup shell di $PROFILE
lfetch --static

# Memilih variasi animasi secara manual (1 atau 2)
lfetch --anim 1
lfetch --anim 2
```

---

## 2. 🖼️ `limg` — Inline Terminal Graphics Viewer

`limg` adalah alat penampil gambar langsung di dalam terminal (*inline graphics viewer*) yang menggunakan protokol modern **Kitty Graphics Protocol** (APC / DCS G) yang diakselerasi langsung oleh GPU Lunula:

### Keunggulan:
- Mendukung format gambar **PNG**, **JPEG**, dan buffer piksel mentah RGBA.
- Otomatis menyesuaikan aspek rasio gambar terhadap ukuran sel karakter teks di terminal.
- Gambar dapat di-scroll secara halus mengikuti riwayat scrollback terminal tanpa merusak tata letak teks.

### Penggunaan CLI:
```bash
# Menampilkan gambar langsung di grid terminal
limg gambar.png

# Menyesuaikan lebar maksimum (dalam sel kolom karakter)
limg screenshot.jpg --cols 60
```

---

## 3. 📦 `lpm` — Lunula Package & Theme Manager

`lpm` adalah utilitas baris perintah untuk mengelola tema kustom, plugin, dan validasi konfigurasi Lunula Terminal:

### Penggunaan CLI:
```bash
# Memeriksa integritas berkas konfigurasi lunula.lua
lpm check

# Menampilkan daftar tema yang terpasang di folder themes/
lpm themes list

# Menguji tema tertentu
lpm themes preview cyberpunk
```
