# ==========================================================
#  lunula-theme.ps1 - ganti tema oh-my-posh Kanagawa Wave
#
#  Pasang (sekali saja):   .\lunula-theme.ps1 -Install
#  Pakai:                  theme-twopill     theme-sharp     theme-dev ...
#                          theme             (daftar tema, * = aktif)
#                          theme lean        (tab-completion juga jalan)
#
#  Tema dibaca otomatis dari folder .\themes\lunula-kanagawa*.omp.json,
#  jadi tema baru yang kamu taruh di sana langsung punya perintah theme-<nama>.
#  Pilihan terakhir disimpan di ~\.lunula-theme dan dipakai saat terminal dibuka.
# ==========================================================
param(
    [switch]$Install
)

$script:LunulaThemeDir  = Join-Path $PSScriptRoot 'themes'
$script:LunulaThemeFile = Join-Path $HOME '.lunula-theme'
$script:LunulaPrefix    = 'lunula-kanagawa'
$script:LunulaDefault   = 'twopill'

# ---------- -Install: tambahkan baris dot-source ke $PROFILE ----------
if ($Install) {
    if (-not (Test-Path -LiteralPath $PROFILE)) {
        New-Item -ItemType File -Path $PROFILE -Force | Out-Null
    }
    $profileText = Get-Content -LiteralPath $PROFILE -Raw -ErrorAction SilentlyContinue
    if ($profileText -and $profileText.Contains($PSCommandPath)) {
        Write-Host "Sudah terpasang di $PROFILE"
    }
    else {
        Add-Content -LiteralPath $PROFILE -Value "`r`n# Lunula - tema oh-my-posh Kanagawa`r`n. `"$PSCommandPath`""
        Write-Host "Terpasang di $PROFILE"
    }
    if ($profileText -and ($profileText -match 'oh-my-posh\s+init')) {
        Write-Warning "Profil kamu masih punya baris 'oh-my-posh init ...' sendiri. Hapus baris itu supaya tidak menimpa tema pilihanmu."
    }
    Write-Host "Buka tab baru atau tekan Ctrl+Shift+P, lalu coba: theme-twopill"
    return
}

# ---------- daftar tema: nama -> path ----------
function global:Get-LunulaThemeMap {
    $map = [ordered]@{}
    $files = Get-ChildItem -LiteralPath $script:LunulaThemeDir -Filter "$($script:LunulaPrefix)*.omp.json" -ErrorAction SilentlyContinue | Sort-Object Name
    foreach ($f in $files) {
        $n = $f.Name -replace '\.omp\.json$', ''
        $n = $n -replace "^$($script:LunulaPrefix)-?", ''
        if (-not $n) { $n = 'default' }
        $map[$n] = $f.FullName
    }
    return $map
}

function global:Show-LunulaThemes {
    $map = Get-LunulaThemeMap
    Write-Host 'Tema tersedia (ketik theme-<nama>):'
    foreach ($k in $map.Keys) {
        $mark = if ($k -eq $env:LUNULA_THEME) { '*' } else { ' ' }
        Write-Host ("  {0} theme-{1}" -f $mark, $k)
    }
}

# ---------- terapkan tema ----------
function global:Set-LunulaTheme {
    param(
        [Parameter(Position = 0)][string]$Name,
        [switch]$NoSave
    )
    $map = Get-LunulaThemeMap
    if (-not $Name) { Show-LunulaThemes; return }
    if (-not $map.Contains($Name)) {
        Write-Warning "Tema '$Name' tidak ada. Pilihan: $($map.Keys -join ', ')"
        return
    }
    if (-not (Get-Command oh-my-posh -ErrorAction SilentlyContinue)) {
        Write-Warning 'oh-my-posh tidak ditemukan di PATH.'
        return
    }
    oh-my-posh init pwsh --config $map[$Name] | Invoke-Expression
    $env:LUNULA_THEME = $Name
    if (-not $NoSave) {
        Set-Content -LiteralPath $script:LunulaThemeFile -Value $Name -Encoding ASCII
    }
}

# `theme` / `theme <nama>`
function global:theme {
    param([Parameter(Position = 0)][string]$Name)
    Set-LunulaTheme $Name
}

# theme-<nama> untuk setiap file tema
foreach ($n in (Get-LunulaThemeMap).Keys) {
    $sb = [scriptblock]::Create("Set-LunulaTheme '$n'")
    Set-Item -Path "function:global:theme-$n" -Value $sb
}

# tab-completion: theme <Tab>
Register-ArgumentCompleter -CommandName 'theme', 'Set-LunulaTheme' -ParameterName 'Name' -ScriptBlock {
    param($commandName, $parameterName, $wordToComplete)
    (Get-LunulaThemeMap).Keys | Where-Object { $_ -like "$wordToComplete*" } | ForEach-Object {
        [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_)
    }
}

# ---------- saat terminal dibuka: pakai pilihan terakhir ----------
$saved = $null
if (Test-Path -LiteralPath $script:LunulaThemeFile) {
    $saved = (Get-Content -LiteralPath $script:LunulaThemeFile -Raw).Trim()
}
$known = Get-LunulaThemeMap
if (-not $saved -or -not $known.Contains($saved)) {
    $saved = if ($known.Contains($script:LunulaDefault)) { $script:LunulaDefault } else { @($known.Keys)[0] }
}
if ($saved) { Set-LunulaTheme $saved -NoSave }
