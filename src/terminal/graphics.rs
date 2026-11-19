//! Kitty Graphics Protocol decoder and image manager.
//!
//! Supports APC/DCS sequences `\x1b_G...;\x1b\` and `\x1bPG...;\x1b\` with:
//! - action `a`: transmit (`t`), transmit without moving cursor (`T`), query, delete (`d`)
//! - format `f`: 32 (RGBA), 24 (RGB), 100 (PNG)
//! - chunking `m`: 1 (more chunks), 0 (final chunk)
//! - zero external dependencies for PNG decoding using `yazi::decompress` (deflate/zlib)

use std::collections::HashMap;

/// An image placed within the terminal grid.
#[derive(Debug, Clone)]
pub struct KittyImage {
    /// Unique identifier for this image.
    pub id: u32,
    /// Placement ID.
    pub placement_id: u32,
    /// Column in terminal grid where top-left corner is positioned.
    pub col: usize,
    /// Absolute row in scrollback + screen coordinate space.
    pub abs_row: usize,
    /// Display width in terminal cells.
    pub width_cells: usize,
    /// Display height in terminal cells.
    pub height_cells: usize,
    /// Native pixel width.
    pub pixel_width: u32,
    /// Native pixel height.
    pub pixel_height: u32,
    /// Decoded 32-bit RGBA pixels (length = pixel_width * pixel_height * 4).
    pub rgba_data: Vec<u8>,
    /// Set to true when uploaded or modified, cleared after GPU texture creation.
    pub dirty: std::cell::Cell<bool>,
}

/// Parameters extracted from the Kitty Graphics control keys.
#[derive(Debug, Clone, Default)]
pub struct KittyControlParams {
    pub action: char,        // 'a': 't' (transmit/display), 'T', 'q', 'p', 'd'
    pub format: u32,         // 'f': 32 (RGBA), 24 (RGB), 100 (PNG)
    pub pixel_width: u32,    // 's'
    pub pixel_height: u32,   // 'v'
    pub image_id: u32,       // 'i'
    pub placement_id: u32,   // 'p'
    pub cols: usize,         // 'c'
    pub rows: usize,         // 'r'
    pub more_chunks: bool,   // 'm': 1 = more chunks follow, 0 = last chunk
    pub quiet: u8,           // 'q': 0 = normal, 1 = suppress OK, 2 = suppress all
    pub delete_action: char, // 'd': 'a' (all), 'i' (by id), 'p' (placement), 'c' (cursor)
}

/// Accumulator for multi-chunk image transmissions.
#[derive(Debug, Default)]
pub struct KittyGraphicsManager {
    /// Active images rendered on grid.
    pub images: Vec<KittyImage>,
    /// Pending chunk buffers keyed by image_id (or 0 for anonymous).
    pub pending_transmissions: HashMap<u32, (KittyControlParams, Vec<u8>)>,
    /// Next automatic image id if none specified.
    next_auto_id: u32,
}

impl KittyGraphicsManager {
    pub fn new() -> Self {
        Self {
            images: Vec::new(),
            pending_transmissions: HashMap::new(),
            next_auto_id: 1,
        }
    }

