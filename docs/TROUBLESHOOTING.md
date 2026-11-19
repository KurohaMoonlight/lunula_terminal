# 🔍 Troubleshooting & Optimization Guide

This document provides solutions to common issues and optimization techniques when using **Lunula Terminal**.

---

## 1. Missing Nerd Font Glyphs or Box Characters (Tofu)

### Cause:
The font specified in `lunula.lua` is either not installed on your Windows machine or is not a patched *Nerd Font* (which contains special glyphs for Powerline symbols, Git branches, folder icons, etc.).

### Solution:
1. Download a patched monospace Nerd Font (such as **JetBrainsMono Nerd Font**, **FiraCode Nerd Font**, or **CaskaydiaCove Nerd Font**) from [Nerd Fonts](https://www.nerdfonts.com/).
2. Install the font on Windows (right-click the `.ttf` / `.otf` file $\rightarrow$ *Install for all users*).
3. Open `lunula.lua` (via the Gear button `⚙️` or `Ctrl + ,`), and update the font family:
   ```lua
   terminal = {
       font_family = "JetBrainsMono Nerd Font, Cascadia Code, monospace",
       font_size = 15.0,
   }
   ```
4. Save and press `Ctrl + Shift + R` to hot-reload the configuration.

---

## 2. Maximizing Laptop Battery Life

By default, Lunula Terminal renders a 60 FPS Kanagawa wave animation in the title bar and simulates interactive 2D jelly window physics. On desktop workstations or plugged-in laptops, GPU power draw is negligible. When traveling on battery, you can activate the ultra-low-power profile:

### Battery Saver Settings in `lunula.lua`:
```lua
window = {
    animate_titlebar = false, -- Disables continuous title bar wave animation
}
wobble = {
    stiffness = 500.0,
    damping = 25.0,
}
```
When `animate_titlebar = false`, Lunula Terminal enters deep event-driven sleep (*WaitUntil idle mode*), utilizing **0% CPU and 0% GPU** whenever there is no active keyboard input or shell output.

---

## 3. Special Characters, IME & Emojis

Lunula Terminal fully supports Unicode text input, complex IMEs (Japanese, Chinese, Korean, etc.), and native Windows emoji picker (`Win + .`):
* The IME candidate window automatically positions itself directly beneath the active terminal cursor.
* Cursor tracking updates IME candidate bounding boxes in real time across splits and tabs.

---

## 4. Resetting Configuration to Defaults

If configuration files become corrupted or unparseable:
1. Delete or rename `%APPDATA%\lunula`.
2. Launch Lunula Terminal again.
3. Lunula automatically generates pristine default versions of `lunula.lua`, `colors.lua`, and `keybinds.lua`.
