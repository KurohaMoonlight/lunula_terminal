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

#[cfg(windows)]
#[link(name = "shell32")]
unsafe extern "system" {
    fn ShellExecuteW(
        hwnd: *mut std::ffi::c_void,
        lpOperation: *const u16,
        lpFile: *const u16,
        lpParameters: *const u16,
        lpDirectory: *const u16,
        nShowCmd: i32,
    ) -> isize;
}

/// Validates whether a URL has a safe web scheme (`http://` or `https://`) and contains no ASCII control characters or whitespace.
pub fn is_safe_url(url: &str) -> bool {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.chars().any(|c| c.is_ascii_control() || c.is_ascii_whitespace()) {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("http://") {
        !rest.is_empty()
    } else if let Some(rest) = lower.strip_prefix("https://") {
        !rest.is_empty()
    } else {
        false
    }
}

/// Safely opens a validated `http://` or `https://` URL directly via Win32 `ShellExecuteW`.
///
/// Bypasses `cmd.exe` completely to prevent command injection vulnerabilities from shell
/// metacharacters (`&`, `|`, `^`, `%`) or dangerous URI schemes (`file://`, `ms-msdt:`),
/// and spawns the API call onto a worker thread so the main UI render loop is never blocked.
pub fn open_url_safely(url: &str) -> bool {
    if !is_safe_url(url) {
        log::warn!("Rejected opening unsafe or unsupported hyperlink URL: {}", url);
        return false;
    }

    #[cfg(windows)]
    {
        let url_owned = url.trim().to_string();
        std::thread::spawn(move || {
            let wide_url: Vec<u16> = url_owned.encode_utf16().chain(std::iter::once(0)).collect();
            let wide_op: Vec<u16> = "open".encode_utf16().chain(std::iter::once(0)).collect();

            let ret = unsafe {
                ShellExecuteW(
                    std::ptr::null_mut(),
                    wide_op.as_ptr(),
                    wide_url.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    1, // SW_SHOWNORMAL
                )
            };
            if ret <= 32 {
                log::warn!("ShellExecuteW failed to open URL (code: {})", ret);
            }
        });
        true
    }

    #[cfg(not(windows))]
    {
        false
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_safe_url() {
        assert!(is_safe_url("http://localhost:8080"));
        assert!(is_safe_url("https://github.com/lunula-terminal"));
        assert!(is_safe_url("https://example.com/path?foo=bar&baz=qux#heading"));

        // Reject dangerous / non-http schemes
        assert!(!is_safe_url("file:///C:/Windows/System32/cmd.exe"));
        assert!(!is_safe_url("cmd.exe /c calc"));
        assert!(!is_safe_url("powershell.exe -Command calc"));
        assert!(!is_safe_url("ftp://example.com"));
        assert!(!is_safe_url("javascript:alert(1)"));
        assert!(!is_safe_url("ms-msdt:/id"));

        // Reject incomplete schemes
        assert!(!is_safe_url("http://"));
        assert!(!is_safe_url("https://"));
        assert!(!is_safe_url(""));

        // Reject URLs with whitespace / shell injection attempts
        assert!(!is_safe_url("https://example.com & calc.exe"));
        assert!(!is_safe_url("https://example.com | calc.exe"));
        assert!(!is_safe_url("https://example.com\x00calc.exe"));
        assert!(!is_safe_url("https://example.com\ncalc.exe"));
    }
}
