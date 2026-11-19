//! `lpm` - Lunula Package Manager (Yay / Pacman style interactive CLI wrapper for Windows winget).
//! Ultra-lightweight & zero heavy dependencies: fast startup, ANSI colors, interactive selection.

use std::env;
use std::io::{self, BufRead, Write};
use std::process::{Command, Stdio};

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetStdHandle(nStdHandle: u32) -> *mut std::ffi::c_void;
    fn GetConsoleMode(hConsoleHandle: *mut std::ffi::c_void, lpMode: *mut u32) -> i32;
    fn SetConsoleMode(hConsoleHandle: *mut std::ffi::c_void, dwMode: u32) -> i32;
}

#[cfg(windows)]
fn enable_ansi_support() {
    unsafe {
        const STD_OUTPUT_HANDLE: u32 = (-11i32) as u32;
        const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut mode: u32 = 0;
        if GetConsoleMode(handle, &mut mode) != 0 {
            let _ = SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
        }
    }
}

#[cfg(not(windows))]
fn enable_ansi_support() {}

// ANSI Color constants
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const CYAN: &str = "\x1b[36m";
const BOLD_CYAN: &str = "\x1b[1;36m";
#[allow(dead_code)]
const GREEN: &str = "\x1b[32m";
const BOLD_GREEN: &str = "\x1b[1;32m";
const YELLOW: &str = "\x1b[33m";
const BOLD_YELLOW: &str = "\x1b[1;33m";
#[allow(dead_code)]
const RED: &str = "\x1b[31m";
const BOLD_RED: &str = "\x1b[1;31m";
const MAGENTA: &str = "\x1b[35m";
const BOLD_MAGENTA: &str = "\x1b[1;35m";
const GRAY: &str = "\x1b[90m";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageEntry {
    pub name: String,
    pub id: String,
    pub version: String,
    pub match_tag: String,
    pub source: String,
}

fn print_logo() {
    println!(
        "{BOLD_MAGENTA}🌙 lpm{RESET} {DIM}-{RESET} {BOLD_CYAN}Lunula Package Manager{RESET} {GRAY}(winget wrapper){RESET}"
    );
}

fn print_help() {
    print_logo();
    println!();
    println!("{BOLD}PENGGUNAAN:{RESET}");
    println!("  {CYAN}lpm{RESET} {YELLOW}<nama_paket>{RESET}              Pencarian interaktif & install (ala {BOLD}yay{RESET})");
    println!("  {CYAN}lpm -S{RESET} {YELLOW}<paket...>{RESET}               Install paket langsung (ala {BOLD}pacman -S{RESET})");
    println!("  {CYAN}lpm -Ss{RESET} {YELLOW}<kata_kunci>{RESET}           Cari paket di katalog (ala {BOLD}pacman -Ss{RESET})");
    println!("  {CYAN}lpm -Syu{RESET}                       Perbarui semua paket terpasang (ala {BOLD}pacman -Syu{RESET})");
    println!("  {CYAN}lpm -R{RESET} {YELLOW}<paket...>{RESET}               Copot / hapus paket (ala {BOLD}pacman -R{RESET})");
    println!("  {CYAN}lpm -Q{RESET} {GRAY}[filter]{RESET}                Daftar paket yang terpasang (ala {BOLD}pacman -Q{RESET})");
    println!("  {CYAN}lpm -Si{RESET} {YELLOW}<paket>{RESET}                Lihat rincian paket (ala {BOLD}pacman -Si{RESET})");
    println!();
    println!("{BOLD}ALIAS TAMBAHAN:{RESET}");
    println!("  {CYAN}lpm install{RESET} {YELLOW}<paket...>{RESET}        Sama dengan -S");
    println!("  {CYAN}lpm search{RESET} {YELLOW}<kata_kunci>{RESET}       Sama dengan -Ss");
    println!("  {CYAN}lpm update{RESET} / {CYAN}upgrade{RESET}          Sama dengan -Syu");
    println!("  {CYAN}lpm remove{RESET} / {CYAN}uninstall{RESET}        Sama dengan -R");
    println!("  {CYAN}lpm list{RESET} {GRAY}[filter]{RESET}             Sama dengan -Q");
    println!("  {CYAN}lpm info{RESET} {YELLOW}<paket>{RESET}               Sama dengan -Si");
    println!();
    println!("{BOLD}CONTOH:{RESET}");
    println!("  {GRAY}# Cari dan pilih secara interaktif:{RESET}");
    println!("  lpm neovim");
    println!("  lpm discord");
    println!();
    println!("  {GRAY}# Upgrade seluruh sistem Windows:{RESET}");
    println!("  lpm -Syu");
}

