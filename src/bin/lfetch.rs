//! `lfetch` - Ultra-fast, minimal, animated zero-dependency system info fetcher for Lunula Terminal.
//! Designed around `principle.md`:
//! - Ultra-lightweight & zero external heavy dependencies (`std` + Win32 FFI + mlua)
//! - Sub-millisecond system metric collection via Win32 / Registry API
//! - Supports two 3D animated lunar themes configurable via `lunula.lua` or CLI flags:
//!   * Animation 1: 3D Raymarched Crescent Moon & Breaking Kanagawa Wave Crest + Foam Claw (38x15)
//!   * Animation 2: 3D Analytical CSG Moon with Craters & Rolling Kanagawa Swells (44x17)
//! - Synchronized Output (`?2026h` / `?2026l`) for tear-free rendering
//! - Side-by-side layout with full system metrics & ANSI 16-color palette

use std::fmt::Write as _;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Set to true by the Ctrl+C handler to stop live mode cleanly.
static STOP: AtomicBool = AtomicBool::new(false);

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
    use std::sync::atomic::Ordering;

    const STD_INPUT_HANDLE: u32 = (-10i32) as u32;
    const STD_OUTPUT_HANDLE: u32 = (-11i32) as u32;

    const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
    const ENABLE_PROCESSED_INPUT: u32 = 0x0001;
    const ENABLE_LINE_INPUT: u32 = 0x0002;
    const ENABLE_ECHO_INPUT: u32 = 0x0004;
    const CP_UTF8: u32 = 65001;

    const KEY_EVENT: u16 = 0x0001;

    pub const HKEY_LOCAL_MACHINE: isize = 0x80000002_u32 as i32 as isize;
    pub const KEY_READ: u32 = 0x20019;
    pub const TH32CS_SNAPPROCESS: u32 = 0x00000002;
    pub const INVALID_HANDLE_VALUE: isize = -1;

    #[repr(C)]
    #[derive(Clone, Copy)]
    #[allow(non_snake_case)]
    struct KEY_EVENT_RECORD {
        bKeyDown: i32,
        wRepeatCount: u16,
        wVirtualKeyCode: u16,
        wVirtualScanCode: u16,
        uChar: u16,
        dwControlKeyState: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    union EventUnion {
        key_event: KEY_EVENT_RECORD,
        _pad: [u32; 4],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    #[allow(non_snake_case)]
    struct INPUT_RECORD {
        EventType: u16,
        Event: EventUnion,
    }

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
        fn ReadConsoleInputW(
            h_console_input: *mut c_void,
            lp_buffer: *mut INPUT_RECORD,
            n_length: u32,
            lp_number_of_events_read: *mut u32,
        ) -> i32;
        pub fn FlushConsoleInputBuffer(h_console_input: *mut c_void) -> i32;
        pub fn GetConsoleOutputCP() -> u32;
        pub fn SetConsoleOutputCP(w_code_page_id: u32) -> i32;
        pub fn SetConsoleCtrlHandler(
            handler_routine: Option<unsafe extern "system" fn(u32) -> i32>,
            add: i32,
        ) -> i32;

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

    /// Enables ANSI/VT output + UTF-8 code page for the entire run, restoring previous code page on drop.
    pub struct OutputGuard {
        old_cp: u32,
    }

    impl OutputGuard {
        pub fn new() -> Self {
            unsafe {
                let old_cp = GetConsoleOutputCP();
                SetConsoleOutputCP(CP_UTF8);
                let h = GetStdHandle(STD_OUTPUT_HANDLE);
                if !h.is_null() && (h as isize) != INVALID_HANDLE_VALUE {
                    let mut mode = 0u32;
                    if GetConsoleMode(h, &mut mode) != 0 {
                        SetConsoleMode(h, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
                    }
                }
                Self { old_cp }
            }
        }
    }

    impl Drop for OutputGuard {
        fn drop(&mut self) {
            if self.old_cp != 0 {
                unsafe { SetConsoleOutputCP(self.old_cp) };
            }
        }
    }

    pub fn install_ctrl_handler() {
        unsafe extern "system" fn handler(_ctrl_type: u32) -> i32 {
            super::STOP.store(true, Ordering::SeqCst);
            1
        }
        unsafe {
            SetConsoleCtrlHandler(Some(handler), 1);
        }
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
                // Discard stale key events (such as the Enter key that launched lfetch)
                FlushConsoleInputBuffer(h_in);

                Some(Self {
                    h_in,
                    h_out,
                    orig_in_mode,
                    orig_out_mode,
                })
            }
        }

        /// Waits up to `timeout_ms` and returns `true` ONLY on an actual key-down press.
        /// Ignores focus, mouse-move, resize, and modifier keys (Shift, Ctrl, Alt).
        pub fn poll_key(&self, timeout_ms: u32) -> bool {
            let deadline = std::time::Instant::now()
                + std::time::Duration::from_millis(timeout_ms as u64);
            loop {
                let now = std::time::Instant::now();
                if now >= deadline {
                    return false;
                }
                let rem = (deadline - now).as_millis() as u32;
                let wait = unsafe { WaitForSingleObject(self.h_in, rem.max(1)) };
                if wait != 0 {
                    return false;
                }
                let mut rec = unsafe { std::mem::zeroed::<INPUT_RECORD>() };
                let mut read = 0u32;
                let ok = unsafe { ReadConsoleInputW(self.h_in, &mut rec, 1, &mut read) };
                if ok == 0 || read == 0 {
                    return false;
                }
                if rec.EventType == KEY_EVENT {
                    let ke = unsafe { rec.Event.key_event };
                    if ke.bKeyDown != 0 {
                        let vk = ke.wVirtualKeyCode;
                        if vk != 0x10 && vk != 0x11 && vk != 0x12 {
                            return true;
                        }
                    }
                }
            }
        }
    }

    impl Drop for RawConsole {
        fn drop(&mut self) {
            unsafe {
                FlushConsoleInputBuffer(self.h_in);
                SetConsoleMode(self.h_in, self.orig_in_mode);
                SetConsoleMode(self.h_out, self.orig_out_mode);
            }
        }
    }
}

