//! `limg` - High-performance built-in image viewer for Lunula Terminal.
//!
//! Supports JPEG, PNG, BMP, GIF, TIFF, and ICO with zero extra crates
//! by utilizing hardware-accelerated Windows GDI+ decoding and the Kitty Graphics Protocol.
//!
//! Usage:
//!   limg <path/to/image.jpg|png|bmp> [-c <cols>] [-r <rows>] [--clear]

use std::env;
use std::ffi::c_void;
use std::io::{self, Write};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

#[repr(C)]
struct GdiplusStartupInput {
    version: u32,
    debug_event_callback: usize,
    suppress_background_thread: i32,
    suppress_external_codecs: i32,
}

#[repr(C)]
struct GdiplusRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

#[repr(C)]
struct BitmapData {
    width: u32,
    height: u32,
    stride: i32,
    pixel_format: i32,
    scan0: *mut u8,
    reserved: usize,
}

const PIXEL_FORMAT_32BPP_ARGB: i32 = 0x26200A;
const IMAGE_LOCK_MODE_READ: u32 = 1;

#[link(name = "gdiplus")]
unsafe extern "system" {
    fn GdiplusStartup(token: *mut usize, input: *const GdiplusStartupInput, output: *mut usize) -> i32;
    fn GdiplusShutdown(token: usize);
    fn GdipLoadImageFromFile(filename: *const u16, image: *mut *mut c_void) -> i32;
    fn GdipGetImageWidth(image: *mut c_void, width: *mut u32) -> i32;
    fn GdipGetImageHeight(image: *mut c_void, height: *mut u32) -> i32;
    fn GdipCreateBitmapFromScan0(
        width: i32,
        height: i32,
        stride: i32,
        format: i32,
        scan0: *mut u8,
        bitmap: *mut *mut c_void,
    ) -> i32;
    fn GdipGetImageGraphicsContext(image: *mut c_void, graphics: *mut *mut c_void) -> i32;
    fn GdipSetInterpolationMode(graphics: *mut c_void, mode: i32) -> i32;
    fn GdipDrawImageRectRectI(
        graphics: *mut c_void,
        image: *mut c_void,
        dstx: i32, dsty: i32, dstwidth: i32, dstheight: i32,
        srcx: i32, srcy: i32, srcwidth: i32, srcheight: i32,
        srcUnit: i32, imageAttributes: *mut c_void,
        callback: usize, callbackData: usize,
    ) -> i32;
    fn GdipDeleteGraphics(graphics: *mut c_void) -> i32;
    fn GdipBitmapLockBits(
        bitmap: *mut c_void,
        rect: *const GdiplusRect,
        flags: u32,
        format: i32,
        data: *mut BitmapData,
    ) -> i32;
    fn GdipBitmapUnlockBits(bitmap: *mut c_void, data: *mut BitmapData) -> i32;
    fn GdipDisposeImage(image: *mut c_void) -> i32;
}

