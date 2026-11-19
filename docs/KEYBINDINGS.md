# ⌨️ Panduan Lengkap Navigasi & Shortcut Lunula Terminal

Dokumen ini berisi referensi lengkap kombinasi tombol keyboard dan interaksi mouse di **Lunula Terminal**, termasuk cara memodifikasinya di berkas `keybinds.lua`.

---

## 📑 Tabel Pintasan Keyboard Utama

### 1. Manajemen Split Pane (Multi-Pane)
| Shortcut | Tindakan | Keterangan |
| :--- | :--- | :--- |
| `Ctrl + Shift + E` | **Split Vertikal** | Membagi split pane aktif ke kiri & kanan (*side-by-side*) |
| `Ctrl + Shift + D` | **Split Horizontal** | Membagi split pane aktif ke atas & bawah (*stacked*) |
| `Ctrl + Shift + W` | **Tutup Pane** | Menutup pane aktif (atau menutup tab jika pane terakhir) |
| `Alt + Panah (← ↑ → ↓)` | **Pindah Fokus** | Mengalihkan fokus kursor antar split pane |
| Perintah `exit` | **Tutup Pane** | Mengetik `exit` di shell otomatis membersihkan pane tersebut |
| Tombol `[✕]` | **Tutup via Mouse** | Klik badge silang merah di sudut kanan atas tiap pane |

> [!NOTE]
> `Ctrl + W` biasa tetap diteruskan ke shell sebagai karakter `^W` standar (menghapus satu kata sebelumnya di Bash / PowerShell / Zsh) dan tidak membajak window Anda. Untuk menutup pane gunakan **`Ctrl + Shift + W`**.

---

### 2. Manajemen Tab (Multi-Tab)
| Shortcut | Tindakan | Keterangan |
| :--- | :--- | :--- |
| `Ctrl + Shift + T` | **Tab Baru** | Membuka tab terminal baru |
| `Ctrl + Tab` | **Tab Berikutnya** | Beralih maju ke tab selanjutnya |
| `Ctrl + Shift + Tab` | **Tab Sebelumnya** | Beralih mundur ke tab sebelumnya |
| `Ctrl + 1` s/d `Ctrl + 9` | **Lompat ke Tab** | Langsung menuju tab ke-1 hingga tab ke-9 |
| Tombol `[+]` di Tab Bar | **Tab Baru via Mouse** | Klik tombol plus di samping bar tab |

---

### 3. Pengaturan & Bantuan Jendela
| Shortcut | Tindakan | Keterangan |
| :--- | :--- | :--- |
| `Ctrl + ,` | **Settings Modal** | Membuka jendela menu konfigurasi & file modular |
| Tombol Gear `⚙️` | **Settings Modal** | Klik ikon gear di titlebar untuk membuka Settings Hub |
| `Shift + F1` | **Help Reference** | Membuka modal ringkasan shortcut keyboard |
| `Ctrl + Shift + H` | **Help Reference** | Alternatif membuka modal ringkasan shortcut |
| `Ctrl + Shift + R` | **Hot-Reload Config** | Menerapkan ulang isi `lunula.lua` tanpa restart |
| `Ctrl + Shift + P` | **Reload Profile** | Memuat ulang profil PowerShell pengguna (`. $PROFILE`) |
| `F11` | **Fullscreen / Maximize** | Beralih mode layar penuh tanpa border |

> [!TIP]
> Di dalam **Settings Modal (`Ctrl + ,`)**:
> - Tekan angka **`1`** s/d **`6`** untuk langsung membuka berkas konfigurasi.
> - Atau gunakan panah **`↑` / `↓`** (atau **`k` / `j`**) dan tekan **`Enter`**.
> - Tekan **`Esc`** atau **`q`** untuk menutup modal.

---

### 4. Pencarian Cepat Teks (*Quick Search*)
| Shortcut | Tindakan | Keterangan |
| :--- | :--- | :--- |
| `Ctrl + Shift + F` | **Buka Quick Search** | Memunculkan kotak pencarian di bagian bawah pane |
| Ketik teks | **Pencarian Real-Time** | Hasil match langsung disorot secara instan |
| `Enter` | **Match Berikutnya** | Melompat ke kemunculan teks selanjutnya |
| `Shift + Enter` | **Match Sebelumnya** | Melompat ke kemunculan teks sebelumnya |
| `Esc` | **Tutup Pencarian** | Menutup search bar dan kembali ke kursor shell |

---

### 5. Vi Copy Mode (Navigasi Keyboard Bebas Mouse)
Bagi pengguna Vim/Neovim yang ingin menyeleksi dan menyalin riwayat teks tanpa menyentuh mouse:

| Shortcut | Tindakan |
| :--- | :--- |
| `Ctrl + Shift + Space` | Masuk / Keluar Vi Mode |
| `h`, `j`, `k`, `l` | Gerakan kursor (Kiri, Bawah, Atas, Kanan) |
| `w`, `b`, `e` | Lompat maju kata, mundur kata, akhir kata |
| `0`, `$` | Lompat ke awal baris, akhir baris |
| `Ctrl + U` / `Ctrl + D` | Gulir setengah layar ke atas / ke bawah |
| `v` | Mulai seleksi karakter (*Visual Character Mode*) |
| `V` | Mulai seleksi baris penuh (*Visual Line Mode*) |
| `y` | Salin teks terseleksi ke Windows Clipboard (*Yank*) |
| `/` | Cari kata di scrollback ke arah bawah |
| `?` | Cari kata di scrollback ke arah atas |
| `n` / `N` | Loncat ke match pencarian berikutnya / sebelumnya |
| `f` / `F` | Pindai seluruh URL tautan di layar dan beri label loncat instan |

---

### 6. Interaksi Mouse & Clipboard
- **Seleksi Teks**: Klik kiri dan drag mouse untuk menyeleksi teks.
- **Copy Teks**: `Ctrl + C` atau `Ctrl + Shift + C` saat ada seleksi teks (jika tidak ada seleksi, `Ctrl + C` mengirim sinyal `SIGINT` ke proses CLI).
- **Paste Teks**: `Ctrl + V`, `Ctrl + Shift + V`, atau cukup **Klik Kanan Mouse**.
- **Buka Hyperlink**: Tekan **`Ctrl + Klik Kiri`** pada teks URL (`http://...`, `https://...`, atau OSC 8) untuk langsung membukanya di browser default Anda.
- **Smooth Sub-Pixel Scroll**: Putar roda mouse untuk menggulir riwayat terminal dengan interpolasi pixel halus 60-120 FPS.

---

## 🛠️ Kustomisasi Keybindings di `keybinds.lua`

Anda dapat menyesuaikan shortcut sesuai kebiasaan Anda di berkas `keybinds.lua`:

```lua
return {
    ["Ctrl+Shift+T"]   = "new_tab",
    ["Ctrl+Shift+W"]   = "close_pane",
    ["Ctrl+Tab"]       = "next_tab",
    ["Ctrl+Shift+Tab"] = "prev_tab",
    ["Ctrl+Shift+E"]   = "split_vertical",
    ["Ctrl+Shift+D"]   = "split_horizontal",
    ["Ctrl+Shift+F"]   = "search",
    ["Ctrl+Shift+R"]   = "reload_config",
    ["Ctrl+Shift+H"]   = "toggle_help",
    ["Ctrl+,"]         = "toggle_settings",
    ["F11"]            = "toggle_fullscreen",
}
```
