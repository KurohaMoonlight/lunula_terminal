//! `lfetch` - Ultra-fast, minimal, animated zero-dependency system info fetcher for Lunula Terminal.
//! Designed around `principle.md`:
//! - Ultra-lightweight & zero external heavy dependencies
//! - Sub-millisecond system metric collection via Win32 / Registry API
//! - Matrix decrypt intro + interactive floating crescent moon over waves
//! - Side-by-side layout with full system metrics & ANSI 16-color palette

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Default, Debug)]
pub struct MemoryInfo {
    pub total_mb: u64,
    pub used_mb: u64,
    pub percent: u32,
}

#[derive(Default, Debug)]
pub struct SystemMetrics {
    pub user_host: String,
    pub os_name: String,
    pub uptime_formatted: String,
    pub shell: String,
    pub terminal: String,
    pub cpu_name: String,
    pub memory: MemoryInfo,
}

#[cfg(windows)]
mod win32 {
    use super::MemoryInfo;
    use std::ffi::c_void;

    const STD_INPUT_HANDLE: u32 = (-10i32) as u32;
    const STD_OUTPUT_HANDLE: u32 = (-11i32) as u32;

    const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
    const ENABLE_PROCESSED_INPUT: u32 = 0x0001;
    const ENABLE_LINE_INPUT: u32 = 0x0002;
    const ENABLE_ECHO_INPUT: u32 = 0x0004;

    pub const HKEY_LOCAL_MACHINE: isize = 0x80000002_u32 as i32 as isize;
    pub const KEY_READ: u32 = 0x20019;
    pub const TH32CS_SNAPPROCESS: u32 = 0x00000002;
    pub const INVALID_HANDLE_VALUE: isize = -1;

    #[repr(C)]
    pub struct MemoryStatusEx {
        pub dw_length: u32,
        pub dw_memory_load: u32,
        pub ull_total_phys: u64,
        pub ull_avail_phys: u64,
        pub ull_total_page_file: u64,
        pub ull_avail_page_file: u64,
        pub ull_total_virtual: u64,
        pub ull_avail_virtual: u64,
        pub ull_avail_extended_virtual: u64,
    }

    #[repr(C)]
    pub struct ProcessEntry32W {
        pub dw_size: u32,
        pub cnt_usage: u32,
        pub th32_process_id: u32,
        pub th32_default_heap_id: usize,
        pub th32_module_id: u32,
        pub cnt_threads: u32,
        pub th32_parent_process_id: u32,
        pub pc_pri_class_base: i32,
        pub dw_flags: u32,
        pub sz_exe_file: [u16; 260],
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn GetStdHandle(n_std_handle: u32) -> *mut c_void;
        pub fn GetConsoleMode(h_console_handle: *mut c_void, lp_mode: *mut u32) -> i32;
        pub fn SetConsoleMode(h_console_handle: *mut c_void, dw_mode: u32) -> i32;
        pub fn WaitForSingleObject(h_handle: *mut c_void, dw_milliseconds: u32) -> u32;

        pub fn GlobalMemoryStatusEx(lp_buffer: *mut MemoryStatusEx) -> i32;
        pub fn GetTickCount64() -> u64;
        pub fn GetCurrentProcessId() -> u32;
        pub fn CreateToolhelp32Snapshot(dw_flags: u32, th32_process_id: u32) -> isize;
        pub fn Process32FirstW(h_snapshot: isize, lppe: *mut ProcessEntry32W) -> i32;
        pub fn Process32NextW(h_snapshot: isize, lppe: *mut ProcessEntry32W) -> i32;
        pub fn CloseHandle(h_object: isize) -> i32;
    }

