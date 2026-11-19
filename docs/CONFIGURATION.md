# ⚙️ Lunula Terminal Configuration Guide (`lunula.lua`)

Lunula Terminal is built around the **Config-as-Code** philosophy using fast, modular, and strongly-typed Lua 5.4. All settings can be changed and applied instantly without closing the terminal (*hot-reload* via `Ctrl + Shift + R` or the Gear button `⚙️`).

---

## 📂 Modular Configuration Structure

To fulfill the *“highly customizable”* principle, Lunula Terminal uses a clean modular structure:

```text
lunula/  (or current executable working directory / %APPDATA%\lunula\)
├── lunula.lua           # Master configuration (Window, Physics, Shell, Font)
├── colors.lua           # UI color customization & 16 ANSI terminal palette
├── keybinds.lua         # Keyboard & mouse shortcut customization
└── themes/              # User custom themes directory
    └── cyberpunk.lua    # Example user-created theme
```

> [!TIP]
> Wherever you open the terminal (`C:\`, `D:\Project`, or from a USB drive), clicking the **Gear button (⚙️)** on the title bar or pressing **`Ctrl + ,`** opens the **Settings Modal**, allowing you to pick which configuration file to edit directly using the built-in micro-editor `luna`.

---

## 📑 Option Reference in `lunula.lua`

Here is the complete reference for every parameter section in `lunula.lua`:

### 1. `theme` (Global Visual Theme)
Specifies the global color theme for Lunula Terminal and the built-in micro-editor `luna`.

```lua
theme = "kanagawa",
```

**Built-in Theme Presets:**
- `"kanagawa"` *(Default)*: Authentic Kanagawa Wave palette (`#1F1F28`, `#7FB4CA`, `#957FB8`, `#98BB6C`).
- `"tokyo_night"`: Tokyo Night Storm nocturnal aesthetic (`#24283B`, `#C0CAF5`, `#BB9AF7`, `#9ECE6A`).
- `"catppuccin"`: Soft pastel Catppuccin Mocha (`#1E1E2E`, `#CDD6F4`, `#CBA6F7`, `#89B4FA`).
- `"gruvbox"`: Warm retro Gruvbox Dark (`#282828`, `#EBDBB2`, `#FB4934`, `#83A598`).
- `"nord"`: Cool arctic Nord Frost (`#2E3440`, `#ECEFF4`, `#81A1C1`, `#88C0D0`).
- **Custom Theme**: Entering a theme name located in the `themes/` directory (e.g., `"cyberpunk"`), causes Lunula to automatically load `themes/cyberpunk.lua`.

---

### 2. `window` (Window Geometry & Visuals)
Configures initial dimensions, window transparency, and title bar wave animation.

```lua
window = {
    width = "auto",            -- "auto" or numeric pixel value (e.g., 1100)
    height = "auto",           -- "auto" or numeric pixel value (e.g., 700)
    width_percent = 0.62,      -- 62% of monitor width when in "auto" mode
    height_percent = 0.68,     -- 68% of monitor height when in "auto" mode
    lock_size = false,         -- If true, size will not auto-adjust to monitor resolution
    opacity = 0.95,            -- Window background opacity (0.1 = translucent, 1.0 = solid)
    title = "Lunula Terminal", -- Default window title
    animate_titlebar = true,   -- 60 FPS procedural wave animation in title bar
}
```

---

### 3. `wobble` (Interactive 2D Jelly Window Physics)
Calibrates the mass-spring-damper physical simulation responding to window movement and dragging.

```lua
wobble = {
    stiffness = 380.0,    -- Spring stiffness (higher = faster oscillation)
    damping = 18.0,       -- Friction / damping (higher = settles faster without overshooting)
    max_deform = 22.0,    -- Maximum allowed vertex deformation in pixels
    spring_shear = 0.12,  -- Corner shear tension coefficient
}
```

* **Playful / Super-Jelly Profile**: `stiffness = 220.0`, `damping = 10.0`, `max_deform = 35.0`
* **Stiff / Corporate Minimal Profile**: `stiffness = 600.0`, `damping = 30.0`, `max_deform = 8.0`

---

### 4. `terminal` (Text Engine & Font Rendering)
Configures font typography, sizing, and scrollback memory allocation.

```lua
terminal = {
    font_family = "Cascadia Code, Consolas, monospace", -- Font family fallback chain
    font_size = 14.0,                                    -- Font size in points
    max_scrollback = 10000,                              -- Scrollback buffer line capacity
    allow_clipboard_read = false,                        -- Allow PTY/remote apps to read clipboard via OSC 52 (default: false for security)
}
```

> [!NOTE]
> If a font family is not installed on the system, Lunula gracefully falls back through the comma-separated font chain down to standard Windows monospace fonts.

---

### 5. `cursor` (Cursor Appearance & Animations)
Controls terminal cursor shape, blinking animation, and custom colors.

```lua
cursor = {
    shape = "block",          -- "block", "beam", or "underline"
    blinking = true,          -- Enable periodic blink pacing
    color = { 255, 158, 59 }, -- RGB color (Kanagawa Surimi Orange)
}
```

---

### 6. `scroll` (Sub-Pixel Smooth Scrolling)
Configures inertial scrolling and wheel speed factors.

```lua
scroll = {
    multiplier = 3.0,  -- Lines scrolled per mouse wheel tick
    smooth = true,     -- Sub-pixel interpolation for butter-smooth scrolling
}
```

---

### 7. `tabs` (Multi-Tab Management)
Configures tab bar behaviors, title truncate width, and close-tab rules.

```lua
tabs = {
    max_tabs = 16,     -- Maximum number of open tabs allowed
    show_index = true, -- Displays numeric index badge [1], [2], ... for quick switching
}
```

---

### 8. `shell` (Default Shell & Arguments)
Specifies which shell to spawn when launching panes and new tabs.

```lua
shell = {
    program = "powershell.exe", -- Executable path or name ("powershell.exe", "pwsh.exe", "cmd.exe")
    args = { "-NoLogo" },       -- Command-line arguments passed to shell process
}
```

---

## 🎨 Customizing 16 ANSI Colors (`colors.lua`)

To customize terminal palette colors without touching master configuration, edit `colors.lua`:

```lua
return {
    -- Terminal selection highlight color [R, G, B]
    selection = { 45, 79, 103 },

    -- Optional background/foreground overrides
    background = nil,  -- Use default theme background or set { 30, 30, 46 }
    foreground = nil,  -- Use default theme foreground or set { 205, 214, 244 }

    -- 16 ANSI Colors (0-15: Normal and Bright colors)
    ansi = {
        [0]  = { 22, 22, 29 },    -- Black (sumiInk0)
        [1]  = { 195, 64, 67 },   -- Red (autumnRed)
        [2]  = { 152, 187, 108 }, -- Green (springGreen)
        [3]  = { 230, 195, 132 }, -- Yellow (boatYellow2)
        [4]  = { 126, 156, 216 }, -- Blue (crystalBlue)
        [5]  = { 149, 127, 184 }, -- Magenta (oniViolet)
        [6]  = { 122, 168, 159 }, -- Cyan (waveAqua1)
        [7]  = { 220, 215, 186 }, -- White (fujiWhite)
        [8]  = { 114, 113, 105 }, -- Bright Black (fujiGray)
        [9]  = { 232, 131, 108 }, -- Bright Red (surimiOrange)
        [10] = { 152, 187, 108 }, -- Bright Green (springGreen)
        [11] = { 230, 195, 132 }, -- Bright Yellow (boatYellow2)
        [12] = { 127, 180, 202 }, -- Bright Blue (springBlue)
        [13] = { 147, 138, 169 }, -- Bright Magenta (springViolet1)
        [14] = { 122, 168, 159 }, -- Bright Cyan (waveAqua2)
        [15] = { 242, 241, 235 }, -- Bright White (oldWhite)
    }
}
```

---

## ⌨️ Customizing Keyboard Shortcuts (`keybinds.lua`)

Map any key combination directly to actions in `keybinds.lua`:

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

Whenever you save `keybinds.lua`, Lunula Terminal automatically reloads the bindings and updates the Help Modal overlay immediately!