/// Parse output tabel dari `winget search`
pub fn parse_winget_table(raw_output: &str) -> Vec<PackageEntry> {
    let mut entries = Vec::new();
    // Normalisasi carriage return (\r) dari spinner progress bar winget
    let normalized = raw_output.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.lines().collect();

    // Cari baris separator (garis putus-putus "---")
    // Header selalu berada tepat 1 baris di atas separator
    let mut sep_idx = None;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("---") && trimmed.chars().all(|c| c == '-' || c == ' ') {
            sep_idx = Some(i);
            break;
        }
    }

    let sep_idx = match sep_idx {
        Some(idx) if idx > 0 => idx,
        _ => return entries,
    };

    let header_line = lines[sep_idx - 1];
    let id_col = header_line.find("Id").unwrap_or(30);
    let ver_col = header_line.find("Version").unwrap_or(55);
    let match_col = header_line.find("Match");
    let source_col = header_line.find("Source").unwrap_or(header_line.len().saturating_sub(10));

    // Data dimulai tepat setelah baris separator
    for line in lines.iter().skip(sep_idx + 1) {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('<')
            || trimmed.starts_with('-')
            || trimmed.contains("result limit")
        {
            continue;
        }

        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();

        let extract = |start: usize, end: usize| -> String {
            if start >= len {
                return String::new();
            }
            let end_bounded = end.min(len);
            chars[start..end_bounded].iter().collect::<String>().trim().to_string()
        };

        let name = extract(0, id_col);
        let id = extract(id_col, ver_col);

        let (version, match_tag, source) = if let Some(m_col) = match_col {
            let ver = extract(ver_col, m_col);
            let m = extract(m_col, source_col);
            let src = if source_col < len {
                chars[source_col..].iter().collect::<String>().trim().to_string()
            } else {
                String::new()
            };
            (ver, m, src)
        } else {
            let ver = extract(ver_col, source_col);
            let src = if source_col < len {
                chars[source_col..].iter().collect::<String>().trim().to_string()
            } else {
                String::new()
            };
            (ver, String::new(), src)
        };

        if !name.is_empty() && !id.is_empty() {
            entries.push(PackageEntry {
                name,
                id,
                version,
                match_tag,
                source,
            });
        }
    }

    entries
}