#[cfg(not(windows))]
mod win32 {
    use super::MemoryInfo;
    use std::time::Duration;

    pub fn get_memory_info() -> MemoryInfo {
        MemoryInfo {
            total_mb: 16384,
            used_mb: 6144,
            percent: 37,
        }
    }
    pub fn get_uptime() -> String {
        "2h 15m".to_string()
    }
    pub fn get_cpu_name() -> String {
        "Generic x86_64 CPU".to_string()
    }
    pub fn get_os_info() -> String {
        "Unix-like OS".to_string()
    }
    pub fn get_shell_and_terminal() -> (String, String) {
        ("sh".to_string(), "Lunula Terminal".to_string())
    }
    pub fn install_ctrl_handler() {}

    pub struct OutputGuard;
    impl OutputGuard {
        pub fn new() -> Self {
            Self
        }
    }

    pub struct RawConsole;
    impl RawConsole {
        pub fn init() -> Option<Self> {
            Some(Self)
        }
        pub fn poll_key(&self, ms: u32) -> bool {
            std::thread::sleep(Duration::from_millis(ms as u64));
            false
        }
    }
}

fn collect_metrics() -> SystemMetrics {
    let (shell, terminal) = win32::get_shell_and_terminal();
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "user".to_string());
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "host".to_string());

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

// ───────────────────────────── Shared Kanagawa Info Panel ─────────────────────────────

const C_RESET: &str = "\x1b[0m";
const C_TITLE: &str = "\x1b[38;2;149;127;184m\x1b[1m"; // Oni violet (#957FB8) bold
const C_DIM: &str = "\x1b[38;2;84;84;109m";           // Sumi ink 6 (#54546D)
const C_LABEL: &str = "\x1b[38;2;126;156;216m\x1b[1m"; // Crystal blue (#7E9CD8) bold
const C_VAL: &str = "\x1b[38;2;220;215;186m";          // Fuji white (#DCD7BA)

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
    let (pal1, pal2) = render_color_palette();
    let divider_len = m.user_host.chars().count().max(24);

    vec![
        format!("{C_TITLE}{}{C_RESET}", m.user_host),
        format!("{C_DIM}{}{C_RESET}", "─".repeat(divider_len)),
        format!("{C_LABEL}OS       {C_RESET}: {C_VAL}{}{C_RESET}", m.os_name),
        format!("{C_LABEL}Uptime   {C_RESET}: {C_VAL}{}{C_RESET}", m.uptime_formatted),
        format!("{C_LABEL}Shell    {C_RESET}: {C_VAL}{}{C_RESET}", m.shell),
        format!("{C_LABEL}Terminal {C_RESET}: {C_VAL}{}{C_RESET}", m.terminal),
        format!("{C_LABEL}CPU      {C_RESET}: {C_VAL}{}{C_RESET}", m.cpu_name),
        format!(
            "{C_LABEL}Memory   {C_RESET}: {C_VAL}{} MB / {} MB ({}%){C_RESET}",
            m.memory.used_mb, m.memory.total_mb, m.memory.percent
        ),
        String::new(),
        pal1,
        pal2,
    ]
}

// ───────────────────────────── Animation 1: 3D Raymarched Moon & Great Wave ─────────────────────────────
pub mod anim1 {
    use super::*;

    pub const CANVAS_W: usize = 38;
    pub const CANVAS_H: usize = 15;

    #[derive(Clone, Copy)]
    pub struct Vec3 {
        pub x: f32,
        pub y: f32,
        pub z: f32,
    }

    impl Vec3 {
        #[inline]
        pub const fn new(x: f32, y: f32, z: f32) -> Self {
            Self { x, y, z }
        }
        #[inline]
        pub fn add(self, o: Self) -> Self {
            Self::new(self.x + o.x, self.y + o.y, self.z + o.z)
        }
        #[inline]
        pub fn sub(self, o: Self) -> Self {
            Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
        }
        #[inline]
        pub fn scale(self, s: f32) -> Self {
            Self::new(self.x * s, self.y * s, self.z * s)
        }
        #[inline]
        pub fn dot(self, o: Self) -> f32 {
            self.x * o.x + self.y * o.y + self.z * o.z
        }
        #[inline]
        pub fn len(self) -> f32 {
            self.dot(self).sqrt()
        }
        #[inline]
        pub fn normalize(self) -> Self {
            let l = self.len().max(1e-6);
            self.scale(1.0 / l)
        }
        #[inline]
        pub fn rot_y(self, a: f32) -> Self {
            let (s, c) = a.sin_cos();
            Self::new(self.x * c + self.z * s, self.y, -self.x * s + self.z * c)
        }
        #[inline]
        pub fn rot_x(self, a: f32) -> Self {
            let (s, c) = a.sin_cos();
            Self::new(self.x, self.y * c - self.z * s, self.y * s + self.z * c)
        }
        #[inline]
        pub fn rot_z(self, a: f32) -> Self {
            let (s, c) = a.sin_cos();
            Self::new(self.x * c - self.y * s, self.x * s + self.y * c, self.z)
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Rgb(pub u8, pub u8, pub u8);

    #[inline]
    fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
        let t = t.clamp(0.0, 1.0);
        (a as f32 + (b as f32 - a as f32) * t).round() as u8
    }

    #[inline]
    fn lerp_rgb(a: Rgb, b: Rgb, t: f32) -> Rgb {
        Rgb(
            lerp_u8(a.0, b.0, t),
            lerp_u8(a.1, b.1, t),
            lerp_u8(a.2, b.2, t),
        )
    }

    #[inline]
    fn smax(a: f32, b: f32, k: f32) -> f32 {
        let h = (k - (a - b).abs()).max(0.0) / k;
        a.max(b) + h * h * k * 0.25
    }

    #[inline]
    fn sdf_crescent(p: Vec3, yaw: f32, pitch: f32, tilt: f32) -> f32 {
        let q = p.rot_y(yaw).rot_x(pitch).rot_z(tilt);
        let d_outer = q.len() - 0.96;
        let c_off = Vec3::new(q.x - 0.44, q.y - 0.06, q.z * 0.58);
        let d_cut = c_off.len() - 0.84;
        smax(d_outer, -d_cut, 0.04)
    }

