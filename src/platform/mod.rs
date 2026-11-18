//! Platform-specific utilities (Win32 FFI for cursor position, system clock, and Git branch lookup).

use std::path::Path;

#[repr(C)]
struct POINT {
    x: i32,
    y: i32,
}

#[repr(C)]
#[allow(non_snake_case)]
struct SYSTEMTIME {
    wYear: u16,
    wMonth: u16,
    wDayOfWeek: u16,
    wDay: u16,
    wHour: u16,
    wMinute: u16,
    wSecond: u16,
    wMilliseconds: u16,
}

unsafe extern "system" {
    fn GetCursorPos(lp: *mut POINT) -> i32;
    fn GetLocalTime(lpSystemTime: *mut SYSTEMTIME);
    fn GetCurrentProcess() -> isize;
    fn SetProcessWorkingSetSize(
        hProcess: isize,
        dwMinimumWorkingSetSize: usize,
        dwMaximumWorkingSetSize: usize,
    ) -> i32;
}

pub mod clipboard;

/// Trims the process working set, freeing pages left behind by startup initialization
/// (such as shader translation ASTs, system font directory indexing, and driver initialization).
#[inline]
pub fn trim_working_set() {
    unsafe {
        SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

/// Returns the current physical desktop cursor coordinates `(x, y)` via Win32 `GetCursorPos`.
#[inline]
pub fn screen_cursor() -> (i32, i32) {
    let mut pt = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut pt) };
    (pt.x, pt.y)
}

/// Returns the current local time formatted as `"HH:MM"` via Win32 `GetLocalTime`.
pub fn get_local_time_hm() -> String {
    let mut st = SYSTEMTIME {
        wYear: 0,
        wMonth: 0,
        wDayOfWeek: 0,
        wDay: 0,
        wHour: 0,
        wMinute: 0,
        wSecond: 0,
        wMilliseconds: 0,
    };
    unsafe { GetLocalTime(&mut st) };
    format!("{:02}:{:02}", st.wHour, st.wMinute)
}

/// Resolves the active Git branch (or short commit hash) by walking upward from `start_path`.
pub fn get_git_branch_for_path(start_path: &Path) -> Option<String> {
    let mut dir = if start_path.is_file() {
        start_path.parent()?.to_path_buf()
    } else {
        start_path.to_path_buf()
    };
    loop {
        let git_head = dir.join(".git").join("HEAD");
        if Path::new(&git_head).exists() {
            if let Ok(content) = std::fs::read_to_string(git_head) {
                let trimmed = content.trim();
                if let Some(branch) = trimmed.strip_prefix("ref: refs/heads/") {
                    return Some(branch.to_string());
                } else if trimmed.len() >= 7 {
                    return Some(trimmed[..7].to_string());
                }
            }
            return None;
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// Resolves the active Git branch (or short commit hash) by walking upward from `current_dir`.
pub fn get_git_branch() -> Option<String> {
    let dir = std::env::current_dir().ok()?;
    get_git_branch_for_path(&dir)
}
