# ⌨️ Lunula Terminal Keyboard Shortcuts & Navigation Guide

This document provides a comprehensive reference for all keyboard shortcuts and mouse interactions in **Lunula Terminal**, including customization via `keybinds.lua`.

---

## 📑 Core Keyboard Shortcuts Table

### 1. Split Pane Management (Tiling Panes)
| Shortcut | Action | Description |
| :--- | :--- | :--- |
| `Ctrl + Shift + E` | **Split Vertically** | Splits active pane into left and right (*side-by-side*) |
| `Ctrl + Shift + D` | **Split Horizontally** | Splits active pane into top and bottom (*stacked*) |
| `Ctrl + Shift + W` | **Close Pane** | Closes active pane (or closes tab if last remaining pane) |
| `Alt + Arrows (← ↑ → ↓)` | **Navigate Focus** | Moves keyboard focus between split panes |
| Shell command `exit` | **Close Pane** | Typing `exit` in the shell cleanly disposes of the pane |
| `[✕]` Button | **Close via Mouse** | Click the close badge in the top-right corner of each pane |

> [!NOTE]
> Standard `Ctrl + W` is forwarded directly to the shell as control code `^W` (deleting the previous word in PowerShell / Bash / Zsh). Use **`Ctrl + Shift + W`** to close a pane.

---

### 2. Tab Management (Multi-Tab)
| Shortcut | Action | Description |
| :--- | :--- | :--- |
| `Ctrl + Shift + T` | **New Tab** | Opens a new terminal tab |
| `Ctrl + Tab` | **Next Tab** | Cycles forward to the next tab |
| `Ctrl + Shift + Tab` | **Previous Tab** | Cycles backward to the previous tab |
| `Ctrl + 1` .. `Ctrl + 9` | **Direct Jump** | Jumps directly to Tab 1 through Tab 9 |
| `[+]` Button on Tab Bar | **New Tab via Mouse** | Click the plus icon next to the tab bar |

---

### 3. Settings, Help & Window Actions
| Shortcut | Action | Description |
| :--- | :--- | :--- |
| `Ctrl + ,` | **Settings Hub** | Opens the modular configuration overlay |
| Gear Button `⚙️` | **Settings Hub** | Click the gear icon on the title bar |
| `Shift + F1` | **Help Reference** | Opens the shortcuts and feature reference modal |
| `Ctrl + Shift + H` | **Help Reference** | Alternative shortcut to open Help reference |
| `Ctrl + Shift + R` | **Hot-Reload Config** | Re-applies configuration without restarting |
| `Ctrl + Shift + P` | **Reload Profile** | Re-sources user's PowerShell profile (`. $PROFILE`) |
| `F11` | **Fullscreen / Maximize** | Toggles borderless fullscreen mode |

> [!TIP]
> Inside the **Settings Modal (`Ctrl + ,`)**:
> - Press **`1`** through **`6`** to execute actions immediately.
> - Or navigate with **`↑` / `↓`** (or **`k` / `j`**) and press **`Enter`**.
> - Press **`Esc`** or **`q`** to close the overlay.

---

### 4. Clipboard & Selection Interactions
| Shortcut / Gesture | Action | Description |
| :--- | :--- | :--- |
| `Left-Click + Drag` | **Select Text** | Selects text bounded strictly to screen cell contents |
| `Double Click` | **Select Word** | Highlights the word under cursor |
| `Triple Click` | **Select Line** | Highlights the entire row of text |
| `Ctrl + C` / `Ctrl + Shift + C` | **Copy** | Copies selected text (or sends `SIGINT` if no selection) |
| `Ctrl + V` / `Right-Click` | **Paste** | Pastes clipboard text directly into PTY |
| `Ctrl + Left-Click` | **Open Hyperlink** | Opens detected URL (`http://`, `https://`, or OSC 8) in default browser |

---

### 5. Vi Navigation Mode, Search & Regex Hints
| Shortcut | Action | Description |
| :--- | :--- | :--- |
| `Ctrl + Shift + Space` | **Toggle Vi Mode** | Enters keyboard-driven scrollback buffer navigation |
| `h` / `j` / `k` / `l` | **Vi Movement** | Moves cursor left, down, up, and right through history |
| `v` | **Visual Selection** | Toggles character-wise text selection |
| `V` | **Visual Line** | Toggles full-line text selection |
| `y` | **Yank** | Copies active visual selection to Windows clipboard |
| `/` or `Ctrl + Shift + F` | **Search Scrollback** | Opens search query bar to find text across buffer |
| `n` / `N` | **Next / Prev Match** | Jumps to next / previous search match |
| `Ctrl + Shift + U` | **Regex Hints** | Scans visible rows for URLs, file paths, and hashes with quick labels |

---

## 🛠️ Dynamic Customization via `keybinds.lua`

All shortcuts can be remapped inside `keybinds.lua`:

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

Whenever this file is saved, Lunula Terminal automatically reloads the bindings and updates the Help Modal overlay immediately!