    #[link(name = "advapi32")]
    unsafe extern "system" {
        pub fn RegOpenKeyExW(
            h_key: isize,
            lp_sub_key: *const u16,
            ul_options: u32,
            sam_desired: u32,
            phk_result: *mut isize,
        ) -> i32;
        pub fn RegQueryValueExW(
            h_key: isize,
            lp_value_name: *const u16,
            lp_reserved: *mut u32,
            lp_type: *mut u32,
            lp_data: *mut u8,
            lpcb_data: *mut u32,
        ) -> i32;
        pub fn RegCloseKey(h_key: isize) -> i32;
    }

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn read_reg_string(h_key: isize, subkey: &str, val_name: &str) -> Option<String> {
        let subkey_w = to_wide(subkey);
        let val_w = to_wide(val_name);

        let mut h_sub = 0;
        let res = unsafe { RegOpenKeyExW(h_key, subkey_w.as_ptr(), 0, KEY_READ, &mut h_sub) };
        if res != 0 {
            return None;
        }

        let mut buf = [0u8; 512];
        let mut buf_len = buf.len() as u32;
        let mut val_type = 0u32;

        let res = unsafe {
            RegQueryValueExW(
                h_sub,
                val_w.as_ptr(),
                std::ptr::null_mut(),
                &mut val_type,
                buf.as_mut_ptr(),
                &mut buf_len,
            )
        };
        unsafe { RegCloseKey(h_sub) };

        if res == 0 && buf_len > 1 {
            let u16_slice = unsafe {
                std::slice::from_raw_parts(buf.as_ptr() as *const u16, (buf_len as usize) / 2)
            };
            let mut s = String::from_utf16_lossy(u16_slice);
            if let Some(null_pos) = s.find('\0') {
                s.truncate(null_pos);
            }
            return Some(s.trim().to_string());
        }
        None
    }

    pub fn get_memory_info() -> MemoryInfo {
        let mut status = MemoryStatusEx {
            dw_length: std::mem::size_of::<MemoryStatusEx>() as u32,
            dw_memory_load: 0,
            ull_total_phys: 0,
            ull_avail_phys: 0,
            ull_total_page_file: 0,
            ull_avail_page_file: 0,
            ull_total_virtual: 0,
            ull_avail_virtual: 0,
            ull_avail_extended_virtual: 0,
        };

        let success = unsafe { GlobalMemoryStatusEx(&mut status) };
        if success != 0 {
            let total_mb = status.ull_total_phys / (1024 * 1024);
            let avail_mb = status.ull_avail_phys / (1024 * 1024);
            let used_mb = total_mb.saturating_sub(avail_mb);
            MemoryInfo {
                total_mb,
                used_mb,
                percent: status.dw_memory_load,
            }
        } else {
            MemoryInfo::default()
        }
    }

    pub fn get_uptime() -> String {
        let ms = unsafe { GetTickCount64() };
        let total_secs = ms / 1000;
        let days = total_secs / 86400;
        let hours = (total_secs % 86400) / 3600;
        let mins = (total_secs % 3600) / 60;

        let mut parts = Vec::new();
        if days > 0 {
            parts.push(format!("{}d", days));
        }
        if hours > 0 {
            parts.push(format!("{}h", hours));
        }
        parts.push(format!("{}m", mins));
        parts.join(" ")
    }

    pub fn get_cpu_name() -> String {
        if let Some(name) = read_reg_string(
            HKEY_LOCAL_MACHINE,
            r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
            "ProcessorNameString",
        ) {
            let cleaned: Vec<&str> = name.split_whitespace().collect();
            return cleaned.join(" ");
        }
        "Unknown CPU".to_string()
    }

    pub fn get_os_info() -> String {
        let subkey = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
        let product = read_reg_string(HKEY_LOCAL_MACHINE, subkey, "ProductName")
            .unwrap_or_else(|| "Windows".to_string());
        let build = read_reg_string(HKEY_LOCAL_MACHINE, subkey, "CurrentBuild")
            .unwrap_or_default();
        let display_ver = read_reg_string(HKEY_LOCAL_MACHINE, subkey, "DisplayVersion");

        let os_family = if let Ok(b) = build.parse::<u32>() {
            if b >= 22000 {
                product.replacen("Windows 10", "Windows 11", 1)
            } else {
                product
            }
        } else {
            product
        };

        match (display_ver, build.is_empty()) {
            (Some(ver), false) => format!("{} ({}, Build {})", os_family, ver, build),
            (Some(ver), true) => format!("{} ({})", os_family, ver),
            (None, false) => format!("{} (Build {})", os_family, build),
            (None, true) => os_family,
        }
    }

