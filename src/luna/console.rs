//! Native Win32 Console FFI (`kernel32`) for raw keyboard/resize input and alternate screen.

use std::ffi::c_void;
use std::io::{self, Write};

const STD_INPUT_HANDLE: u32 = (-10i32) as u32;
const STD_OUTPUT_HANDLE: u32 = (-11i32) as u32;

const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
const ENABLE_WINDOW_INPUT: u32 = 0x0008;
const ENABLE_PROCESSED_INPUT: u32 = 0x0001;
const ENABLE_LINE_INPUT: u32 = 0x0002;
const ENABLE_ECHO_INPUT: u32 = 0x0004;

const KEY_EVENT: u16 = 0x0001;
const WINDOW_BUFFER_SIZE_EVENT: u16 = 0x0004;

const LEFT_CTRL_PRESSED: u32 = 0x0008;
const RIGHT_CTRL_PRESSED: u32 = 0x0004;

#[repr(C)]
#[derive(Clone, Copy)]
struct COORD {
    x: i16,
    y: i16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SMALL_RECT {
    left: i16,
    top: i16,
    right: i16,
    bottom: i16,
}

#[repr(C)]
#[derive(Clone, Copy)]
#[allow(non_snake_case)]
struct CONSOLE_SCREEN_BUFFER_INFO {
    dwSize: COORD,
    dwCursorPosition: COORD,
    wAttributes: u16,
    srWindow: SMALL_RECT,
    dwMaximumWindowSize: COORD,
}

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
#[allow(non_snake_case)]
struct WINDOW_BUFFER_SIZE_RECORD {
    dwSize: COORD,
}

#[repr(C)]
#[derive(Clone, Copy)]
union EventUnion {
    key_event: KEY_EVENT_RECORD,
    window_buffer_size_event: WINDOW_BUFFER_SIZE_RECORD,
    _pad: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
#[allow(non_snake_case)]
struct INPUT_RECORD {
    EventType: u16,
    Event: EventUnion,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetStdHandle(nStdHandle: u32) -> *mut c_void;
    fn GetConsoleMode(hConsoleHandle: *mut c_void, lpMode: *mut u32) -> i32;
    fn SetConsoleMode(hConsoleHandle: *mut c_void, dwMode: u32) -> i32;
    fn GetConsoleScreenBufferInfo(
        hConsoleOutput: *mut c_void,
        lpConsoleScreenBufferInfo: *mut CONSOLE_SCREEN_BUFFER_INFO,
    ) -> i32;
    fn ReadConsoleInputW(
        hConsoleInput: *mut c_void,
        lpBuffer: *mut INPUT_RECORD,
        nLength: u32,
        lpNumberOfEventsRead: *mut u32,
    ) -> i32;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    Char(char),
    Ctrl(char),
    Enter,
    Backspace,
    Delete,
    Tab,
    Esc,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Resize(usize, usize),
}

pub struct ConsoleGuard {
    h_in: *mut c_void,
    h_out: *mut c_void,
    orig_in_mode: u32,
    orig_out_mode: u32,
}

impl ConsoleGuard {
    pub fn enter() -> io::Result<Self> {
        unsafe {
            let h_in = GetStdHandle(STD_INPUT_HANDLE);
            let h_out = GetStdHandle(STD_OUTPUT_HANDLE);

            let mut orig_in_mode = 0;
            let mut orig_out_mode = 0;
            GetConsoleMode(h_in, &mut orig_in_mode);
            GetConsoleMode(h_out, &mut orig_out_mode);

            // Raw input: disable line buffering, echo, and Ctrl+C/Ctrl+Z signal processing
            let new_in_mode =
                (orig_in_mode & !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT))
                    | ENABLE_WINDOW_INPUT;
            SetConsoleMode(h_in, new_in_mode);

            // Ensure VT ANSI output processing is enabled
            SetConsoleMode(h_out, orig_out_mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);

            let mut stdout = io::stdout();
            stdout.write_all(b"\x1b[?1049h\x1b[?25h")?;
            stdout.flush()?;

            Ok(Self {
                h_in,
                h_out,
                orig_in_mode,
                orig_out_mode,
            })
        }
    }

    pub fn size(&self) -> (usize, usize) {
        unsafe {
            let mut info = std::mem::zeroed::<CONSOLE_SCREEN_BUFFER_INFO>();
            if GetConsoleScreenBufferInfo(self.h_out, &mut info) != 0 {
                let cols = (info.srWindow.right - info.srWindow.left + 1).max(20) as usize;
                let rows = (info.srWindow.bottom - info.srWindow.top + 1).max(6) as usize;
                (cols, rows)
            } else {
                (80, 24)
            }
        }
    }

    pub fn read_event(&self) -> io::Result<InputEvent> {
        let mut record = unsafe { std::mem::zeroed::<INPUT_RECORD>() };
        let mut read = 0u32;

        loop {
            let ok = unsafe { ReadConsoleInputW(self.h_in, &mut record, 1, &mut read) };
            if ok == 0 || read == 0 {
                return Err(io::Error::last_os_error());
            }

            if record.EventType == WINDOW_BUFFER_SIZE_EVENT {
                let (cols, rows) = self.size();
                return Ok(InputEvent::Resize(cols, rows));
            }

            if record.EventType == KEY_EVENT {
                let ke = unsafe { record.Event.key_event };
                if ke.bKeyDown == 0 {
                    continue;
                }

                let ctrl = (ke.dwControlKeyState & (LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED)) != 0;
                let vk = ke.wVirtualKeyCode;

                // Map virtual keys first
                match vk {
                    0x26 => return Ok(InputEvent::Up),
                    0x28 => return Ok(InputEvent::Down),
                    0x25 => return Ok(InputEvent::Left),
                    0x27 => return Ok(InputEvent::Right),
                    0x24 => return Ok(InputEvent::Home),
                    0x23 => return Ok(InputEvent::End),
                    0x21 => return Ok(InputEvent::PageUp),
                    0x22 => return Ok(InputEvent::PageDown),
                    0x2E => return Ok(InputEvent::Delete),
                    0x08 => return Ok(InputEvent::Backspace),
                    0x0D => return Ok(InputEvent::Enter),
                    0x09 => return Ok(InputEvent::Tab),
                    0x1B => return Ok(InputEvent::Esc),
                    _ => {}
                }

                if ctrl && (0x41..=0x5A).contains(&vk) {
                    let ch = (vk as u8 - 0x41 + b'a') as char;
                    return Ok(InputEvent::Ctrl(ch));
                }

                let uc = ke.uChar;
                if uc != 0 {
                    if let Some(ch) = char::from_u32(uc as u32) {
                        if ctrl && (ch as u32) >= 1 && (ch as u32) <= 26 {
                            let letter = ((ch as u8) - 1 + b'a') as char;
                            return Ok(InputEvent::Ctrl(letter));
                        }
                        if !ch.is_control() {
                            return Ok(InputEvent::Char(ch));
                        }
                    }
                }
            }
        }
    }
}

impl Drop for ConsoleGuard {
    fn drop(&mut self) {
        unsafe {
            let mut stdout = io::stdout();
            let _ = stdout.write_all(b"\x1b[0m\x1b[?25h\x1b[?1049l");
            let _ = stdout.flush();
            SetConsoleMode(self.h_in, self.orig_in_mode);
            SetConsoleMode(self.h_out, self.orig_out_mode);
        }
    }
}