    #[inline]
    fn moon_normal(p: Vec3, yaw: f32, pitch: f32, tilt: f32) -> Vec3 {
        let e = 0.0025;
        let dx = sdf_crescent(p.add(Vec3::new(e, 0.0, 0.0)), yaw, pitch, tilt)
            - sdf_crescent(p.sub(Vec3::new(e, 0.0, 0.0)), yaw, pitch, tilt);
        let dy = sdf_crescent(p.add(Vec3::new(0.0, e, 0.0)), yaw, pitch, tilt)
            - sdf_crescent(p.sub(Vec3::new(0.0, e, 0.0)), yaw, pitch, tilt);
        let dz = sdf_crescent(p.add(Vec3::new(0.0, 0.0, e)), yaw, pitch, tilt)
            - sdf_crescent(p.sub(Vec3::new(0.0, 0.0, e)), yaw, pitch, tilt);
        Vec3::new(dx, dy, dz).normalize()
    }

    fn sample_moon_3d(nx: f32, ny: f32, t: f32) -> Option<(char, Rgb, bool)> {
        let bob_y = 0.09 * (t * 1.6).sin();
        let yaw = 0.52 * (t * 0.95).sin();
        let pitch = 0.14 * (t * 0.70).cos();
        let tilt = -0.15 + 0.05 * (t * 0.85).sin();

        let ro = Vec3::new(-0.46, -0.48 - bob_y, 3.25);
        let rd = Vec3::new(nx * 1.45, ny * 1.25, -2.0).normalize();

        let b = ro.dot(rd);
        let c = ro.dot(ro) - 1.05 * 1.05;
        let disc = b * b - c;
        if disc < 0.0 {
            return None;
        }

        let mut dist_t = (-b - disc.sqrt()).max(0.0);
        let max_t = (-b + disc.sqrt()).min(5.2);

        for _ in 0..32 {
            let p = ro.add(rd.scale(dist_t));
            let d = sdf_crescent(p, yaw, pitch, tilt);
            if d < 0.004 {
                let n = moon_normal(p, yaw, pitch, tilt);
                let view = rd.scale(-1.0);

                let light_key = Vec3::new(-0.62, 0.52, 0.68).normalize();
                let light_rim = Vec3::new(0.65, -0.30, 0.45).normalize();

                let diff = n.dot(light_key).max(0.0);
                let fill = n.dot(light_rim).max(0.0) * 0.30;

                let half_v = light_key.add(view).normalize();
                let spec = n.dot(half_v).max(0.0).powf(18.0);
                let fresnel = (1.0 - n.dot(view).max(0.0)).powf(2.4);

                let lum = (0.12 + diff * 0.74 + fill + spec * 0.52 + fresnel * 0.28).clamp(0.0, 1.0);

                const RAMP: &[u8] = b".:-=+*#%@";
                let idx = ((lum * (RAMP.len() - 1) as f32).round() as usize).min(RAMP.len() - 1);
                let ch = RAMP[idx] as char;

                let col = if lum < 0.30 {
                    lerp_rgb(Rgb(84, 84, 109), Rgb(149, 127, 184), lum / 0.30)
                } else if lum < 0.62 {
                    lerp_rgb(Rgb(149, 127, 184), Rgb(126, 156, 216), (lum - 0.30) / 0.32)
                } else if lum < 0.84 {
                    lerp_rgb(Rgb(126, 156, 216), Rgb(230, 195, 132), (lum - 0.62) / 0.22)
                } else {
                    lerp_rgb(Rgb(230, 195, 132), Rgb(220, 215, 186), (lum - 0.84) / 0.16)
                };

                let bold = lum > 0.75;
                return Some((ch, col, bold));
            }
            dist_t += d;
            if dist_t > max_t {
                break;
            }
        }
        None
    }