/// Menjalankan pencarian `winget search`
fn search_packages(query: &str) -> io::Result<Vec<PackageEntry>> {
    println!("{BOLD_CYAN}::{RESET} Mencari paket untuk '{BOLD}{query}{RESET}' di katalog winget...");
    
    let output = Command::new("winget")
        .args([
            "search",
            query,
            "--accept-source-agreements",
        ])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries = parse_winget_table(&stdout);

    // Fallback jika formatting tabel berbeda di sistem tertentu
    if entries.is_empty() {
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed.starts_with('-')
                || (trimmed.contains("Name") && trimmed.contains("Id"))
                || trimmed.contains("result limit")
                || trimmed.contains("No package found")
            {
                continue;
            }
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 3 {
                if let Some(id_pos) = parts.iter().position(|p| p.contains('.') || (p.len() > 6 && p.chars().any(|c| c.is_ascii_uppercase()))) {
                    if id_pos > 0 && id_pos + 1 < parts.len() {
                        let name = parts[0..id_pos].join(" ");
                        let id = parts[id_pos].to_string();
                        let version = parts[id_pos + 1].to_string();
                        let source = parts.last().unwrap_or(&"winget").to_string();
                        entries.push(PackageEntry {
                            name,
                            id,
                            version,
                            match_tag: String::new(),
                            source,
                        });
                    }
                }
            }
        }
    }

    // Prioritaskan hasil pencarian secara cerdas:
    // 1. Exact match ID atau Name
    // 2. Name diawali dengan query
    // 3. Source winget diprioritaskan dibanding msstore
    // 4. Entri non-tag diprioritaskan dibanding yang hanya cocok via Tag
    let query_lower = query.to_lowercase();
    entries.sort_by(|a, b| {
        let score = |pkg: &PackageEntry| -> i32 {
            let id_l = pkg.id.to_lowercase();
            let name_l = pkg.name.to_lowercase();
            let mut s = 0;

            if id_l == query_lower || name_l == query_lower {
                s += 1000;
            } else if id_l.starts_with(&query_lower) || name_l.starts_with(&query_lower) {
                s += 500;
            } else if id_l.contains(&query_lower) || name_l.contains(&query_lower) {
                s += 200;
            }

            if pkg.source.eq_ignore_ascii_case("winget") {
                s += 50;
            }

            if pkg.match_tag.is_empty() {
                s += 20;
            }

            s
        };

        let score_a = score(a);
        let score_b = score(b);
        score_b.cmp(&score_a)
    });

    Ok(entries)
}

/// Menginstall satu atau beberapa paket berdasarkan ID
fn install_package_by_id(package_id: &str) -> io::Result<bool> {
    println!("{BOLD_GREEN}==>{RESET} Menginstall {BOLD_CYAN}{package_id}{RESET}...");
    let status = Command::new("winget")
        .args([
            "install",
            "--id",
            package_id,
            "-e",
            "--accept-source-agreements",
            "--accept-package-agreements",
        ])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    Ok(status.success())
}

/// Alur interaktif ala `yay`
fn interactive_search_and_install(query: &str) -> io::Result<()> {
    let entries = search_packages(query)?;

    if entries.is_empty() {
        println!("{BOLD_RED}::{RESET} Tidak ada paket yang ditemukan untuk '{BOLD}{query}{RESET}'.");
        return Ok(());
    }

    println!();
    println!(
        "{BOLD_CYAN}::{RESET} Ditemukan {BOLD}{}{RESET} paket:",
        entries.len()
    );

    let display_limit = entries.len().min(30);
    for (i, pkg) in entries.iter().take(display_limit).enumerate() {
        let idx_str = format!("{:>2})", i + 1);
        let match_info = if !pkg.match_tag.is_empty() {
            format!(" {GRAY}[{}]{RESET}", pkg.match_tag)
        } else {
            String::new()
        };
        let src_info = if !pkg.source.is_empty() {
            format!("{DIM}::{RESET} {MAGENTA}{}{RESET}", pkg.source)
        } else {
            String::new()
        };

        println!(
            " {BOLD_GREEN}{idx_str}{RESET} {BOLD}{}{RESET} {CYAN}({}){RESET} {YELLOW}[{}]{RESET}{match_info} {src_info}",
            pkg.name, pkg.id, pkg.version
        );
    }

    if entries.len() > display_limit {
        println!(
            "  {GRAY}... dan {} paket lainnya disembunyikan.{RESET}",
            entries.len() - display_limit
        );
    }

    println!();
    print!("{BOLD_YELLOW}==>{RESET} Masukkan nomor paket yang ingin diinstall (cth: {BOLD}1{RESET}, {BOLD}1 2{RESET}, atau {BOLD}q{RESET} untuk batal) [{BOLD}1{RESET}]: ");
    io::stdout().flush()?;

    let mut input = String::new();
    let stdin = io::stdin();
    stdin.lock().read_line(&mut input)?;

    let input = input.trim();
    if input.eq_ignore_ascii_case("q") || input.eq_ignore_ascii_case("batal") || input.eq_ignore_ascii_case("exit") {
        println!("{GRAY}Dibatalkan.{RESET}");
        return Ok(());
    }

    // Default ke nomor 1 jika user langsung tekan enter
    let selected_indices: Vec<usize> = if input.is_empty() {
        vec![1]
    } else {
        input
            .split(&[' ', ','][..])
            .filter_map(|s| s.trim().parse::<usize>().ok())
            .collect()
    };

    if selected_indices.is_empty() {
        println!("{BOLD_RED}::{RESET} Pilihan tidak valid.");
        return Ok(());
    }

    for idx in selected_indices {
        if idx == 0 || idx > entries.len() {
            println!("{BOLD_RED}::{RESET} Nomor {idx} di luar jangkauan pilihan.");
            continue;
        }

        let pkg = &entries[idx - 1];
        let success = install_package_by_id(&pkg.id)?;
        if success {
            println!("{BOLD_GREEN}::{RESET} {BOLD}{}{RESET} berhasil terinstall!", pkg.name);
        } else {
            println!("{BOLD_RED}::{RESET} Gagal menginstall {BOLD}{}{RESET}.", pkg.name);
        }
    }

    Ok(())
}

