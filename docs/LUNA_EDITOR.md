# 🌙 Luna Micro-Editor (`luna.exe`)

**`luna`** is the built-in terminal text editor (*TUI micro-editor*) shipped with Lunula Terminal. Designed as a modern, ultra-lightweight (<400 KB) alternative to `nano`, `luna` features zero external runtime dependencies, instant startup time (<5 ms), and full ANSI TrueColor lexical syntax highlighting synchronized with your Lunula Terminal theme.

---

## ✨ Key Features of `luna`

1. **Ultra-Lightweight & Single Executable**:
   * Binary footprint under 400 KB in optimized release builds.
   * Utilizes native Windows Console APIs without heavy third-party TUI frameworks.
2. **Fast Lexical Syntax Highlighting**:
   * Automatically recognizes and highlights code tokens:
     * **Lua** (`.lua`)
     * **Rust** (`.rs`)
     * **Shell & PowerShell** (`.ps1`, `.psm1`, `.sh`, `.bash`, `.zsh`, `.cmd`, `.bat`)
     * **Configuration** (`.json`, `.toml`, `.yaml`, `.yml`, `.ini`, `.conf`, `.omp.json`, `Cargo.lock`)
     * **Python** (`.py`)
     * **Markdown** (`.md`)
3. **Automatic Theme Synchronization**:
   * Reads settings directly from `lunula.lua`.
   * Switching your terminal theme to `catppuccin`, `tokyo_night`, `gruvbox`, `nord`, or custom files in `themes/` updates `luna`'s syntax highlights and UI borders immediately.
4. **Modern Editing Controls**:
   * Multi-level undo via `Ctrl + Z`.
   * Fast in-buffer search via `Ctrl + F`.
   * Unsaved changes confirmation prompt on exit (`Ctrl + X`).
   * Clean horizontal scrolling for long lines preserving line number gutters and formatting.

---

## 🚀 Usage

### Opening a File
```bash
luna file.lua
```

### Creating a New File
```bash
luna new_file.rs
```

### Editing Lunula Configuration Directly
Click the **Gear button (⚙️)** on the title bar or press **`Ctrl + ,`**, then select **`[1] Edit Master Config`**. Lunula automatically opens the configuration file with its absolute path in `luna`.

---

## ⌨️ Luna Keybindings Reference

| Shortcut | Action | Description |
| :--- | :--- | :--- |
| `Ctrl + S` | **Save File** | Saves current buffer to disk immediately |
| `Ctrl + X` | **Exit** | Prompts to save if modified, then exits |
| `Ctrl + Z` | **Undo** | Reverts recent typing and deletions |
| `Ctrl + F` | **Search** | Opens search query bar at bottom of screen |
| `Enter` (in search) | **Next Match** | Jumps to next occurrence of search term |
| `Esc` (in search) | **Cancel Search** | Cancels search and returns to normal editing |
| `Arrows (← ↑ → ↓)` | **Move Cursor** | Navigates cursor through text rows and columns |
| `Home` / `End` | **Line Boundary** | Jumps to start or end of the current line |
| `Page Up` / `Page Down` | **Page Scroll** | Scrolls viewable buffer up or down by one screen height |
| `Backspace` / `Delete` | **Delete** | Removes characters before or after cursor |

---

## 🔧 Setting as Default Git Editor

To use `luna` as your default Git commit editor:

```bash
git config --global core.editor "luna"
```
Whenever Git prompts for a commit message or rebase, it will open instantaneously in `luna`!