    fn sample_kanagawa_wave(col: usize, row: usize, t: f32) -> Option<(char, Rgb, bool)> {
        let x = col as f32;
        let y = row as f32;

        let surge = 0.38 * (t * 1.45).sin();
        let curl_peak_x = 7.8 + 1.1 * (t * 0.9).sin();
        let dx_crest = x - curl_peak_x;

        let crest_profile = if dx_crest < 0.0 {
            (-dx_crest * dx_crest * 0.020).exp() * (4.1 + surge)
        } else {
            (-dx_crest * dx_crest * 0.085).exp() * (4.1 + surge)
        };

        let right_swell = 0.65 * ((x * 0.30) - t * 2.1).sin()
            + 0.35 * ((x * 0.55) + t * 1.35).cos();

        let wave_top_y = 11.7 - crest_profile - right_swell * 0.55;

        let claw_cx = curl_peak_x + 3.0 + 0.45 * (t * 1.8).cos();
        let claw_cy = 8.2 - 0.30 * surge;
        let d_claw = ((x - claw_cx) * 0.48).hypot((y - claw_cy) * 1.15);
        if d_claw < 1.30 && x > curl_peak_x + 0.3 && y < wave_top_y + 0.4 {
            let phase = (x * 1.9 + y * 2.7 + t * 4.5).sin() * 0.5 + 0.5;
            let ch = if d_claw < 0.62 {
                if phase > 0.5 { '@' } else { '#' }
            } else if phase > 0.55 {
                '*'
            } else if phase > 0.28 {
                '\''
            } else {
                '.'
            };
            let col_rgb = lerp_rgb(Rgb(163, 212, 213), Rgb(220, 215, 186), phase);
            return Some((ch, col_rgb, d_claw < 0.72));
        }

        if y >= wave_top_y - 1.3
            && y < wave_top_y - 0.1
            && x > curl_peak_x - 1.0
            && x < curl_peak_x + 6.5
        {
            let h = ((x * 13.7 + y * 7.3 + (t * 3.0).floor()).sin() * 43758.545)
                .fract()
                .abs();
            if h > 0.85 {
                let ch = if h > 0.93 { '*' } else { '.' };
                return Some((ch, Rgb(200, 192, 147), false));
            }
        }

        if y >= wave_top_y {
            let depth = y - wave_top_y;
            let moon_x = 22.5;
            let refl_half_w = (4.8 - depth * 0.50).max(1.3);
            let refl_dist = (x - moon_x).abs();
            let shimmer = (x * 1.4 - t * 4.2 + y * 2.1).sin() * 0.5 + 0.5;
            let is_reflected = refl_dist < refl_half_w && shimmer > 0.50 && depth > 0.30;

            if depth < 0.72 {
                let foam_ch = if dx_crest > -1.5 && dx_crest < 3.5 {
                    if ((x + t * 4.0) as i32) % 2 == 0 { '%' } else { '^' }
                } else if (x * 0.8 - t * 3.0).sin() > 0.2 {
                    '~'
                } else {
                    '='
                };
                let foam_col = if is_reflected {
                    Rgb(230, 195, 132)
                } else if dx_crest > -3.0 && dx_crest < 4.5 {
                    Rgb(220, 215, 186)
                } else {
                    Rgb(127, 180, 202)
                };
                return Some((foam_ch, foam_col, dx_crest > -2.0 && dx_crest < 4.0));
            }

            let flow = (x * 0.45 - t * 2.6 + y * 0.9).sin() * 0.5 + 0.5;
            let ch = if is_reflected {
                if shimmer > 0.78 { '=' } else { '~' }
            } else if depth < 1.8 {
                if flow > 0.62 {
                    '~'
                } else if flow > 0.30 {
                    '/'
                } else {
                    '-'
                }
            } else if flow > 0.58 {
                '~'
            } else if flow > 0.25 {
                '='
            } else {
                '_'
            };

            let base_col = if depth < 1.7 {
                lerp_rgb(Rgb(126, 156, 216), Rgb(45, 79, 103), (depth - 0.72) / 1.0)
            } else {
                lerp_rgb(
                    Rgb(45, 79, 103),
                    Rgb(34, 50, 73),
                    ((depth - 1.7) / 2.2).min(1.0),
                )
            };

            let final_col = if is_reflected {
                lerp_rgb(base_col, Rgb(230, 195, 132), 0.78)
            } else {
                base_col
            };

            return Some((ch, final_col, is_reflected && shimmer > 0.80));
        }
        None
    }

    fn sample_starfield(col: usize, row: usize, t: f32) -> Option<(char, Rgb, bool)> {
        const STARS: [(usize, usize, f32); 12] = [
            (3, 1, 0.0),
            (9, 0, 1.4),
            (13, 3, 2.9),
            (31, 1, 2.3),
            (35, 2, 0.7),
            (5, 4, 3.1),
            (33, 5, 1.9),
            (2, 6, 4.2),
            (36, 7, 2.8),
            (27, 0, 3.7),
            (11, 1, 5.0),
            (32, 9, 0.4),
        ];

        for &(sx, sy, phase) in &STARS {
            if col == sx && row == sy {
                let pulse = (t * 2.4 + phase).sin() * 0.5 + 0.5;
                let (ch, rgb, bold) = if pulse > 0.80 {
                    ('*', Rgb(230, 195, 132), true)
                } else if pulse > 0.45 {
                    ('+', Rgb(220, 165, 97), false)
                } else {
                    ('.', Rgb(113, 126, 154), false)
                };
                return Some((ch, rgb, bold));
            }
        }
        None
    }

    pub fn render_frame(buf: &mut String, info_lines: &[String], t: f32, is_live: bool) {
        buf.clear();
        let info_top_offset = (CANVAS_H.saturating_sub(info_lines.len())) / 2;

        for row in 0..CANVAS_H {
            buf.push_str("  ");

            let mut last_color: Option<(Rgb, bool)> = None;

            for col in 0..CANVAS_W {
                let nx = ((col as f32 + 0.5) / CANVAS_W as f32 - 0.5) * 2.0;
                let ny = (0.5 - (row as f32 + 0.5) / CANVAS_H as f32) * 2.0;

                let pixel = if let Some(wave_px) = sample_kanagawa_wave(col, row, t) {
                    Some(wave_px)
                } else if let Some(moon_px) = sample_moon_3d(nx, ny, t) {
                    Some(moon_px)
                } else {
                    sample_starfield(col, row, t)
                };

                if let Some((ch, rgb, bold)) = pixel {
                    if last_color != Some((rgb, bold)) {
                        if bold {
                            let _ = write!(
                                buf,
                                "\x1b[0;1;38;2;{};{};{}m",
                                rgb.0, rgb.1, rgb.2
                            );
                        } else {
                            let _ = write!(
                                buf,
                                "\x1b[0;38;2;{};{};{}m",
                                rgb.0, rgb.1, rgb.2
                            );
                        }
                        last_color = Some((rgb, bold));
                    }
                    buf.push(ch);
                } else {
                    if last_color.is_some() {
                        buf.push_str(C_RESET);
                        last_color = None;
                    }
                    buf.push(' ');
                }
            }

            if last_color.is_some() {
                buf.push_str(C_RESET);
            }

            buf.push_str("   ");

            if row >= info_top_offset {
                let info_idx = row - info_top_offset;
                if info_idx < info_lines.len() {
                    buf.push_str(&info_lines[info_idx]);
                }
            }

            if is_live {
                buf.push_str("\x1b[K");
            }
            buf.push('\n');
        }

        if is_live {
            buf.push_str("\n\x1b[2m  [Animation 1: 3D Raymarched Wave] Press any key or Ctrl+C to exit...\x1b[0m\x1b[K\n");
        }
    }
}

// ───────────────────────────── Animation 2: 3D Analytical Moon with Craters & Rolling Swells ─────────────────────────────
pub mod anim2 {
    use super::*;

    pub const CANVAS_W: usize = 44;
    pub const SKY_ROWS: usize = 13;
    pub const WAVE_ROWS: usize = 4;
    pub const TOTAL_ROWS: usize = SKY_ROWS + WAVE_ROWS;
    pub const INFO_START: usize = 3;
    const MOON_COL: f32 = 22.0;

    type RgbF = (f32, f32, f32);