/// Decodes any image supported by Windows (JPEG, PNG, BMP, GIF, etc.) into 32-bit RGBA.
/// Automatically downsamples large high-res camera photos to `max_dim` for instant terminal transmission.
fn load_image_rgba(path: &Path, max_dim: u32) -> Result<(u32, u32, Vec<u8>), String> {
    let mut token = 0usize;
    let input = GdiplusStartupInput {
        version: 1,
        debug_event_callback: 0,
        suppress_background_thread: 0,
        suppress_external_codecs: 0,
    };

    unsafe {
        let status = GdiplusStartup(&mut token, &input, std::ptr::null_mut());
        if status != 0 {
            return Err("Failed to initialize Windows GDI+".into());
        }

        let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut image: *mut c_void = std::ptr::null_mut();
        let status = GdipLoadImageFromFile(wide_path.as_ptr(), &mut image);
        if status != 0 || image.is_null() {
            GdiplusShutdown(token);
            return Err(format!("Could not load image (error code: {})", status));
        }

        let mut orig_w = 0u32;
        let mut orig_h = 0u32;
        GdipGetImageWidth(image, &mut orig_w);
        GdipGetImageHeight(image, &mut orig_h);

        if orig_w == 0 || orig_h == 0 {
            GdipDisposeImage(image);
            GdiplusShutdown(token);
            return Err("Image has invalid dimensions (0x0)".into());
        }

        // Calculate aspect-ratio-preserving scaled dimensions
        let (target_w, target_h) = if orig_w > max_dim || orig_h > max_dim {
            let scale = (max_dim as f32 / orig_w as f32).min(max_dim as f32 / orig_h as f32);
            (
                ((orig_w as f32 * scale).round() as u32).max(1),
                ((orig_h as f32 * scale).round() as u32).max(1),
            )
        } else {
            (orig_w, orig_h)
        };

        // If downsampling needed, render with high quality bicubic interpolation
        let (final_bitmap, needs_dispose) = if target_w != orig_w || target_h != orig_h {
            let mut scaled: *mut c_void = std::ptr::null_mut();
            GdipCreateBitmapFromScan0(
                target_w as i32,
                target_h as i32,
                0,
                PIXEL_FORMAT_32BPP_ARGB,
                std::ptr::null_mut(),
                &mut scaled,
            );
            let mut gfx: *mut c_void = std::ptr::null_mut();
            GdipGetImageGraphicsContext(scaled, &mut gfx);
            // 7 = HighQualityBicubic
            GdipSetInterpolationMode(gfx, 7);
            GdipDrawImageRectRectI(
                gfx,
                image,
                0,
                0,
                target_w as i32,
                target_h as i32,
                0,
                0,
                orig_w as i32,
                orig_h as i32,
                2, // UnitPixel = 2
                std::ptr::null_mut(),
                0,
                0,
            );
            GdipDeleteGraphics(gfx);
            GdipDisposeImage(image);
            (scaled, true)
        } else {
            (image, false)
        };

        let rect = GdiplusRect {
            x: 0,
            y: 0,
            width: target_w as i32,
            height: target_h as i32,
        };
        let mut data = BitmapData {
            width: 0,
            height: 0,
            stride: 0,
            pixel_format: 0,
            scan0: std::ptr::null_mut(),
            reserved: 0,
        };

        let status = GdipBitmapLockBits(
            final_bitmap,
            &rect,
            IMAGE_LOCK_MODE_READ,
            PIXEL_FORMAT_32BPP_ARGB,
            &mut data,
        );

        if status != 0 || data.scan0.is_null() {
            if needs_dispose {
                GdipDisposeImage(final_bitmap);
            }
            GdiplusShutdown(token);
            return Err("Failed to lock bitmap bits".into());
        }

        let stride = data.stride.abs() as usize;
        let mut rgba = Vec::with_capacity((target_w * target_h * 4) as usize);

        for row in 0..target_h as usize {
            let row_ptr = data.scan0.add(row * stride);
            for col in 0..target_w as usize {
                let px_ptr = row_ptr.add(col * 4);
                let b = *px_ptr;
                let g = *px_ptr.add(1);
                let r = *px_ptr.add(2);
                let a = *px_ptr.add(3);
                rgba.extend_from_slice(&[r, g, b, a]);
            }
        }

        GdipBitmapUnlockBits(final_bitmap, &mut data);
        GdipDisposeImage(final_bitmap);
        GdiplusShutdown(token);

        Ok((target_w, target_h, rgba))
    }
}

fn print_help() {
    println!(
        "\x1b[1;36mlimg\x1b[0m - Built-in Image Viewer for Lunula Terminal\n\n\
         \x1b[1mUSAGE:\x1b[0m\n    \
           limg <image_path> [options]\n    \
           limg --clear\n\n\
         \x1b[1mOPTIONS:\x1b[0m\n    \
           -c, --cols <num>   Display width in terminal columns (default: 40)\n    \
           -r, --rows <num>   Display height in terminal rows (default: auto proportional)\n    \
           -C, --clear        Clear all images currently on the terminal screen\n    \
           -h, --help         Show this help message\n\n\
         \x1b[1mSUPPORTED FORMATS:\x1b[0m\n    \
           JPEG, PNG, BMP, GIF, TIFF, ICO\n\n\
         \x1b[1mEXAMPLES:\x1b[0m\n    \
           limg photo.jpg\n    \
           limg 'foto bali.jpg' -c 30\n    \
           limg -C\n"
    );
}

#[repr(C)]
struct Coord {
    x: i16,
    y: i16,
}

#[repr(C)]
struct SmallRect {
    left: i16,
    top: i16,
    right: i16,
    bottom: i16,
}

#[repr(C)]
struct ConsoleScreenBufferInfo {
    size: Coord,
    cursor_position: Coord,
    attributes: u16,
    window: SmallRect,
    maximum_window_size: Coord,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetStdHandle(nStdHandle: i32) -> *mut c_void;
    fn GetConsoleScreenBufferInfo(
        hConsoleOutput: *mut c_void,
        lpConsoleScreenBufferInfo: *mut ConsoleScreenBufferInfo,
    ) -> i32;
}

fn get_terminal_cols() -> usize {
    unsafe {
        let handle = GetStdHandle(-11); // STD_OUTPUT_HANDLE
        let mut csbi: ConsoleScreenBufferInfo = std::mem::zeroed();
        if GetConsoleScreenBufferInfo(handle, &mut csbi) != 0 {
            let width = (csbi.window.right - csbi.window.left + 1) as usize;
            if width >= 20 {
                return width;
            }
        }
    }
    80
}

