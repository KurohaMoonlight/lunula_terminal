-- ==========================================================
--  Lunula Terminal - Custom Colors & 16-Color ANSI Palette
--  (colors.lua)
-- ==========================================================

return {
    -- Override warna UI utama (jika diisi, menimpa theme aktif):
    selection  = "#2D4F67", -- Warna highlight seleksi mouse
    -- background = "#1F1F28",
    -- foreground = "#DCD7BA",
    -- cursor     = "#7FB4CA",
    -- tab_bar    = "#16161D",
    -- border     = "#363646",

    -- 16 ANSI Palette Colors (0..15):
    -- Menentukan warna output perintah CLI di shell (ls, git, htop, powershell syntax, dll.)
    ansi = {
        [0]  = "#16161D", -- Black (sumiInk0)
        [1]  = "#C34043", -- Red (autumnRed)
        [2]  = "#76946A", -- Green (autumnGreen)
        [3]  = "#C0A36E", -- Yellow (boatYellow1)
        [4]  = "#7E9CD8", -- Blue (crystalBlue)
        [5]  = "#957FB8", -- Magenta (oniViolet)
        [6]  = "#6A9589", -- Cyan (waveAqua1)
        [7]  = "#C8C093", -- White (oldWhite)
        [8]  = "#727169", -- Bright Black (fujiGray)
        [9]  = "#E82424", -- Bright Red (samuraiRed)
        [10] = "#98BB6C", -- Bright Green (springGreen)
        [11] = "#E6C384", -- Bright Yellow (boatYellow2)
        [12] = "#7FB4CA", -- Bright Blue (springBlue)
        [13] = "#D27E99", -- Bright Magenta (sakuraPink)
        [14] = "#7AA89F", -- Bright Cyan (waveAqua2)
        [15] = "#DCD7BA", -- Bright White (fujiWhite)
    },
}