    const fn rgb_f(r: u8, g: u8, b: u8) -> RgbF {
        (r as f32, g as f32, b as f32)
    }

    const K_FUJI: RgbF = rgb_f(220, 215, 186);
    const K_CARP: RgbF = rgb_f(230, 195, 132);
    const K_AUTUMN: RgbF = rgb_f(220, 165, 97);
    const K_LIGHT_BLUE: RgbF = rgb_f(163, 212, 213);
    const K_SPRING_BLUE: RgbF = rgb_f(127, 180, 202);
    const K_CRYSTAL: RgbF = rgb_f(126, 156, 216);
    const K_ONI: RgbF = rgb_f(149, 127, 184);
    const K_STARDUST: RgbF = rgb_f(113, 126, 154);

    const MOON_STOPS: [(f32, RgbF); 6] = [
        (0.00, rgb_f(52, 48, 84)),
        (0.18, rgb_f(104, 88, 150)),
        (0.40, K_ONI),
        (0.62, K_CRYSTAL),
        (0.82, K_LIGHT_BLUE),
        (1.00, K_FUJI),
    ];

    fn mix(a: RgbF, b: RgbF, t: f32) -> RgbF {
        (
            a.0 + (b.0 - a.0) * t,
            a.1 + (b.1 - a.1) * t,
            a.2 + (b.2 - a.2) * t,
        )
    }

    fn scale(c: RgbF, k: f32) -> RgbF {
        (c.0 * k, c.1 * k, c.2 * k)
    }

    fn gradient(stops: &[(f32, RgbF)], t: f32) -> RgbF {
        let t = t.clamp(0.0, 1.0);
        for w in stops.windows(2) {
            if t <= w[1].0 {
                let span = w[1].0 - w[0].0;
                let k = if span > 0.0 { (t - w[0].0) / span } else { 0.0 };
                return mix(w[0].1, w[1].1, k.clamp(0.0, 1.0));
            }
        }
        stops[stops.len() - 1].1
    }

    fn hash3(a: u32, b: u32, c: u32) -> f32 {
        let mut h = a.wrapping_mul(374_761_393) ^ b.wrapping_mul(668_265_263) ^ c.wrapping_mul(2_147_483_647);
        h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
        h ^= h >> 16;
        (h & 0xffff) as f32 / 65535.0
    }

    #[derive(Clone, Copy)]
    struct Cell {
        ch: char,
        rgb: RgbF,
    }

    const BLANK: Cell = Cell {
        ch: ' ',
        rgb: (0.0, 0.0, 0.0),
    };

    const MOON_R: f32 = 11.0;
    const CUT_C: [f32; 3] = [6.4, 1.0, 0.0];
    const CUT_AX: [f32; 3] = [10.0, 10.0, 16.0];
    const MOON_ROLL: f32 = 0.32;

    const CRATERS: [([f32; 3], f32); 7] = [
        ([-0.80, 0.20, 0.55], 0.30),
        ([-0.60, -0.45, 0.65], 0.22),
        ([-0.92, -0.05, 0.35], 0.18),
        ([-0.30, 0.60, 0.74], 0.20),
        ([-0.45, 0.05, 0.88], 0.14),
        ([-0.70, 0.55, 0.45], 0.13),
        ([-0.35, -0.75, 0.55], 0.16),
    ];

    fn local_to_world(v: [f32; 3], yaw: f32, roll: f32) -> [f32; 3] {
        let (sy, cy) = yaw.sin_cos();
        let x1 = v[0] * cy + v[2] * sy;
        let z1 = -v[0] * sy + v[2] * cy;
        let (sr, cr) = roll.sin_cos();
        [x1 * cr - v[1] * sr, x1 * sr + v[1] * cr, z1]
    }

    fn world_to_local(v: [f32; 3], yaw: f32, roll: f32) -> [f32; 3] {
        let (sr, cr) = (-roll).sin_cos();
        let x1 = v[0] * cr - v[1] * sr;
        let y1 = v[0] * sr + v[1] * cr;
        let (sy, cy) = (-yaw).sin_cos();
        [x1 * cy + v[2] * sy, y1, -x1 * sy + v[2] * cy]
    }

    fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }

    fn normalize(v: [f32; 3]) -> [f32; 3] {
        let l = dot(v, v).sqrt().max(1e-6);
        [v[0] / l, v[1] / l, v[2] / l]
    }

    struct Moon {
        yaw: f32,
        roll: f32,
        ex: [f32; 3],
        ey: [f32; 3],
        dir: [f32; 3],
        light: [f32; 3],
    }

    impl Moon {
        fn new(t: f32) -> Self {
            let yaw = 0.7 * (t * 0.7).sin();
            let roll = MOON_ROLL;
            Self {
                yaw,
                roll,
                ex: world_to_local([1.0, 0.0, 0.0], yaw, roll),
                ey: world_to_local([0.0, 1.0, 0.0], yaw, roll),
                dir: world_to_local([0.0, 0.0, -1.0], yaw, roll),
                light: normalize([-0.55 + 0.22 * (t * 0.4).sin(), 0.45, 0.72]),
            }
        }