    pub fn get_shell_and_terminal() -> (String, String) {
        let term = std::env::var("TERM_PROGRAM").unwrap_or_else(|_| {
            if std::env::var("WT_SESSION").is_ok() {
                "Windows Terminal".to_string()
            } else if std::env::var("ALACRITTY_WINDOW_ID").is_ok() {
                "Alacritty".to_string()
            } else {
                "Lunula Terminal".to_string()
            }
        });

        let my_pid = unsafe { GetCurrentProcessId() };
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        let mut shell = "PowerShell".to_string();

        if snapshot != INVALID_HANDLE_VALUE {
            let mut pe = ProcessEntry32W {
                dw_size: std::mem::size_of::<ProcessEntry32W>() as u32,
                cnt_usage: 0,
                th32_process_id: 0,
                th32_default_heap_id: 0,
                th32_module_id: 0,
                cnt_threads: 0,
                th32_parent_process_id: 0,
                pc_pri_class_base: 0,
                dw_flags: 0,
                sz_exe_file: [0; 260],
            };

            let mut process_map = Vec::new();
            if unsafe { Process32FirstW(snapshot, &mut pe) } != 0 {
                loop {
                    let name_len = pe
                        .sz_exe_file
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(pe.sz_exe_file.len());
                    let exe_name =
                        String::from_utf16_lossy(&pe.sz_exe_file[..name_len]).to_lowercase();
                    process_map.push((pe.th32_process_id, pe.th32_parent_process_id, exe_name));

                    if unsafe { Process32NextW(snapshot, &mut pe) } == 0 {
                        break;
                    }
                }
            }
            unsafe { CloseHandle(snapshot) };

            let mut cur_target_pid = my_pid;
            for _ in 0..5 {
                if let Some((_, parent_pid, _)) =
                    process_map.iter().find(|(pid, _, _)| *pid == cur_target_pid)
                {
                    if let Some((_, _, parent_name)) =
                        process_map.iter().find(|(pid, _, _)| *pid == *parent_pid)
                    {
                        let name = parent_name.trim_end_matches(".exe");
                        match name {
                            "pwsh" => {
                                shell = "PowerShell 7 (pwsh)".to_string();
                                break;
                            }
                            "powershell" => {
                                shell = "Windows PowerShell".to_string();
                                break;
                            }
                            "cmd" => {
                                shell = "Command Prompt (cmd)".to_string();
                                break;
                            }
                            "bash" => {
                                shell = "Bash".to_string();
                                break;
                            }
                            "zsh" => {
                                shell = "Zsh".to_string();
                                break;
                            }
                            "nu" => {
                                shell = "Nushell".to_string();
                                break;
                            }
                            "fish" => {
                                shell = "Fish".to_string();
                                break;
                            }
                            _ => {
                                cur_target_pid = *parent_pid;
                            }
                        }
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        }

        (shell, term)
    }

    pub struct RawConsole {
        h_in: *mut c_void,
        h_out: *mut c_void,
        orig_in_mode: u32,
        orig_out_mode: u32,
    }

    impl RawConsole {
        pub fn init() -> Option<Self> {
            unsafe {
                let h_in = GetStdHandle(STD_INPUT_HANDLE);
                let h_out = GetStdHandle(STD_OUTPUT_HANDLE);
                if h_in.is_null() || h_out.is_null() {
                    return None;
                }

                let mut orig_in_mode = 0;
                let mut orig_out_mode = 0;
                GetConsoleMode(h_in, &mut orig_in_mode);
                GetConsoleMode(h_out, &mut orig_out_mode);

                // Non-canonical raw input
                let new_in_mode = orig_in_mode
                    & !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT);
                SetConsoleMode(h_in, new_in_mode);
                SetConsoleMode(h_out, orig_out_mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);

                Some(Self {
                    h_in,
                    h_out,
                    orig_in_mode,
                    orig_out_mode,
                })
            }
        }

        pub fn poll_key(&self, timeout_ms: u32) -> bool {
            unsafe { WaitForSingleObject(self.h_in, timeout_ms) == 0 }
        }
    }

    impl Drop for RawConsole {
        fn drop(&mut self) {
            unsafe {
                SetConsoleMode(self.h_in, self.orig_in_mode);
                SetConsoleMode(self.h_out, self.orig_out_mode);
            }
        }
    }
}

#[cfg(not(windows))]
mod win32 {
    use super::MemoryInfo;
    pub fn get_memory_info() -> MemoryInfo {
        MemoryInfo::default()
    }
    pub fn get_uptime() -> String {
        "unknown".to_string()
    }
    pub fn get_cpu_name() -> String {
        "Unknown CPU".to_string()
    }
    pub fn get_os_info() -> String {
        "Unknown OS".to_string()
    }
    pub fn get_shell_and_terminal() -> (String, String) {
        ("sh".to_string(), "terminal".to_string())
    }
    pub struct RawConsole;
    impl RawConsole {
        pub fn init() -> Option<Self> {
            Some(Self)
        }
        pub fn poll_key(&self, _ms: u32) -> bool {
            false
        }
    }
}

fn collect_metrics() -> SystemMetrics {
    let (shell, terminal) = win32::get_shell_and_terminal();
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "user".to_string());
    let host = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "host".to_string());