/// Menghapus/uninstall paket
fn remove_package(package_query: &str) -> io::Result<()> {
    println!("{BOLD_RED}==>{RESET} Mencopot {BOLD_CYAN}{package_query}{RESET}...");
    let status = Command::new("winget")
        .args(["uninstall", package_query])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if status.success() {
        println!("{BOLD_GREEN}::{RESET} Berhasil dicopot!");
    } else {
        println!("{BOLD_RED}::{RESET} Gagal mencopot paket.");
    }
    Ok(())
}

/// Upgrade semua paket (ala pacman -Syu)
fn upgrade_all() -> io::Result<()> {
    println!("{BOLD_CYAN}::{RESET} Memeriksa & memperbarui seluruh paket terpasang...");
    let status = Command::new("winget")
        .args([
            "upgrade",
            "--all",
            "--include-unknown",
            "--accept-source-agreements",
            "--accept-package-agreements",
        ])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if status.success() {
        println!("{BOLD_GREEN}::{RESET} Pembaruan sistem selesai!");
    }
    Ok(())
}

/// Menampilkan daftar paket terpasang (ala pacman -Q)
fn list_installed(query: Option<&str>) -> io::Result<()> {
    let mut cmd = Command::new("winget");
    cmd.arg("list");
    if let Some(q) = query {
        cmd.arg(q);
    }
    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let _ = cmd.status()?;
    Ok(())
}

/// Menampilkan rincian paket (ala pacman -Si)
fn show_info(query: &str) -> io::Result<()> {
    let mut cmd = Command::new("winget");
    cmd.args(["show", query, "--accept-source-agreements"]);
    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let _ = cmd.status()?;
    Ok(())
}