    /// Handles a Kitty Graphics payload: `<control_data>;<payload_base64>`.
    ///
    /// Returns an optional terminal response (e.g. `\x1b_Gi={id};OK\x1b\`).
    pub fn handle_kitty_sequence(
        &mut self,
        payload_bytes: &[u8],
        cursor_x: usize,
        cursor_y: usize,
        scrollback_len: usize,
    ) -> Option<String> {
        // Format: [control_keys];[payload_base64]
        let semi_pos = payload_bytes.iter().position(|&b| b == b';');
        let (ctrl_bytes, data_bytes) = match semi_pos {
            Some(idx) => (&payload_bytes[..idx], &payload_bytes[idx + 1..]),
            None => (payload_bytes, &[][..]),
        };

        let ctrl_str = std::str::from_utf8(ctrl_bytes).unwrap_or("");
        let params = parse_kitty_controls(ctrl_str);

        // Handle delete actions immediately
        if params.action == 'd' || params.delete_action != '\0' {
            self.handle_delete(&params, cursor_x, cursor_y, scrollback_len);
            if params.quiet == 0 {
                return Some(format!("\x1b_Gi={};OK\x1b\\", params.image_id));
            }
            return None;
        }

        let transmission_id = if params.image_id != 0 {
            params.image_id
        } else {
            0
        };

        // Append to multi-chunk transmission
        let (merged_params, complete_base64) = if params.more_chunks {
            let entry = self
                .pending_transmissions
                .entry(transmission_id)
                .or_insert_with(|| (params.clone(), Vec::new()));
            entry.1.extend_from_slice(data_bytes);
            return None; // Waiting for more chunks
        } else if let Some((mut prev_params, mut prev_data)) =
            self.pending_transmissions.remove(&transmission_id)
        {
            prev_data.extend_from_slice(data_bytes);
            // Overwrite any updated parameters from last chunk
            if params.cols != 0 {
                prev_params.cols = params.cols;
            }
            if params.rows != 0 {
                prev_params.rows = params.rows;
            }
            if params.pixel_width != 0 {
                prev_params.pixel_width = params.pixel_width;
            }
            if params.pixel_height != 0 {
                prev_params.pixel_height = params.pixel_height;
            }
            (prev_params, prev_data)
        } else {
            (params, data_bytes.to_vec())
        };

        // Decode Base64 data
        let raw_bytes = match decode_base64(&complete_base64) {
            Some(bytes) if !bytes.is_empty() => bytes,
            _ => {
                if merged_params.quiet < 2 {
                    return Some(format!(
                        "\x1b_Gi={};EINVAL:base64_decode_failed\x1b\\",
                        merged_params.image_id
                    ));
                }
                return None;
            }
        };

        // Decode image format (PNG, RGBA32, RGB24)
        let (pixel_w, pixel_h, rgba_data) = match merged_params.format {
            100 => {
                // PNG image
                match decode_png(&raw_bytes) {
                    Some(decoded) => decoded,
                    None => {
                        if merged_params.quiet < 2 {
                            return Some(format!(
                                "\x1b_Gi={};EINVAL:png_decode_failed\x1b\\",
                                merged_params.image_id
                            ));
                        }
                        return None;
                    }
                }
            }
            24 => {
                // 24-bit RGB
                let w = merged_params.pixel_width;
                let h = merged_params.pixel_height;
                if w == 0 || h == 0 || raw_bytes.len() < (w * h * 3) as usize {
                    return None;
                }
                let mut rgba = Vec::with_capacity((w * h * 4) as usize);
                for chunk in raw_bytes.chunks_exact(3) {
                    rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
                }
                (w, h, rgba)
            }
            _ => {
                // 32-bit RGBA (default)
                let w = merged_params.pixel_width;
                let h = merged_params.pixel_height;
                if w == 0 || h == 0 || raw_bytes.len() < (w * h * 4) as usize {
                    return None;
                }
                (w, h, raw_bytes)
            }
        };

        let image_id = if merged_params.image_id != 0 {
            merged_params.image_id
        } else {
            let id = self.next_auto_id;
            self.next_auto_id += 1;
            id
        };

        // Cell dimensions: if omitted, estimate using standard cell aspect ratio ~10x20
        let cols = if merged_params.cols > 0 {
            merged_params.cols
        } else {
            ((pixel_w + 9) / 10).max(1) as usize
        };
        let rows = if merged_params.rows > 0 {
            merged_params.rows
        } else {
            ((pixel_h + 19) / 20).max(1) as usize
        };

        let abs_row = scrollback_len + cursor_y;

        // If replacing an existing image with the same id and placement_id, remove old
        self.images.retain(|img| {
            !(img.id == image_id && img.placement_id == merged_params.placement_id)
        });

        // Add image to active images list
        self.images.push(KittyImage {
            id: image_id,
            placement_id: merged_params.placement_id,
            col: cursor_x,
            abs_row,
            width_cells: cols,
            height_cells: rows,
            pixel_width: pixel_w,
            pixel_height: pixel_h,
            rgba_data,
            dirty: std::cell::Cell::new(true),
        });

        if merged_params.quiet == 0 {
            Some(format!("\x1b_Gi={};OK\x1b\\", image_id))
        } else {
            None
        }
    }