    SystemMetrics {
        user_host: format!("{}@{}", user, host),
        os_name: win32::get_os_info(),
        uptime_formatted: win32::get_uptime(),
        shell,
        terminal,
        cpu_name: win32::get_cpu_name(),
        memory: win32::get_memory_info(),
    }
}

// Moon ASCII Art grid: 7 rows x 20 cols
const MOON_HEIGHT: usize = 7;
const MOON_WIDTH: usize = 20;

const MOON_TEMPLATE: [&str; MOON_HEIGHT] = [
    r"       .---.        ",
    r"      /     \       ",
    r"     |   /\  |      ",
    r"     |  /  \ |      ",
    r"     |  \  / |      ",
    r"      \     /       ",
    r"       `---'        ",
];

// Simple fast pseudo-random generator (Xorshift32)
struct SimpleRng(u32);
impl SimpleRng {
    fn new(seed: u32) -> Self {
        Self(if seed == 0 { 0x12345678 } else { seed })
    }
    fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    fn choose(&mut self, chars: &[char]) -> char {
        let idx = (self.next_u32() as usize) % chars.len();
        chars[idx]
    }
}

const GLITCH_CHARS: &[char] = &[
    '#', '%', '&', '*', '+', '=', '<', '>', '~', '$', '@', '?', '0', '1', '^', ':', ';',
];

// 16 ANSI Color Blocks generator
fn render_color_palette() -> (String, String) {
    let mut row1 = String::new();
    let mut row2 = String::new();
    for i in 0..8 {
        row1.push_str(&format!("\x1b[48;5;{}m   \x1b[0m", i));
    }
    for i in 8..16 {
        row2.push_str(&format!("\x1b[48;5;{}m   \x1b[0m", i));
    }
    (row1, row2)
}

