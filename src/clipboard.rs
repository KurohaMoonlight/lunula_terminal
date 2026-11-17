use std::ffi::c_void;

const CF_UNICODETEXT: u32 = 13;
const GMEM_MOVEABLE: u32 = 0x0002;

#[link(name = "user32")]
unsafe extern "system" {
    fn OpenClipboard(hWndNewOwner: *mut c_void) -> i32;
    fn CloseClipboard() -> i32;
    fn EmptyClipboard() -> i32;
    fn GetClipboardData(uFormat: u32) -> *mut c_void;
    fn SetClipboardData(uFormat: u32, hMem: *mut c_void) -> *mut c_void;
    fn IsClipboardFormatAvailable(uFormat: u32) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalAlloc(uFlags: u32, dwBytes: usize) -> *mut c_void;
    fn GlobalLock(hMem: *mut c_void) -> *mut c_void;
    fn GlobalUnlock(hMem: *mut c_void) -> i32;
}

pub fn get_text() -> Option<String> {
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return None;
        }
        if IsClipboardFormatAvailable(CF_UNICODETEXT) == 0 {
            CloseClipboard();
            return None;
        }
        let handle = GetClipboardData(CF_UNICODETEXT);
        if handle.is_null() {
            CloseClipboard();
            return None;
        }
        let ptr = GlobalLock(handle) as *const u16;
        if ptr.is_null() {
            CloseClipboard();
            return None;
        }
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(ptr, len);
        let result = String::from_utf16_lossy(slice);
        GlobalUnlock(handle);
        CloseClipboard();
        Some(result)
    }
}

pub fn set_text(text: &str) -> bool {
    let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes_len = utf16.len() * std::mem::size_of::<u16>();
    unsafe {
        let h_mem = GlobalAlloc(GMEM_MOVEABLE, bytes_len);
        if h_mem.is_null() {
            return false;
        }
        let dest = GlobalLock(h_mem) as *mut u16;
        if dest.is_null() {
            return false;
        }
        std::ptr::copy_nonoverlapping(utf16.as_ptr(), dest, utf16.len());
        GlobalUnlock(h_mem);

        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return false;
        }
        EmptyClipboard();
        let res = SetClipboardData(CF_UNICODETEXT, h_mem);
        CloseClipboard();
        !res.is_null()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_roundtrip() {
        let test_str = "Lunula Terminal Clipboard Test 123 🚀";
        assert!(set_text(test_str));
        let got = get_text();
        assert_eq!(got, Some(test_str.to_string()));
    }
}