fn clear_images() {
    let payload = b"\x1b_Ga=d,d=a;\x1b\\";
    if let Ok(port_str) = env::var("LUNULA_IPC_PORT") {
        if let Ok(port) = port_str.parse::<u16>() {
            if let Ok(mut stream) = std::net::TcpStream::connect(("127.0.0.1", port)) {
                let _ = stream.write_all(payload);
                let _ = stream.flush();
                return;
            }
        }
    }
    let _ = io::stdout().write_all(payload);
    let _ = io::stdout().flush();
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() * 4 + 2) / 3);
    let mut chunks = bytes.chunks_exact(3);

    for chunk in &mut chunks {
        let b0 = chunk[0] as usize;
        let b1 = chunk[1] as usize;
        let b2 = chunk[2] as usize;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(triple >> 18) & 0x3F] as char);
        out.push(TABLE[(triple >> 12) & 0x3F] as char);
        out.push(TABLE[(triple >> 6) & 0x3F] as char);
        out.push(TABLE[triple & 0x3F] as char);
    }

    let rem = chunks.remainder();
    match rem.len() {
        1 => {
            let b0 = rem[0] as usize;
            let triple = b0 << 16;
            out.push(TABLE[(triple >> 18) & 0x3F] as char);
            out.push(TABLE[(triple >> 12) & 0x3F] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let b0 = rem[0] as usize;
            let b1 = rem[1] as usize;
            let triple = (b0 << 16) | (b1 << 8);
            out.push(TABLE[(triple >> 18) & 0x3F] as char);
            out.push(TABLE[(triple >> 12) & 0x3F] as char);
            out.push(TABLE[(triple >> 6) & 0x3F] as char);
            out.push('=');
        }
        _ => {}
    }

    out
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_help();
        return;
    }

    let mut path: Option<String> = None;
    let mut cols: Option<usize> = None;
    let mut rows: Option<usize> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_help();
                return;
            }
            "-C" | "--clear" => {
                clear_images();
                println!("\x1b[32m✔ Cleared all images from terminal.\x1b[0m");
                return;
            }
            "-c" | "--cols" => {
                if i + 1 < args.len() {
                    cols = args[i + 1].parse().ok();
                    i += 1;
                }
            }
            "-r" | "--rows" => {
                if i + 1 < args.len() {
                    rows = args[i + 1].parse().ok();
                    i += 1;
                }
            }
            arg if !arg.starts_with('-') && path.is_none() => {
                path = Some(arg.to_string());
            }
            _ => {}
        }
        i += 1;
    }

    let Some(image_path_str) = path else {
        eprintln!("\x1b[31mError: No image path provided.\x1b[0m");
        print_help();
        return;
    };

    let file_path = Path::new(&image_path_str);
    if !file_path.exists() {
        eprintln!("\x1b[31mError: File '{}' not found.\x1b[0m", image_path_str);
        return;
    }

    // Default column width: fit terminal nicely (up to 60 cols)
    let term_cols = get_terminal_cols();
    let display_cols = cols.unwrap_or_else(|| (term_cols.saturating_sub(4)).clamp(30, 60));
    // Downsample resolution proportional to display size (up to 960px)
    let max_render_dim = (display_cols as u32 * 24).clamp(400, 960);

    let (target_w, target_h, rgba_data) = match load_image_rgba(file_path, max_render_dim) {
        Ok(res) => res,
        Err(e) => {
            eprintln!("\x1b[31mError decoding '{}': {}\x1b[0m", image_path_str, e);
            return;
        }
    };

    // Calculate aspect-ratio-preserving rows if not specified
    // Standard terminal cell aspect ratio is ~1:2 (width:height)
    let display_rows = rows.unwrap_or_else(|| {
        let cell_aspect = 0.5f32; // cell width / cell height
        let r = (display_cols as f32 * (target_h as f32 / target_w as f32) * cell_aspect).round();
        (r as usize).max(1)
    });

    let b64 = encode_base64(&rgba_data);
    let ctrl_str = format!(
        "a=t,f=32,s={},v={},c={},r={}",
        target_w, target_h, display_cols, display_rows
    );

    let chunk_size = 4096;
    let b64_bytes = b64.as_bytes();

    let payload = if b64_bytes.len() <= chunk_size {
        format!("\x1b_G{},m=0;{}\x1b\\", ctrl_str, b64)
    } else {
        let mut full = String::new();
        let chunks: Vec<&[u8]> = b64_bytes.chunks(chunk_size).collect();
        for (idx, chunk) in chunks.iter().enumerate() {
            let is_last = idx == chunks.len() - 1;
            let m = if is_last { 0 } else { 1 };
            let chunk_str = std::str::from_utf8(chunk).unwrap_or("");
            if idx == 0 {
                full.push_str(&format!("\x1b_G{},m={};{}\x1b\\", ctrl_str, m, chunk_str));
            } else {
                full.push_str(&format!("\x1b_Gm={};{}\x1b\\", m, chunk_str));
            }
        }
        full
    };

    let mut delivered_via_ipc = false;
    if let Ok(port_str) = env::var("LUNULA_IPC_PORT") {
        if let Ok(port) = port_str.parse::<u16>() {
            if let Ok(mut stream) = std::net::TcpStream::connect(("127.0.0.1", port)) {
                if stream.write_all(payload.as_bytes()).is_ok() && stream.flush().is_ok() {
                    delivered_via_ipc = true;
                }
            }
        }
    }

    if !delivered_via_ipc {
        // Fallback for native Kitty terminals on Linux/macOS
        let _ = io::stdout().write_all(payload.as_bytes());
        let _ = io::stdout().flush();
    }

    // Advance stdout by display_rows lines so the shell prompt is placed cleanly below the image
    for _ in 0..display_rows {
        println!();
    }
    let _ = io::stdout().flush();
}