fn build_info_lines(m: &SystemMetrics) -> Vec<String> {
    let c_title = "\x1b[38;5;141m\x1b[1m"; // Lavender title
    let c_label = "\x1b[38;5;183m\x1b[1m"; // Soft violet label
    let c_val = "\x1b[37m";                // Crisp white value
    let c_dim = "\x1b[38;5;240m";           // Subtle border
    let c_reset = "\x1b[0m";

    let (pal1, pal2) = render_color_palette();

    vec![
        format!("{c_title}{}{c_reset}", m.user_host),
        format!("{c_dim}{}{c_reset}", "─".repeat(m.user_host.len().max(24))),
        format!("{c_label}OS       {c_reset}: {c_val}{}{c_reset}", m.os_name),
        format!("{c_label}Uptime   {c_reset}: {c_val}{}{c_reset}", m.uptime_formatted),
        format!("{c_label}Shell    {c_reset}: {c_val}{}{c_reset}", m.shell),
        format!("{c_label}Terminal {c_reset}: {c_val}{}{c_reset}", m.terminal),
        format!("{c_label}CPU      {c_reset}: {c_val}{}{c_reset}", m.cpu_name),
        format!(
            "{c_label}Memory   {c_reset}: {c_val}{} MB / {} MB ({}%){c_reset}",
            m.memory.used_mb, m.memory.total_mb, m.memory.percent
        ),
        String::new(),
        pal1,
        pal2,
    ]
}

const TOTAL_CANVAS_ROWS: usize = 11;

fn render_frame(
    buf: &mut String,
    frame: usize,
    decrypt_progress: f32, // 0.0 (full scramble) -> 1.0 (fully resolved)
    info_lines: &[String],
    rng: &mut SimpleRng,
) {
    buf.clear();
    // Return cursor to top-left of the reserved fetch area
    buf.push_str("\x1b[H");

    // Bobbing offset for the floating moon: sin curve oscillating between 0 and 1 row
    let float_offset = if decrypt_progress >= 0.85 {
        let t = (frame as f32 * 0.25).sin();
        if t > 0.3 {
            1
        } else {
            0
        }
    } else {
        0
    };

    // Wave moving offset
    let wave_shift = (frame / 2) % 4;

    // Palette colors
    let c_moon = "\x1b[38;5;141m\x1b[1m"; // Signature Lunula lavender glow
    let c_glitch = "\x1b[38;5;69m";        // Digital cyan/blue glitch
    let c_wave = "\x1b[38;5;39m";          // Ocean azure
    let c_wave_dim = "\x1b[38;5;31m";      // Deep sea
    let c_reset = "\x1b[0m";

    for row_idx in 0..TOTAL_CANVAS_ROWS {
        // --- 1. RENDER LEFT SIDE: MOON / WAVES (width ~ 22 chars) ---
        let mut left_col = String::with_capacity(32);

        if row_idx < 8 {
            // Moon area (with bobbing shift)
            let adjusted_row = row_idx as isize - float_offset as isize;
            if adjusted_row >= 0 && (adjusted_row as usize) < MOON_HEIGHT {
                let target_line = MOON_TEMPLATE[adjusted_row as usize];
                left_col.push_str("  "); // Left padding
                for ch in target_line.chars() {
                    if ch == ' ' {
                        left_col.push(' ');
                    } else {
                        // Check if this cell is resolved or scrambling
                        let cell_rand = (rng.next_u32() % 100) as f32 / 100.0;
                        if cell_rand > decrypt_progress {
                            // Scrambled glitch character
                            let g = rng.choose(GLITCH_CHARS);
                            left_col.push_str(c_glitch);
                            left_col.push(g);
                            left_col.push_str(c_reset);
                        } else {
                            // Resolved moon character
                            left_col.push_str(c_moon);
                            left_col.push(ch);
                            left_col.push_str(c_reset);
                        }
                    }
                }
            } else {
                left_col.push_str(&" ".repeat(MOON_WIDTH + 2));
            }
        } else {
            // Wave area (rows 8, 9, 10)
            left_col.push_str("  ");
            let wave_row = row_idx - 8;
            match wave_row {
                0 => {
                    // Crest of waves
                    left_col.push_str(c_wave);
                    let base_wave = [" ~≈~ ", "≈~≈~ ", " ~≈~ ", "≈~≈~ "];
                    for w in 0..4 {
                        left_col.push_str(base_wave[(w + wave_shift) % 4]);
                    }
                    left_col.push_str(c_reset);
                }
                1 => {
                    // Body of waves
                    left_col.push_str(c_wave_dim);
                    let base_wave2 = ["≈≈≈  ", " ≈≈≈ ", "  ≈≈≈", "≈  ≈≈"];
                    for w in 0..4 {
                        left_col.push_str(base_wave2[(w + wave_shift) % 4]);
                    }
                    left_col.push_str(c_reset);
                }
                _ => {
                    // Deep calm water
                    left_col.push_str(c_wave_dim);
                    left_col.push_str("  ·   ~    ·   ~   ·  ");
                    left_col.push_str(c_reset);
                }
            }
        }

        // --- 2. RENDER RIGHT SIDE: INFO METRICS ---
        let right_col = if row_idx < info_lines.len() {
            &info_lines[row_idx]
        } else {
            ""
        };

        // Combine left and right side with space gap
        buf.push_str(&left_col);
        buf.push_str("   ");
        buf.push_str(right_col);
        buf.push_str("\x1b[K\n"); // Clear to end of line
    }

    // Interactive hint at bottom
    buf.push_str("\n\x1b[2m  [Live Floating Loop] Press any key or Ctrl+C to exit...\x1b[0m\x1b[K\n");
}

