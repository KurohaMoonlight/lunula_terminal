-- ==========================================================
--  Lunula Terminal - Keyboard Shortcuts Configuration
--  (keybinds.lua)
-- ==========================================================

return {
    -- Format: ["Shortcut"] = "Action"
    -- Actions: "new_tab", "close_pane", "next_tab", "prev_tab",
    --          "split_vertical", "split_horizontal", "search",
    --          "reload_config", "toggle_fullscreen", "toggle_help", "toggle_settings"

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
