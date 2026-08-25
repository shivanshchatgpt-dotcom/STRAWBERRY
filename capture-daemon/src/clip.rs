//! 🍓 Clipboard read/write across Wayland, X11 and native OS backends.
//!
//! The daemon previously only ever *read* the clipboard. Handoff has to write
//! back to it, and writing needs care on Wayland: there is no clipboard
//! "store", only a live data-source served by a running process. A background
//! `copy()` spawns a serving thread inside this process, so the compressed
//! packet stays pasteable exactly as long as the daemon lives.

use std::io::Read;

/// Which backend is active for this session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Wayland,
    Native,
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

/// Read the clipboard as UTF-8 text. `None` when empty, non-text or
/// unavailable — all three are normal, not errors worth logging.
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

/// How long the written selection must stay available.
///
/// This distinction only matters on Wayland, where the clipboard has no store:
/// the selection is served live by a process, and when that process exits the
/// selection reverts to whatever owned it before. A long-running daemon can
/// serve from a background thread, but a one-shot CLI has to stay in the
/// foreground or its packet vanishes before the user can paste.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Persist {
    /// Return immediately; the calling process stays alive to serve pastes.
    WhileRunning,
    /// Block, serving unlimited pastes, until another app takes the selection.
    ///
    /// Same contract as `wl-copy` in foreground mode. Serving only a single
    /// request is not sufficient: clipboard managers and some compositors
    /// request the data as soon as it is offered, which would consume the one
    /// serve before the user ever pressed paste.
    BlockUntilReplaced,
}

/// Replace the clipboard contents with `text`.
///
/// With [`Persist::BlockUntilReplaced`] this blocks on Wayland, so print any
/// user-facing report *before* calling it.
pub fn write(backend: Backend, text: &str, persist: Persist) -> Result<(), String> {
    match backend {
        Backend::Wayland => {
            use wl_clipboard_rs::copy::{
                ClipboardType, MimeType, Options, ServeRequests, Source,
            };
            let mut opts = Options::new();
            opts.clipboard(ClipboardType::Regular);
            // Never trim: the packet's trailing newline is part of its shape.
            opts.trim_newline(false);
            opts.serve_requests(ServeRequests::Unlimited);
            opts.foreground(persist == Persist::BlockUntilReplaced);
            opts.copy(
                Source::Bytes(text.as_bytes().to_vec().into_boxed_slice()),
                MimeType::Text,
            )
            .map_err(|e| format!("wayland copy failed: {e}"))
        }
        // X11/macOS/Windows keep a real clipboard owner, so both modes are the
        // same call; arboard's own thread outlives this function.
        Backend::Native => arboard::Clipboard::new()
            .map_err(|e| format!("clipboard unavailable: {e}"))?
            .set_text(text.to_string())
            .map_err(|e| format!("clipboard write failed: {e}")),
    }
}