fn main() -> io::Result<()> {
    enable_ansi_support();

    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        print_help();
        return Ok(());
    }

    let first = &args[0];

    match first.as_str() {
        "-h" | "--help" | "help" => {
            print_help();
        }
        "-V" | "--version" | "version" => {
            println!("{BOLD_MAGENTA}lpm{RESET} versi 0.1.0 {DIM}(Lunula Package Manager){RESET}");
        }

        // Pacman -Syu / upgrade
        "-Syu" | "-Syyu" | "-u" | "update" | "upgrade" => {
            upgrade_all()?;
        }

        // Pacman -Ss / search only
        "-Ss" | "search" => {
            if args.len() < 2 {
                println!("{BOLD_RED}::{RESET} Masukkan kata kunci pencarian. Contoh: {CYAN}lpm -Ss neovim{RESET}");
                return Ok(());
            }
            let query = &args[1..].join(" ");
            let entries = search_packages(query)?;
            if entries.is_empty() {
                println!("{BOLD_RED}::{RESET} Tidak ditemukan paket untuk '{query}'.");
            } else {
                println!("{BOLD_CYAN}::{RESET} Ditemukan {BOLD}{}{RESET} paket:", entries.len());
                for (i, pkg) in entries.iter().enumerate() {
                    let idx_str = format!("{:>2})", i + 1);
                    let match_info = if !pkg.match_tag.is_empty() {
                        format!(" {GRAY}[{}]{RESET}", pkg.match_tag)
                    } else {
                        String::new()
                    };
                    println!(
                        " {BOLD_GREEN}{idx_str}{RESET} {BOLD}{}{RESET} {CYAN}({}){RESET} {YELLOW}[{}]{RESET}{match_info} {DIM}::{RESET} {MAGENTA}{}{RESET}",
                        pkg.name, pkg.id, pkg.version, pkg.source
                    );
                }
            }
        }

        // Pacman -S / install
        "-S" | "install" => {
            if args.len() < 2 {
                println!("{BOLD_RED}::{RESET} Masukkan nama paket yang ingin diinstall. Contoh: {CYAN}lpm -S git{RESET}");
                return Ok(());
            }
            for pkg in &args[1..] {
                interactive_search_and_install(pkg)?;
            }
        }

        // Pacman -R / remove
        "-R" | "-Rs" | "remove" | "uninstall" => {
            if args.len() < 2 {
                println!("{BOLD_RED}::{RESET} Masukkan nama paket yang ingin dicopot. Contoh: {CYAN}lpm -R git{RESET}");
                return Ok(());
            }
            for pkg in &args[1..] {
                remove_package(pkg)?;
            }
        }

        // Pacman -Q / query installed
        "-Q" | "-Qs" | "list" => {
            let filter = args.get(1).map(|s| s.as_str());
            list_installed(filter)?;
        }

        // Pacman -Si / info
        "-Si" | "info" | "show" => {
            if args.len() < 2 {
                println!("{BOLD_RED}::{RESET} Masukkan nama paket untuk melihat info. Contoh: {CYAN}lpm -Si neovim{RESET}");
                return Ok(());
            }
            show_info(&args[1])?;
        }

        // Default mode ala `yay`: `lpm <query>`
        other => {
            if other.starts_with('-') {
                println!("{BOLD_RED}::{RESET} Flag tidak dikenal: {BOLD}{other}{RESET}");
                println!("Gunakan {CYAN}lpm --help{RESET} untuk melihat panduan.");
            } else {
                let query = args.join(" ");
                interactive_search_and_install(&query)?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_table() {
        let sample = "\n   - \n                                                                                                                        \nName           Id                    Version    Match              Source\n-------------------------------------------------------------------------\nNeovim         Neovim.Neovim         0.12.6                        winget\nmarksman       Artempyanykh.Marksman 2026-02-08 Tag: neovim        winget\nneovim-qt      equalsraf.neovim-qt   0.2.19     Tag: neovim        winget\nHelix          Helix.Helix           25.07.1    Tag: neovim        winget\nNeovide        Neovide.Neovide       0.17.2     Tag: neovim        winget\nNeovim Nightly Neovim.Neovim.Nightly 0.13.0                        winget\nPHPantom       PHPantom.LSP          0.11.1     Tag: neovim-plugin winget\n";
        let entries = parse_winget_table(sample);
        assert_eq!(entries.len(), 7, "Harus mem-parse 7 paket, tapi dapat: {:?}", entries);
        assert_eq!(entries[0].name, "Neovim");
        assert_eq!(entries[0].id, "Neovim.Neovim");
        assert_eq!(entries[0].version, "0.12.6");
        assert_eq!(entries[5].name, "Neovim Nightly");
        assert_eq!(entries[5].id, "Neovim.Neovim.Nightly");
    }
}
