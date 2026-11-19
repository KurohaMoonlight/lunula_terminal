# ⚙️ Panduan Konfigurasi Lengkap Lunula Terminal (`lunula.lua`)

Lunula Terminal dirancang dengan prinsip **Config-as-Code** berbasis Lua yang cepat, modular, dan bertipe kuat. Semua konfigurasi dapat diubah secara langsung dan diterapkan secara instan tanpa perlu menutup terminal (*hot-reload* via `Ctrl + Shift + R` atau tombol Gear `⚙️`).

---

## 📂 Struktur Berkas Konfigurasi Modular

Untuk mendukung prinsip *“highly customisable”*, konfigurasi Lunula Terminal dibagi menjadi struktur modular yang bersih:

```text
lunula/  (atau direktori tempat executable berjalan / ~/.config/lunula/)
├── lunula.lua           # Berkas konfigurasi utama (Window, Physics, Shell, Font)
├── colors.lua           # Kustomisasi warna UI & 16 palet warna ANSI terminal
├── keybinds.lua         # Kustomisasi shortcut keyboard & mouse
└── themes/              # Direktori tema kustom buatan pengguna
    └── cyberpunk.lua    # Contoh tema kustom buatan sendiri
```

> [!TIP]
> Di mana pun Anda membuka terminal (di folder `C:\`, `D:\Project`, atau USB), menekan **tombol Gear (⚙️)** di titlebar atau menekan **`Ctrl + ,`** akan membuka **Settings Modal**, di mana Anda bisa langsung memilih berkas konfigurasi mana yang ingin diedit menggunakan micro-editor bawaan `luna`.

---

## 📑 Rincian Setiap Opsi di `lunula.lua`

Berikut adalah rincian lengkap setiap bagian parameter di `lunula.lua`:

### 1. `theme` (Pilihan Tema Tampilan)
Menentukan tema warna global untuk Lunula Terminal sekaligus micro-editor `luna`.

```lua
theme = "kanagawa",
```

**Pilihan Tema Bawaan (*Built-in Presets*):**
- `"kanagawa"` *(Default)*: Palet autentik Kanagawa Wave (`#1F1F28`, `#7FB4CA`, `#957FB8`, `#98BB6C`).
- `"tokyo_night"`: Nuansa malam Tokyo Night Storm (`#24283B`, `#C0CAF5`, `#BB9AF7`, `#9ECE6A`).
- `"catppuccin"`: Nuansa lembut pastel Catppuccin Mocha (`#1E1E2E`, `#CDD6F4`, `#CBA6F7`, `#89B4FA`).
- `"gruvbox"`: Nuansa retro hangat Gruvbox Dark (`#282828`, `#EBDBB2`, `#FB4934`, `#83A598`).
- `"nord"`: Nuansa dingin kutub Nord Frost (`#2E3440`, `#ECEFF4`, `#81A1C1`, `#88C0D0`).
- **Tema Kustom**: Jika Anda mengisi nama file di folder `themes/` (misal `"cyberpunk"`), Lunula otomatis memuat berkas `themes/cyberpunk.lua`.

---

### 2. `window` (Geometri & Visual Jendela)
Mengatur dimensi awal, transparansi, dan animasi gelombang di titlebar.

```lua
window = {
    width = "auto",          -- "auto" atau angka pixel (misal 1100)
    height = "auto",         -- "auto" atau angka pixel (misal 700)
    width_percent = 0.62,    -- 62% dari lebar layar monitor saat mode "auto"
    height_percent = 0.68,   -- 68% dari tinggi layar monitor saat mode "auto"
    lock_size = false,       -- Jika true, ukuran tidak otomatis mengikuti resolusi monitor
    opacity = 0.95,          -- Transparansi jendela (0.1 = sangat transparan, 1.0 = solid)
    title = "Lunula Terminal", -- Judul jendela default
    animate_titlebar = true, -- Animasi gelombang ombak Kanagawa 60 FPS di titlebar
}
```

> [!NOTE]
> Jika menggunakan laptop hemat baterai, Anda dapat mengubah `animate_titlebar = false`. Terminal akan beralih ke mode hemat daya (*idle wait*) saat tidak ada ketikan.

---

### 3. `wobble` (Fisika Jendela Elastis 2D Jelly)
Mengontrol simulasi pegas elastis (*mass-spring-damper*) saat jendela digeser (*drag*) atau dilempar.

```lua
wobble = {
    stiffness = 420.0,       -- Kekakuan pegas (lebih tinggi = lebih kaku, lebih rendah = lebih jelly)
    damping = 7.0,           -- Redaman getaran (kecepatan osilasi berhenti)
    max_deform = 55.0,       -- Deformasi elastis maksimal (dalam pixel) saat digeser kencang
    spring_shear = 300.0,    -- Kekakuan distorsi geser diagonal
    wave_reactivity = 1.0,   -- Reaktivitas ombak titlebar terhadap dorongan getaran (0.0 - 3.0)
}
```

---

### 4. `terminal` (Font & Buffer Teks)
Mengontrol tipografi monospace dan kapasitas riwayat scrollback.

```lua
terminal = {
    font_family = "0xProto Nerd Font Mono", -- Nama font monospace di sistem Anda
    font_size = 18.0,                        -- Ukuran font (dalam pt)
    max_scrollback = 5000,                  -- Batas baris riwayat scrollback per split pane
}
```

---

### 5. `cursor` (Bentuk & Efek Kursor)
Mengatur tampilan kursor teks terminal dan efek jejak elastis (*Kitty-style elastic cursor trail*).

```lua
cursor = {
    shape = "block",         -- Bentuk kursor: "block", "beam", atau "underline"
    blinking = true,         -- Animasi kedip lembut (smooth sine wave blinking)
    color = "#7fb4ca",       -- Warna kursor (hex code atau tabel {r, g, b})
    trail = true,            -- Mengaktifkan ekor jejak elastis saat kursor melompat
    trail_stiffness = 550.0, -- Kecepatan ekor kursor mengejar kepala kursor
    trail_damping = 48.0,    -- Redaman ekor kursor agar tidak bergetar berlebihan
}
```

---

### 6. `scroll` (Smooth Sub-Pixel Scrolling)
Mengatur kehalusan scroll roda mouse.

```lua
scroll = {
    smooth = true,           -- Mengaktifkan interpolasi pixel Y-offset 60-120 FPS
    speed = 60.0,            -- Kecepatan interpolasi scroll
}
```

---

### 7. `tabs` (Multi-Tab & Split Panes)
Mengatur bar navigasi tab dan efek peredupan (*dimming*) pada split pane yang tidak aktif.

```lua
tabs = {
    show_single_tab = false, -- Sembunyikan bar tab jika hanya ada 1 tab aktif
    height = 32.0,           -- Tinggi bar tab dalam pixel
    dim_inactive_panes = true, -- Redupkan split pane yang sedang tidak fokus
    dim_opacity = 0.35,      -- Tingkat kegelapan redup (0.0 = terang normal, 0.35 = elegan)
}
```

---

### 8. `colors` (Warna UI & Seleksi Teks)
Menentukan warna seleksi teks mouse. Untuk kustomisasi 16 palet ANSI lengkap, gunakan berkas `colors.lua`.

```lua
colors = {
    selection = "#2d4f67",   -- Warna highlight seleksi teks mouse
}
```

---

### 9. `fetch` (Utilitas Pendamping `lfetch`)
Mengatur animasi default untuk program fetch 3D bawaan `lfetch`.

```lua
fetch = {
    animation = 2,           -- 1 = Ombak Kanagawa & Bulan, 2 = Bulan 3D berotasi & kawah
    live = true,             -- true = animasi live 60 FPS, false = cetak 1 frame instan (<5ms)
}
```

---

### 10. `shell` (Program Shell Default)
Menentukan shell yang dijalankan saat terminal pertama kali dibuka atau saat membuka tab/split pane baru.

```lua
shell = {
    program = "powershell.exe", -- "powershell.exe", "pwsh.exe", "cmd.exe", "wsl.exe", atau "bash.exe"
    args = { "-NoLogo" },       -- Argumen startup shell
}
```

**Contoh Shell Lain:**
- **PowerShell 7 (Cross-Platform)**: `program = "pwsh.exe", args = { "-NoLogo" }`
- **WSL (Ubuntu / Linux)**: `program = "wsl.exe", args = { "~" }`
- **Command Prompt**: `program = "cmd.exe", args = {}`
- **Git Bash**: `program = "C:\\Program Files\\Git\\bin\\bash.exe", args = { "--login", "-i" }`

---

## 🎨 Kustomisasi 16 Warna ANSI Terminal (`colors.lua`)

Berkas `colors.lua` memungkinkan Anda meng-override ke-16 palet warna terminal yang digunakan oleh perintah CLI (`ls`, `git status`, `cargo`, PowerShell syntax highlighting, `htop`, dll.):

```lua
return {
    selection  = "#2D4F67",
    
    -- 16 Warna ANSI Standar:
    ansi = {
        [0]  = "#16161D", -- Black
        [1]  = "#C34043", -- Red
        [2]  = "#76946A", -- Green
        [3]  = "#C0A36E", -- Yellow
        [4]  = "#7E9CD8", -- Blue
        [5]  = "#957FB8", -- Magenta
        [6]  = "#6A9589", -- Cyan
        [7]  = "#C8C093", -- White
        [8]  = "#727169", -- Bright Black
        [9]  = "#E82424", -- Bright Red
        [10] = "#98BB6C", -- Bright Green
        [11] = "#E6C384", -- Bright Yellow
        [12] = "#7FB4CA", -- Bright Blue
        [13] = "#D27E99", -- Bright Magenta
        [14] = "#7AA89F", -- Bright Cyan
        [15] = "#DCD7BA", -- Bright White
    },
}
```

---

## 🎭 Cara Membuat Tema Sendiri di `themes/`

Anda bisa membuat tema baru tanpa batas! Cukup buat file baru di folder `themes/`, misalnya `themes/matrix.lua`:

```lua
return {
    name = "matrix",
    bg = "#0d1117",
    fg = "#00ff66",
    selection = "#003b1f",
    cursor = "#00ff66",
    tab_bar = "#05080c",
    tab_active = "#161b22",
    border = "#00ff66",
    syntax = {
        keyword = "#00ff66",
        type = "#39ff14",
        string = "#79c0ff",
        number = "#d2a8ff",
        comment = "#484f58",
        punctuation = "#8b949e",
        header = "#56d364",
    },
    ansi = {
        [0] = "#0d1117", [1] = "#ff7b72", [2] = "#3fb950", [3] = "#d29922",
        [4] = "#58a6ff", [5] = "#bc8cff", [6] = "#39c5cf", [7] = "#b1bac4",
        [8] = "#484f58", [9] = "#ffa198", [10] = "#56d364", [11] = "#e3b341",
        [12] = "#79c0ff", [13] = "#d2a8ff", [14] = "#56d4dd", [15] = "#f0f6fc",
    }
}
```

Lalu di `lunula.lua`, set:
```lua
theme = "matrix",
```
Tekan `Ctrl + Shift + R`, dan terminal Anda seketika bertema **Matrix**!
