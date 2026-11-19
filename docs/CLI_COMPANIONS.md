# 🧰 Built-in CLI Companion Utilities

Lunula Terminal ships with a lightweight suite of companion CLI utilities designed to harness the terminal's GPU capabilities without heavy external runtime dependencies:

---

## 1. 🌙 `lfetch` — 3D Raymarched Terminal Fetch Tool

`lfetch` is an ultra-lightweight alternative to `neofetch` / `fastfetch` that displays comprehensive system telemetry (OS, Kernel, CPU, GPU, Memory, Uptime, Screen Resolution) alongside a smooth animated 3D ASCII render:

### Animation Modes:
* **Mode 1**: 3D Raymarched Moon & Great Wave of Kanagawa (rolling wave physics with procedural foam).
* **Mode 2**: 3D Analytical Rotating Moon with calculated crater lighting and oceanic waves.

### CLI Usage:
```bash
# Run 60 FPS live interactive animation (press 'q' to exit)
lfetch

# Output 1 static frame instantly (<5 ms) — ideal for PowerShell/Bash startup profiles
lfetch --static

# Select animation variant manually (1 or 2)
lfetch --anim 1
lfetch --anim 2
```

---

## 2. 🖼️ `limg` — Inline Terminal Graphics Viewer

`limg` is a high-performance inline image viewer utilizing the modern **Kitty Graphics Protocol** (APC / DCS G) accelerated directly by Lunula's WGPU pipeline:

### Capabilities:
* Supports **PNG**, **JPEG**, and raw RGBA pixel buffers.
* Automatically scales image aspect ratios to match character cell dimensions.
* Inline images scroll seamlessly with terminal history without corrupting text layouts.

### CLI Usage:
```bash
# Display an image directly in the terminal grid
limg image.png

# Restrict maximum rendering width (in character columns)
limg screenshot.jpg --cols 60
```

---

## 3. 📦 `lpm` — Lunula Package & Theme Manager

`lpm` is a lightweight package manager for discovering, downloading, and installing community themes and extensions into `%APPDATA%\lunula\themes\`:

### CLI Usage:
```bash
# List installed local themes
lpm list

# Search for available community themes
lpm search catppuccin

# Install a theme directly into %APPDATA%\lunula\themes\
lpm install tokyo_night
```