        fn shade(&self, x: f32, y: f32) -> Option<f32> {
            let ra2 = MOON_R * MOON_R - x * x - y * y;
            if ra2 <= 0.0 {
                return None;
            }
            let za = ra2.sqrt();

            let mut p = [x, y, za];
            let mut n = [x / MOON_R, y / MOON_R, za / MOON_R];
            let mut inner = false;

            let o = [
                x * self.ex[0] + y * self.ey[0],
                x * self.ex[1] + y * self.ey[1],
                x * self.ex[2] + y * self.ey[2],
            ];
            let d = self.dir;
            let q = [
                (o[0] - CUT_C[0]) / CUT_AX[0],
                (o[1] - CUT_C[1]) / CUT_AX[1],
                (o[2] - CUT_C[2]) / CUT_AX[2],
            ];
            let e = [d[0] / CUT_AX[0], d[1] / CUT_AX[1], d[2] / CUT_AX[2]];
            let a = dot(e, e);
            let b = dot(q, e);
            let cc = dot(q, q) - 1.0;
            let disc = b * b - a * cc;

            if disc > 0.0 {
                let s = disc.sqrt();
                let t0 = (-b - s) / a;
                let t1 = (-b + s) / a;
                let b1 = -t0;
                let b0 = -t1;
                if !(za > b1 || za < b0) {
                    if b0 > -za {
                        p = [x, y, b0];
                        let pl = [o[0] + t1 * d[0], o[1] + t1 * d[1], o[2] + t1 * d[2]];
                        let g = [
                            (pl[0] - CUT_C[0]) / (CUT_AX[0] * CUT_AX[0]),
                            (pl[1] - CUT_C[1]) / (CUT_AX[1] * CUT_AX[1]),
                            (pl[2] - CUT_C[2]) / (CUT_AX[2] * CUT_AX[2]),
                        ];
                        n = local_to_world(normalize([-g[0], -g[1], -g[2]]), self.yaw, self.roll);
                        inner = true;
                    } else {
                        return None;
                    }
                }
            }

            let wrapped = ((dot(n, self.light) + 0.2) / 1.2).max(0.0);
            let rim = (1.0 - n[2].max(0.0)).powi(2) * 0.22;
            let mut i = 0.06 + 0.88 * wrapped + rim;

            if inner {
                i *= 0.85;
            } else {
                let u = world_to_local([p[0] / MOON_R, p[1] / MOON_R, p[2] / MOON_R], self.yaw, self.roll);
                for (dir, rad) in CRATERS.iter() {
                    let c = normalize(*dir);
                    let dd = ((u[0] - c[0]).powi(2) + (u[1] - c[1]).powi(2) + (u[2] - c[2]).powi(2)).sqrt();
                    if dd < *rad {
                        let k = dd / rad;
                        i *= 0.58 + 0.42 * k * k;
                        if k > 0.78 {
                            i *= 1.12;
                        }
                    }
                }
            }
            Some(i.clamp(0.0, 1.0))
        }
    }

    const SUBSAMPLES: [(f32, f32); 4] = [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)];
    const SHADE_RAMP: &[u8] = b".,:;-=+*#%@";

    fn draw_moon(cells: &mut [Cell], t: f32) {
        let moon = Moon::new(t);
        let cy = 6.4 + 0.6 * (t * 1.3).sin();

        for row in 0..SKY_ROWS {
            for col in 0..CANVAS_W {
                let mut hits = 0u32;
                let mut sum = 0.0f32;
                for &(sx, sy) in SUBSAMPLES.iter() {
                    let x = col as f32 + sx - MOON_COL;
                    let y = (cy - (row as f32 + sy)) * 2.0;
                    if let Some(i) = moon.shade(x, y) {
                        hits += 1;
                        sum += i;
                    }
                }

                let cell = &mut cells[row * CANVAS_W + col];
                if hits > 0 {
                    let cov = hits as f32 / 4.0;
                    let avg = sum / hits as f32;
                    let i = (avg * (0.45 + 0.55 * cov)).clamp(0.0, 1.0);
                    let idx = (i * (SHADE_RAMP.len() - 1) as f32 + 0.5) as usize;
                    *cell = Cell {
                        ch: SHADE_RAMP[idx] as char,
                        rgb: gradient(&MOON_STOPS, i),
                    };
                } else {
                    let x = col as f32 + 0.5 - MOON_COL;
                    let y = (cy - (row as f32 + 0.5)) * 2.0;
                    let d = (x * x + y * y).sqrt();
                    if d > MOON_R && d < MOON_R + 5.0 {
                        let glow = (1.0 - (d - MOON_R) / 5.0) * (0.85 + 0.15 * (t * 2.0 + d * 0.6).sin());
                        if glow > 0.45 {
                            *cell = Cell {
                                ch: '·',
                                rgb: mix(rgb_f(60, 58, 95), K_ONI, glow),
                            };
                        }
                    }
                }
            }
        }
    }

    const STARS: [(usize, usize, usize); 14] = [
        (3, 1, 0),
        (8, 4, 1),
        (38, 2, 2),
        (41, 6, 3),
        (5, 9, 2),
        (36, 10, 0),
        (14, 0, 1),
        (31, 0, 3),
        (1, 6, 1),
        (42, 11, 2),
        (10, 11, 3),
        (40, 0, 0),
        (6, 2, 3),
        (35, 4, 1),
    ];

    fn draw_stars(cells: &mut [Cell], t: f32) {
        for &(col, row, phase) in STARS.iter() {
            let cell = &mut cells[row * CANVAS_W + col];
            if cell.ch != ' ' {
                continue;
            }
            let step = ((t * 3.0) as usize + phase) % 4;
            *cell = match step {
                0 => Cell { ch: '✦', rgb: K_CARP },
                1 => Cell { ch: '✧', rgb: K_FUJI },
                2 => Cell { ch: '⋆', rgb: K_AUTUMN },
                _ => Cell { ch: '·', rgb: K_STARDUST },
            };
        }
    }

    fn draw_waves(cells: &mut [Cell], t: f32) {
        for wr in 0..WAVE_ROWS {
            let darken = 1.0 - 0.09 * wr as f32;
            for col in 0..CANVAS_W {
                let x = col as f32;

                let ph = x * 0.22 - t * 1.1;
                let ph = ph + 0.55 * ph.sin();
                let surface = 1.2
                    + 0.9 * ph.sin()
                    + 0.45 * (x * 0.5 + t * 0.8).sin()
                    + 0.2 * (x * 1.3 - t * 1.9).sin();

                let d = wr as f32 + 0.5 - surface;
                let cell = &mut cells[(SKY_ROWS + wr) * CANVAS_W + col];

                if d < 0.0 {
                    if d > -1.4 && hash3(col as u32, wr as u32, (t * 5.0) as u32) < 0.07 {
                        let ch = match (col + wr) % 3 {
                            0 => '\'',
                            1 => '·',
                            _ => '.',
                        };
                        *cell = Cell { ch, rgb: K_FUJI };
                    }
                    continue;
                }

                let (ch, color) = if d < 0.6 {
                    ('~', K_FUJI)
                } else if d < 1.5 {
                    ('≈', K_SPRING_BLUE)
                } else if d < 2.6 {
                    ('~', rgb_f(96, 120, 180))
                } else {
                    ('≈', rgb_f(58, 90, 130))
                };
                let mut color = scale(color, darken);
                let mut ch = ch;

                let dx = (x + 0.5 - MOON_COL).abs();
                let max_w = 7.0 - wr as f32 * 1.3;
                if dx < max_w {
                    let glint = (x * 1.7 + t * 3.0 + wr as f32 * 2.1).sin();
                    if glint > 0.35 {
                        let k = (1.0 - dx / max_w).clamp(0.0, 1.0);
                        ch = if d < 1.0 { '=' } else { '-' };
                        color = mix(color, mix(K_CARP, K_FUJI, 0.4), 0.35 + 0.6 * k);
                    }
                }
                *cell = Cell { ch, rgb: color };
            }
        }
    }

    fn render_scene(t: f32) -> Vec<Cell> {
        let mut cells = vec![BLANK; CANVAS_W * TOTAL_ROWS];
        draw_moon(&mut cells, t);
        draw_stars(&mut cells, t);
        draw_waves(&mut cells, t);
        cells
    }

    pub fn render_frame(buf: &mut String, info_lines: &[String], t: f32, is_live: bool) {
        buf.clear();
        let cells = render_scene(t);

        for row in 0..TOTAL_ROWS {
            buf.push_str("  ");

            let mut last: Option<(u8, u8, u8)> = None;
            for col in 0..CANVAS_W {
                let c = cells[row * CANVAS_W + col];
                if c.ch == ' ' {
                    buf.push(' ');
                    continue;
                }
                let q = (
                    c.rgb.0.round().clamp(0.0, 255.0) as u8,
                    c.rgb.1.round().clamp(0.0, 255.0) as u8,
                    c.rgb.2.round().clamp(0.0, 255.0) as u8,
                );
                if last != Some(q) {
                    let _ = write!(buf, "\x1b[38;2;{};{};{}m", q.0, q.1, q.2);
                    last = Some(q);
                }
                buf.push(c.ch);
            }
            buf.push_str(C_RESET);

            buf.push_str("   ");
            if row >= INFO_START {
                if let Some(line) = info_lines.get(row - INFO_START) {
                    buf.push_str(line);
                }
            }

            if is_live {
                buf.push_str("\x1b[K");
            }
            buf.push('\n');
        }

        if is_live {
            buf.push_str("\n\x1b[2m  [Animation 2: 3D Moon & Craters] Press any key or Ctrl+C to exit...\x1b[0m\x1b[K\n");
        }
    }
}