fn main() {
    // 1. Instantly collect system metrics (< 6ms)
    let metrics = collect_metrics();
    let info_lines = build_info_lines(&metrics);

    // Setup console & Ctrl+C handler
    let running = Arc::new(AtomicBool::new(true));
    let r_clone = Arc::clone(&running);
    let _ = ctrlc_setup(r_clone);

    let raw_console = win32::RawConsole::init();

    let mut stdout = io::stdout();
    // Hide cursor & clear screen for clean layout
    let _ = stdout.write_all(b"\x1b[?25l\x1b[2J");
    let _ = stdout.flush();

    let mut rng = SimpleRng::new(0xABCD1234);
    let mut render_buf = String::with_capacity(4096);

    let start_time = Instant::now();
    let mut frame = 0;

    while running.load(Ordering::Relaxed) {
        let elapsed = start_time.elapsed().as_secs_f32();

        // Phase 1: Decrypt progress over 0.7 seconds (0.0 -> 1.0)
        let decrypt_progress = (elapsed / 0.7).min(1.0);

        render_frame(
            &mut render_buf,
            frame,
            decrypt_progress,
            &info_lines,
            &mut rng,
        );

        let _ = stdout.write_all(render_buf.as_bytes());
        let _ = stdout.flush();

        // Check if any key was pressed to exit interactive loop
        if let Some(ref con) = raw_console {
            if con.poll_key(50) {
                break;
            }
        } else {
            std::thread::sleep(Duration::from_millis(50));
        }

        frame += 1;
    }

    // Clean exit: restore cursor, reset colors
    let _ = stdout.write_all(b"\x1b[?25h\x1b[0m\n");
    let _ = stdout.flush();
}

fn ctrlc_setup(running: Arc<AtomicBool>) -> Result<(), ()> {
    #[cfg(windows)]
    unsafe {
        type HandlerRoutine = unsafe extern "system" fn(u32) -> i32;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn SetConsoleCtrlHandler(
                handler_routine: Option<HandlerRoutine>,
                add: i32,
            ) -> i32;
        }

        static mut GLOBAL_RUNNING: Option<Arc<AtomicBool>> = None;
        GLOBAL_RUNNING = Some(running);

        unsafe extern "system" fn ctrl_handler(_type: u32) -> i32 {
            unsafe {
                if let Some(ref r) = GLOBAL_RUNNING {
                    r.store(false, Ordering::SeqCst);
                }
            }
            1
        }

        SetConsoleCtrlHandler(Some(ctrl_handler), 1);
    }
    Ok(())
}