    fn handle_delete(
        &mut self,
        params: &KittyControlParams,
        cursor_x: usize,
        cursor_y: usize,
        scrollback_len: usize,
    ) {
        match params.delete_action {
            'a' | 'A' => {
                // Delete all images
                self.images.clear();
            }
            'i' | 'I' => {
                // Delete by image id
                if params.image_id != 0 {
                    self.images.retain(|img| img.id != params.image_id);
                }
            }
            'p' | 'P' => {
                // Delete by placement id
                self.images
                    .retain(|img| img.placement_id != params.placement_id);
            }
            'c' | 'C' => {
                // Delete images intersecting cursor cell
                let abs_row = scrollback_len + cursor_y;
                self.images.retain(|img| {
                    let col_match = cursor_x >= img.col && cursor_x < img.col + img.width_cells;
                    let row_match = abs_row >= img.abs_row && abs_row < img.abs_row + img.height_cells;
                    !(col_match && row_match)
                });
            }
            _ => {
                // Default deletion if image_id specified
                if params.image_id != 0 {
                    self.images.retain(|img| img.id != params.image_id);
                } else {
                    self.images.clear();
                }
            }
        }
    }
}

/// Parses key=value pairs from the control portion of a Kitty Graphics escape sequence.
pub fn parse_kitty_controls(s: &str) -> KittyControlParams {
    let mut params = KittyControlParams {
        action: 't',   // default action: transmit and display
        format: 32,    // default format: RGBA 32-bit
        ..Default::default()
    };

    for pair in s.split(',') {
        let mut it = pair.splitn(2, '=');
        let key = it.next().unwrap_or("").trim();
        let val = it.next().unwrap_or("").trim();

        match key {
            "a" => {
                if let Some(ch) = val.chars().next() {
                    params.action = ch;
                }
            }
            "f" => {
                if let Ok(num) = val.parse::<u32>() {
                    params.format = num;
                }
            }
            "s" => {
                if let Ok(num) = val.parse::<u32>() {
                    params.pixel_width = num;
                }
            }
            "v" => {
                if let Ok(num) = val.parse::<u32>() {
                    params.pixel_height = num;
                }
            }
            "i" => {
                if let Ok(num) = val.parse::<u32>() {
                    params.image_id = num;
                }
            }
            "p" => {
                if let Ok(num) = val.parse::<u32>() {
                    params.placement_id = num;
                }
            }
            "c" => {
                if let Ok(num) = val.parse::<usize>() {
                    params.cols = num;
                }
            }
            "r" => {
                if let Ok(num) = val.parse::<usize>() {
                    params.rows = num;
                }
            }
            "m" => {
                params.more_chunks = val == "1";
            }
            "q" => {
                if let Ok(num) = val.parse::<u8>() {
                    params.quiet = num;
                }
            }
            "d" => {
                if let Some(ch) = val.chars().next() {
                    params.delete_action = ch;
                }
            }
            _ => {}
        }
    }

    params
}

/// Robust zero-dependency Base64 decoder.
pub fn decode_base64(input: &[u8]) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0;

    for &b in input {
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b' ' | b'\t' | b'\r' | b'\n' => continue,
            _ => return None,
        };
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buf >> bits) as u8);
        }
    }
    Some(output)
}