// ───────────────────────────── Lua Configuration Loader ─────────────────────────────

#[derive(Clone, Copy, Debug)]
pub struct FetchConfig {
    pub animation: u8,
    pub live: bool,
}

impl Default for FetchConfig {
    fn default() -> Self {
        Self {
            animation: 1,
            live: true,
        }
    }
}

/// Locates `lunula.lua` in standard configuration directories.
fn find_lunula_config_path() -> Option<PathBuf> {
    // 1. Current working directory
    let local = Path::new("lunula.lua");
    if local.exists() {
        return Some(local.to_path_buf());
    }

    // 2. Binary executable's parent directory
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            let exe_local = parent.join("lunula.lua");
            if exe_local.exists() {
                return Some(exe_local);
            }
        }
    }

    // 3. APPDATA / USERPROFILE standard paths
    if let Ok(appdata) = std::env::var("APPDATA") {
        let appdata_cfg = Path::new(&appdata).join("lunula").join("lunula.lua");
        if appdata_cfg.exists() {
            return Some(appdata_cfg);
        }
        let appdata_alt = Path::new(&appdata).join("lunula").join("config.lua");
        if appdata_alt.exists() {
            return Some(appdata_alt);
        }
    }

    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        let user_cfg = Path::new(&userprofile)
            .join(".config")
            .join("lunula")
            .join("lunula.lua");
        if user_cfg.exists() {
            return Some(user_cfg);
        }
        let user_alt = Path::new(&userprofile)
            .join(".config")
            .join("lunula")
            .join("config.lua");
        if user_alt.exists() {
            return Some(user_alt);
        }
        let home_cfg = Path::new(&userprofile).join("lunula.lua");
        if home_cfg.exists() {
            return Some(home_cfg);
        }
    }

    None
}

/// Loads `animation` and `live` preferences from `lunula.lua` using `mlua`, with a fast text fallback.
fn load_fetch_config() -> FetchConfig {
    let mut cfg = FetchConfig::default();

    if let Some(path) = find_lunula_config_path() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            // 1. Evaluate via mlua
            let mut parsed_via_lua = false;
            let lua = mlua::Lua::new();
            if let Ok(val) = lua.load(&content).eval::<mlua::Value>() {
                if let mlua::Value::Table(tbl) = val {
                    parsed_via_lua = true;
                    if let Ok(fetch_tbl) = tbl.get::<mlua::Table>("fetch") {
                        if let Ok(anim) = fetch_tbl.get::<u8>("animation") {
                            if anim == 1 || anim == 2 {
                                cfg.animation = anim;
                            }
                        }
                        if let Ok(live) = fetch_tbl.get::<bool>("live") {
                            cfg.live = live;
                        }
                    } else if let Ok(lfetch_tbl) = tbl.get::<mlua::Table>("lfetch") {
                        if let Ok(anim) = lfetch_tbl.get::<u8>("animation") {
                            if anim == 1 || anim == 2 {
                                cfg.animation = anim;
                            }
                        }
                        if let Ok(live) = lfetch_tbl.get::<bool>("live") {
                            cfg.live = live;
                        }
                    } else if let Ok(anim) = tbl.get::<u8>("animation") {
                        if anim == 1 || anim == 2 {
                            cfg.animation = anim;
                        }
                    }
                }
            }

            // 2. Fallback text parsing if mlua encountered syntax error
            if !parsed_via_lua {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("--") {
                        continue;
                    }
                    if trimmed.contains("animation") {
                        if let Some((_, val_part)) = trimmed.split_once('=') {
                            let val_str = val_part
                                .split("--")
                                .next()
                                .unwrap_or("")
                                .trim()
                                .trim_matches(&[',', ' ', ';', '"', '\''][..]);
                            if val_str == "2" {
                                cfg.animation = 2;
                            } else if val_str == "1" {
                                cfg.animation = 1;
                            }
                        }
                    }
                    if trimmed.contains("live") {
                        if let Some((_, val_part)) = trimmed.split_once('=') {
                            let val_str = val_part
                                .split("--")
                                .next()
                                .unwrap_or("")
                                .trim()
                                .trim_matches(&[',', ' ', ';'][..]);
                            if val_str == "false" {
                                cfg.live = false;
                            } else if val_str == "true" {
                                cfg.live = true;
                            }
                        }
                    }
                }
            }
        }
    }

    cfg
}

