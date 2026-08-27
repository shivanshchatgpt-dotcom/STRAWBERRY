//! 🍓 Clipboard read/write across Wayland, X11 and native OS backends.
//!
//! Supports both text and image reading cross-platform (Wayland, macOS, Linux X11, Windows).

use std::io::Read;

/// Which backend is active for this session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Wayland,
    Native,
}

#[derive(Debug, Clone)]
pub struct ClipboardImage {
    pub width: u32,
    pub height: u32,
    pub rgba_bytes: Vec<u8>,
    pub png_bytes: Option<Vec<u8>>,
}

pub fn detect() -> Backend {
    let on_wayland = std::env::var("WAYLAND_DISPLAY")
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    if on_wayland {
        Backend::Wayland
    } else {
        Backend::Native
    }
}

impl Backend {
    pub fn label(self) -> &'static str {
        match self {
            Backend::Wayland => "Wayland (wl-clipboard)",
            Backend::Native => "X11 / native OS (arboard)",
        }
    }
}

/// Read the clipboard as UTF-8 text.
pub fn read(backend: Backend) -> Option<String> {
    match backend {
        Backend::Wayland => {
            use wl_clipboard_rs::paste::{get_contents, ClipboardType, MimeType, Seat};
            let (mut pipe, _mime) = get_contents(
                ClipboardType::Regular,
                Seat::Unspecified,
                MimeType::Specific("text/plain"),
            )
            .ok()?;
            let mut buf = String::new();
            pipe.read_to_string(&mut buf).ok()?;
            Some(buf)
        }
        Backend::Native => arboard::Clipboard::new().ok()?.get_text().ok(),
    }
}

pub fn compute_bytes_sig(width: u32, height: u32, bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    bytes.len().hash(&mut hasher);
    if !bytes.is_empty() {
        let step = (bytes.len() / 64).max(1);
        for i in (0..bytes.len()).step_by(step) {
            bytes[i].hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// Fast check if image clipboard changed, returning image and signature without re-allocating when unchanged.
pub fn read_image_if_changed(backend: Backend, last_sig: u64) -> Option<(ClipboardImage, u64)> {
    match backend {
        Backend::Wayland => {
            use wl_clipboard_rs::paste::{get_contents, ClipboardType, MimeType, Seat};
            if let Ok((mut pipe, _mime)) = get_contents(
                ClipboardType::Regular,
                Seat::Unspecified,
                MimeType::Specific("image/png"),
            ) {
                let mut buf = Vec::new();
                if pipe.read_to_end(&mut buf).is_ok() && !buf.is_empty() {
                    let sig = compute_bytes_sig(0, 0, &buf);
                    if sig == last_sig {
                        return None;
                    }
                    if let Ok(dynamic_img) = image::load_from_memory(&buf) {
                        let rgba = dynamic_img.to_rgba8();
                        let (width, height) = rgba.dimensions();
                        return Some((
                            ClipboardImage {
                                width,
                                height,
                                rgba_bytes: rgba.into_raw(),
                                png_bytes: Some(buf),
                            },
                            sig,
                        ));
                    }
                }
            }
            None
        }
        Backend::Native => {
            if let Ok(mut cb) = arboard::Clipboard::new() {
                if let Ok(img) = cb.get_image() {
                    let w = img.width as u32;
                    let h = img.height as u32;
                    let sig = compute_bytes_sig(w, h, &img.bytes);
                    if sig == last_sig {
                        return None;
                    }
                    return Some((
                        ClipboardImage {
                            width: w,
                            height: h,
                            rgba_bytes: img.bytes.into_owned(),
                            png_bytes: None,
                        },
                        sig,
                    ));
                }
            }
            None
        }
    }
}

/// Read image from clipboard across all OS platforms (Wayland, macOS, Linux X11, Windows).
#[allow(dead_code)]
pub fn read_image(backend: Backend) -> Option<ClipboardImage> {
    read_image_if_changed(backend, 0).map(|(img, _)| img)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_bytes_sig_stability() {
        let buf = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let sig1 = compute_bytes_sig(0, 0, &buf);
        let sig2 = compute_bytes_sig(0, 0, &buf);
        assert_eq!(sig1, sig2);

        let sig_dim = compute_bytes_sig(100, 200, &buf);
        let sig_dim2 = compute_bytes_sig(100, 200, &buf);
        assert_eq!(sig_dim, sig_dim2);
        assert_ne!(sig1, sig_dim);
    }

    #[test]
    fn test_wayland_png_decoding_simulated() {
        // Create an in-memory 15x25 PNG image buffer
        let mut png_bytes: Vec<u8> = Vec::new();
        let img = image::RgbaImage::new(15, 25);
        let mut cursor = std::io::Cursor::new(&mut png_bytes);
        img.write_to(&mut cursor, image::ImageFormat::Png).unwrap();

        let sig = compute_bytes_sig(0, 0, &png_bytes);
        // Load dynamically from memory like Wayland path
        let dynamic_img = image::load_from_memory(&png_bytes).unwrap();
        let rgba = dynamic_img.to_rgba8();
        let (width, height) = rgba.dimensions();

        assert_eq!(width, 15);
        assert_eq!(height, 25);
        assert_eq!(rgba.as_raw().len(), 15 * 25 * 4);

        let decoded_img = ClipboardImage {
            width,
            height,
            rgba_bytes: rgba.into_raw(),
            png_bytes: Some(png_bytes.clone()),
        };
        assert_eq!(decoded_img.width, 15);
        assert_eq!(decoded_img.height, 25);

        // Verification of signature equality to prevent polling loop
        assert_eq!(sig, compute_bytes_sig(0, 0, &png_bytes));
    }
}

/// How long the written selection must stay available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Persist {
    WhileRunning,
    BlockUntilReplaced,
}

/// Replace the clipboard contents with `text`.
pub fn write(backend: Backend, text: &str, persist: Persist) -> Result<(), String> {
    match backend {
        Backend::Wayland => {
            use wl_clipboard_rs::copy::{
                ClipboardType, MimeType, Options, ServeRequests, Source,
            };
            let mut opts = Options::new();
            opts.clipboard(ClipboardType::Regular);
            opts.trim_newline(false);
            opts.serve_requests(ServeRequests::Unlimited);
            opts.foreground(persist == Persist::BlockUntilReplaced);
            opts.copy(
                Source::Bytes(text.as_bytes().to_vec().into_boxed_slice()),
                MimeType::Text,
            )
            .map_err(|e| format!("wayland copy failed: {e}"))
        }
        Backend::Native => arboard::Clipboard::new()
            .map_err(|e| format!("clipboard unavailable: {e}"))?
            .set_text(text.to_string())
            .map_err(|e| format!("clipboard write failed: {e}")),
    }
}