/// Self-contained PNG decoder using `yazi::decompress` for RFC 1950 zlib stream.
///
/// Returns `(width, height, rgba_data)`.
pub fn decode_png(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    // 1. Verify 8-byte PNG header: \x89PNG\r\n\x1a\n
    const PNG_HEADER: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    if bytes.len() < 8 || &bytes[0..8] != &PNG_HEADER {
        return None;
    }

    let mut offset = 8;
    let mut width = 0u32;
    let mut height = 0u32;
    let mut bit_depth = 8u8;
    let mut color_type = 6u8; // 6 = RGBA
    let mut palette = Vec::<[u8; 3]>::new();
    let mut idat_data = Vec::new();

    // 2. Iterate PNG chunks
    while offset + 8 <= bytes.len() {
        let length = u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        let chunk_type = &bytes[offset + 4..offset + 8];
        let data_start = offset + 8;
        let data_end = data_start + length;

        if data_end + 4 > bytes.len() {
            break;
        }

        let chunk_data = &bytes[data_start..data_end];

        match chunk_type {
            b"IHDR" => {
                if chunk_data.len() >= 13 {
                    width = u32::from_be_bytes([
                        chunk_data[0],
                        chunk_data[1],
                        chunk_data[2],
                        chunk_data[3],
                    ]);
                    height = u32::from_be_bytes([
                        chunk_data[4],
                        chunk_data[5],
                        chunk_data[6],
                        chunk_data[7],
                    ]);
                    bit_depth = chunk_data[8];
                    color_type = chunk_data[9];
                }
            }
            b"PLTE" => {
                for rgb in chunk_data.chunks_exact(3) {
                    palette.push([rgb[0], rgb[1], rgb[2]]);
                }
            }
            b"IDAT" => {
                idat_data.extend_from_slice(chunk_data);
            }
            b"IEND" => break,
            _ => {}
        }

        offset = data_end + 4; // Skip data + 4 bytes CRC
    }

    if width == 0 || height == 0 || idat_data.is_empty() || bit_depth != 8 {
        return None;
    }

    // 3. Decompress zlib stream using yazi
    let (decompressed, _) = yazi::decompress(&idat_data, yazi::Format::Zlib).ok()?;

    // 4. Unfilter scanlines and convert to RGBA
    let bytes_per_pixel = match color_type {
        0 => 1, // Grayscale
        2 => 3, // RGB
        3 => 1, // Indexed
        4 => 2, // Gray + Alpha
        6 => 4, // RGBA
        _ => return None,
    };

    let stride = (width as usize) * bytes_per_pixel;
    let expected_len = (height as usize) * (1 + stride);
    if decompressed.len() < expected_len {
        return None;
    }

    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    let mut prev_line = vec![0u8; stride];
    let mut curr_line = vec![0u8; stride];
    let mut src_offset = 0;

    for _ in 0..height {
        let filter_type = decompressed[src_offset];
        src_offset += 1;
        let line_data = &decompressed[src_offset..src_offset + stride];
        src_offset += stride;

        // Unfilter line
        for i in 0..stride {
            let a = if i >= bytes_per_pixel {
                curr_line[i - bytes_per_pixel]
            } else {
                0
            };
            let b = prev_line[i];
            let c = if i >= bytes_per_pixel {
                prev_line[i - bytes_per_pixel]
            } else {
                0
            };

            let raw = line_data[i];
            let val = match filter_type {
                0 => raw,
                1 => raw.wrapping_add(a),
                2 => raw.wrapping_add(b),
                3 => raw.wrapping_add(((a as u16 + b as u16) / 2) as u8),
                4 => {
                    let p = a as i16 + b as i16 - c as i16;
                    let pa = (p - a as i16).abs();
                    let pb = (p - b as i16).abs();
                    let pc = (p - c as i16).abs();
                    let pr = if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    };
                    raw.wrapping_add(pr)
                }
                _ => raw,
            };
            curr_line[i] = val;
        }

        // Convert to RGBA
        match color_type {
            6 => {
                // RGBA directly
                rgba.extend_from_slice(&curr_line);
            }
            2 => {
                // RGB -> RGBA
                for chunk in curr_line.chunks_exact(3) {
                    rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
                }
            }
            0 => {
                // Grayscale -> RGBA
                for &g in &curr_line {
                    rgba.extend_from_slice(&[g, g, g, 255]);
                }
            }
            4 => {
                // Gray + Alpha -> RGBA
                for chunk in curr_line.chunks_exact(2) {
                    rgba.extend_from_slice(&[chunk[0], chunk[0], chunk[0], chunk[1]]);
                }
            }
            3 => {
                // Indexed
                for &idx in &curr_line {
                    if let Some(col) = palette.get(idx as usize) {
                        rgba.extend_from_slice(&[col[0], col[1], col[2], 255]);
                    } else {
                        rgba.extend_from_slice(&[0, 0, 0, 255]);
                    }
                }
            }
            _ => {}
        }

        prev_line.copy_from_slice(&curr_line);
    }

    Some((width, height, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_decode() {
        let input = b"aGVsbG8gd29ybGQ=";
        let decoded = decode_base64(input).unwrap();
        assert_eq!(&decoded, b"hello world");

        let input_multiline = b"aGVs\r\nbG8=\n";
        let decoded2 = decode_base64(input_multiline).unwrap();
        assert_eq!(&decoded2, b"hello");
    }

    #[test]
    fn test_kitty_control_parsing() {
        let params = parse_kitty_controls("a=T,f=100,s=80,v=40,c=10,r=5,i=42,m=0");
        assert_eq!(params.action, 'T');
        assert_eq!(params.format, 100);
        assert_eq!(params.pixel_width, 80);
        assert_eq!(params.pixel_height, 40);
        assert_eq!(params.cols, 10);
        assert_eq!(params.rows, 5);
        assert_eq!(params.image_id, 42);
        assert!(!params.more_chunks);
    }

    #[test]
    fn test_kitty_rgba32_transmission() {
        let mut mgr = KittyGraphicsManager::new();
        // 2x2 red RGBA: 4 pixels of [255, 0, 0, 255]
        let red_pixel = [255, 0, 0, 255];
        let mut raw = Vec::new();
        for _ in 0..4 {
            raw.extend_from_slice(&red_pixel);
        }

        // Base64 of 4 RGBA red pixels: /wAA//8AAP//AAD//wAA/w==
        let payload = b"a=t,f=32,s=2,v=2,c=2,r=1,i=10;/wAA//8AAP//AAD//wAA/w==";
        let resp = mgr.handle_kitty_sequence(payload, 0, 0, 0);
        assert_eq!(resp, Some("\x1b_Gi=10;OK\x1b\\".to_string()));
        assert_eq!(mgr.images.len(), 1);
        assert_eq!(mgr.images[0].id, 10);
        assert_eq!(mgr.images[0].pixel_width, 2);
        assert_eq!(mgr.images[0].pixel_height, 2);
        assert_eq!(mgr.images[0].width_cells, 2);
        assert_eq!(mgr.images[0].height_cells, 1);
    }

    #[test]
    fn test_kitty_png_transmission() {
        let mut mgr = KittyGraphicsManager::new();
        let png_b64 = b"iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAE0lEQVR4nGP4z8DwHwwZGP6DAQBJyAn3FGMynQAAAABJRU5ErkJggg==";
        let mut seq = b"a=t,f=100,c=4,r=2,i=99;".to_vec();
        seq.extend_from_slice(png_b64);

        let resp = mgr.handle_kitty_sequence(&seq, 5, 2, 0);
        assert_eq!(resp, Some("\x1b_Gi=99;OK\x1b\\".to_string()));
        assert_eq!(mgr.images.len(), 1);
        let img = &mgr.images[0];
        assert_eq!(img.id, 99);
        assert_eq!(img.pixel_width, 2);
        assert_eq!(img.pixel_height, 2);
        assert_eq!(img.width_cells, 4);
        assert_eq!(img.height_cells, 2);
        assert_eq!(img.col, 5);
        assert_eq!(img.abs_row, 2);
        assert_eq!(img.rgba_data.len(), 16);
    }
}