// ───────────────────────────── Entrypoint ─────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("lfetch - Lunula Terminal 3D System Info Fetcher\n");
        println!("USAGE:");
        println!("    lfetch [OPTIONS]\n");
        println!("OPTIONS:");
        println!("    -1, --anim 1     Force Animation 1: 3D Raymarched Moon & Great Wave Breaking Crest (38x15)");
        println!("    -2, --anim 2     Force Animation 2: 3D Analytical Moon with Craters & Rolling Swells (44x17)");
        println!("    -s, --static     Render a single instant frame and exit (< 5ms)");
        println!("    -l, --live       Run interactive animated 3D mode");
        println!("    -h, --help       Print help information\n");
        println!("CONFIGURATION (lunula.lua):");
        println!("    Set default animation & live mode in `lunula.lua`:");
        println!("        fetch = {{");
        println!("            animation = 1, -- 1 or 2");
        println!("            live = true,   -- true or false");
        println!("        }}");
        return;
    }

    // Enables VT colors + UTF-8 code page for the whole run (restored on exit).
    let _out_guard = win32::OutputGuard::new();

    // 1. CLI flag parsing
    let mut cli_anim: Option<u8> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-1" => cli_anim = Some(1),
            "-2" => cli_anim = Some(2),
            "--anim" | "--animation" => {
                if i + 1 < args.len() {
                    if args[i + 1] == "1" {
                        cli_anim = Some(1);
                    } else if args[i + 1] == "2" {
                        cli_anim = Some(2);
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    let cli_static = args
        .iter()
        .any(|a| a == "-s" || a == "--static" || a == "--once");
    let cli_live = args
        .iter()
        .any(|a| a == "-l" || a == "--live" || a == "-a" || a == "--animate");

    // 2. Resolve animation & live mode from lunula.lua + CLI overrides
    let file_cfg = load_fetch_config();
    let active_anim = cli_anim.unwrap_or(file_cfg.animation);
    let is_live = if cli_static {
        false
    } else if cli_live {
        true
    } else {
        file_cfg.live
    };

    // 3. Fast static mode (< 5ms)
    if !is_live {
        let metrics = collect_metrics();
        let info_lines = build_info_lines(&metrics);
        let mut buf = String::with_capacity(8192);
        buf.push('\n');
        let mut frame_buf = String::with_capacity(8192);
        if active_anim == 2 {
            anim2::render_frame(&mut frame_buf, &info_lines, 0.9, false);
        } else {
            anim1::render_frame(&mut frame_buf, &info_lines, 0.0, false);
        }
        buf.push_str(&frame_buf);
        buf.push('\n');
        let mut stdout = io::stdout();
        let _ = stdout.write_all(buf.as_bytes());
        let _ = stdout.flush();
        return;
    }

    // 4. Interactive Live Mode
    win32::install_ctrl_handler();
    let raw_console = win32::RawConsole::init();
    let mut stdout = io::stdout();

    // Hide cursor
    let _ = stdout.write_all(b"\x1b[?25l");
    let _ = stdout.flush();

    let mut metrics = collect_metrics();
    let mut info_lines = build_info_lines(&metrics);
    let mut render_buf = String::with_capacity(16384);

    let total_rows = if active_anim == 2 {
        anim2::TOTAL_ROWS
    } else {
        anim1::CANVAS_H
    };

    // Rewind height: Canvas rows + 1 blank line + 1 status line = total_rows + 2
    let cursor_up = format!("\x1b[{}A\r", total_rows + 2);
    let start = Instant::now();
    let mut last_metric_update = Instant::now();
    let mut first = true;

    let frame_delay_ms = if active_anim == 2 { 50 } else { 42 };

    while !STOP.load(Ordering::Relaxed) {
        // Live update memory & uptime every 1 second
        if last_metric_update.elapsed() >= Duration::from_secs(1) {
            metrics.memory = win32::get_memory_info();
            metrics.uptime_formatted = win32::get_uptime();
            info_lines = build_info_lines(&metrics);
            last_metric_update = Instant::now();
        }

        let t = start.elapsed().as_secs_f32();
        if active_anim == 2 {
            anim2::render_frame(&mut render_buf, &info_lines, t, true);
        } else {
            anim1::render_frame(&mut render_buf, &info_lines, t, true);
        }

        // Wrap each frame in DEC Mode ?2026 Synchronized Output for tear-free animation
        let _ = stdout.write_all(b"\x1b[?2026h");
        if !first {
            let _ = stdout.write_all(cursor_up.as_bytes());
        }
        let _ = stdout.write_all(render_buf.as_bytes());
        let _ = stdout.write_all(b"\x1b[?2026l");
        let _ = stdout.flush();
        first = false;

        // Frame pacing (~20 FPS for anim 2, ~24 FPS for anim 1); any key exits
        if let Some(ref con) = raw_console {
            if con.poll_key(frame_delay_ms) {
                break;
            }
        } else {
            std::thread::sleep(Duration::from_millis(frame_delay_ms as u64));
        }
    }

    // Clean exit: restore cursor, reset colors, newline
    let _ = stdout.write_all(b"\x1b[?25h\x1b[0m\n");
    let _ = stdout.flush();
}
