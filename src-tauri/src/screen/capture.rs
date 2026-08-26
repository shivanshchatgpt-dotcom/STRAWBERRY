//! Screen capture service — cross-platform screenshot capture with diff detection,
//! privacy filtering, and background indexing.
//! 
//! NOTE: This is a simplified implementation that compiles. Full Wayland/X11
//! capture with xcap 0.9+ requires additional platform-specific work.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::thread;
use std::fs;

use image::DynamicImage;
use serde::{Deserialize, Serialize};

use crate::screen::hash::perceptual_hash;
use crate::storage::files::files_dir;

/// Capture configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureConfig {
    pub interval_secs: u64,
    pub min_change_threshold: u8,
    pub enable_ocr: bool,
    pub enable_embeddings: bool,
    pub blocklist: Vec<String>,
    pub max_width: u32,
    pub max_height: u32,
    pub jpeg_quality: u8,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            interval_secs: 30,
            min_change_threshold: 8,
            enable_ocr: false,
            enable_embeddings: false,
            blocklist: vec![
                "bank".to_string(),
                "password".to_string(),
                "1password".to_string(),
                "bitwarden".to_string(),
                "keepass".to_string(),
                "proton".to_string(),
            ],
            max_width: 1920,
            max_height: 1080,
            jpeg_quality: 85,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct FrameMeta {
    pub id: i64,
    pub ts: i64,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub file_path: String,
    pub width: u32,
    pub height: u32,
    pub byte_size: u64,
    pub perceptual_hash: String,
    pub ocr_text: Option<String>,
    pub is_blurred: bool,
    pub thumbnail_path: Option<String>,
}

/// Shared handle so commands can start/stop the background thread.
#[derive(Clone, Default)]
pub struct CaptureHandle(pub Arc<Mutex<Option<CaptureService>>>);

/// Screen capture service — runs in background thread.
/// Capture uses `grim` (Wayland) with X11 fallback via `scrot`/`import`.
#[allow(dead_code)]
pub struct CaptureService {
    config: CaptureConfig,
    data_dir: PathBuf,
    running: Arc<Mutex<bool>>,
    last_hash: Arc<Mutex<Option<String>>>,
    last_app: Arc<Mutex<Option<String>>>,
    app_state: Arc<crate::state::AppState>,
}

impl CaptureService {
    pub fn new(
        config: CaptureConfig,
        data_dir: PathBuf,
        app_state: Arc<crate::state::AppState>,
    ) -> Self {
        Self {
            config,
            data_dir,
            running: Arc::new(Mutex::new(false)),
            last_hash: Arc::new(Mutex::new(None)),
            last_app: Arc::new(Mutex::new(None)),
            app_state,
        }
    }

    pub fn start(&self) {
        *self.running.lock().unwrap() = true;
        let service = self.clone();
        thread::spawn(move || service.run_loop());
    }

    pub fn stop(&self) {
        *self.running.lock().unwrap() = false;
    }

    fn is_running(&self) -> bool {
        *self.running.lock().unwrap()
    }

    fn run_loop(&self) {
        let interval = Duration::from_secs(self.config.interval_secs);
        
        while self.is_running() {
            let start = Instant::now();
            
            if let Err(e) = self.capture_and_process() {
                eprintln!("Screen capture error: {}", e);
            }
            
            let elapsed = start.elapsed();
            if elapsed < interval {
                thread::sleep(interval - elapsed);
            }
        }
    }

    fn capture_and_process(&self) -> Result<(), String> {
        // 1. Capture screenshot (stubbed - returns error for now)
        let (img, app_name, window_title) = self.capture_screen()?;
        
        // 2. Check blocklist
        if self.is_blocked(&app_name, &window_title) {
            return Ok(());
        }
        
        // 3. Compute perceptual hash
        let p_hash = perceptual_hash(&img);
        
        // 4. Check if significantly changed
        let mut last_hash = self.last_hash.lock().unwrap();
        if let Some(last) = last_hash.as_ref() {
            if crate::screen::hash::is_similar(last, &p_hash, self.config.min_change_threshold) {
                return Ok(());
            }
        }
        *last_hash = Some(p_hash.clone());
        
        // 5. Check app change
        let mut last_app = self.last_app.lock().unwrap();
        if last_app.as_ref() == Some(&app_name) {
            // Same app, continue
        } else {
            *last_app = Some(app_name.clone());
        }
        
        // 6. Resize if needed
        let img = self.resize_if_needed(img);
        
        // 7. Save frame
        let frame = self.save_frame(img, &p_hash, app_name, window_title)?;
        
        // 8. Persist to database
        self.persist_frame(&frame)?;
        
        Ok(())
    }
    
    fn capture_screen(&self) -> Result<(DynamicImage, String, String), String> {
        // 1. KDE Wayland: spectacle (KWin ignores wlr-screencopy, so grim fails).
        let session = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_uppercase();
        if session.contains("KDE") {
            if let Ok(img) = self.capture_spectacle() {
                let (app, title) = active_window_best_effort();
                return Ok((img, app, title));
            }
        }
        // 2. wlroots-style Wayland: grim writes PNG to stdout.
        if let Ok(out) = std::process::Command::new("grim")
            .args(["-t", "png", "-"])
            .output()
        {
            if out.status.success() && !out.stdout.is_empty() {
                let img = image::load_from_memory(&out.stdout)
                    .map_err(|e| format!("grim PNG decode: {e}"))?;
                let (app, title) = active_window_best_effort();
                return Ok((img, app, title));
            }
        }
        // 3. X11 fallbacks
        for tool in [["scrot", "-z", "-o", "/dev/stdout"].as_slice(), ["import", "-window", "root", "-"].as_slice()] {
            if let Ok(out) = std::process::Command::new(tool[0]).args(&tool[1..]).output() {
                if out.status.success() && !out.stdout.is_empty() {
                    if let Ok(img) = image::load_from_memory(&out.stdout) {
                        let (app, title) = active_window_best_effort();
                        return Ok((img, app, title));
                    }
                }
            }
        }
        Err("No screen capture backend available (install spectacle on KDE, grim on wlroots, or scrot on X11).".to_string())
    }

    fn capture_spectacle(&self) -> Result<DynamicImage, String> {
        let tmp = self.data_dir.join(".spectacle-cap.png");
        let out = std::process::Command::new("spectacle")
            .args(["-b", "-n", "-f", "-o"])
            .arg(&tmp)
            .output()
            .map_err(|e| format!("spectacle spawn: {e}"))?;
        if !out.status.success() || !tmp.exists() {
            return Err("spectacle failed".to_string());
        }
        let img = image::open(&tmp).map_err(|e| format!("spectacle PNG decode: {e}"));
        let _ = std::fs::remove_file(&tmp);
        img
    }
    
    fn is_blocked(&self, app_name: &str, window_title: &str) -> bool {
        let haystack = format!("{} {}", app_name, window_title).to_lowercase();
        for pattern in &self.config.blocklist {
            if haystack.contains(&pattern.to_lowercase()) {
                return true;
            }
        }
        false
    }
    
    fn resize_if_needed(&self, img: DynamicImage) -> DynamicImage {
        let (w, h) = (img.width(), img.height());
        if w <= self.config.max_width && h <= self.config.max_height {
            return img;
        }
        img.resize(self.config.max_width, self.config.max_height, image::imageops::FilterType::Lanczos3)
    }
    
    fn save_frame(&self, img: DynamicImage, p_hash: &str, app_name: String, window_title: String) -> Result<FrameMeta, String> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
        let date_str = chrono::DateTime::from_timestamp_millis(now)
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "unknown".to_string());
        
        let screens_dir = files_dir(&self.data_dir).join("screens").join(&date_str);
        fs::create_dir_all(&screens_dir).map_err(|e| format!("mkdir: {}", e))?;
        
        let filename = format!("{}_{}.jpg", now, &p_hash[..8]);
        let file_path = screens_dir.join(&filename);
        
        // Save JPEG
        let mut buf = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Jpeg)
            .map_err(|e| format!("JPEG encode: {}", e))?;
        
        fs::write(&file_path, &buf).map_err(|e| format!("write: {}", e))?;
        
        // Generate thumbnail
        let thumb = img.thumbnail(200, 150);
        let thumb_path = screens_dir.join(format!("{}_thumb.jpg", now));
        thumb.save(&thumb_path).ok();
        
        Ok(FrameMeta {
            id: 0,
            ts: now,
            app_name: Some(app_name),
            window_title: Some(window_title),
            file_path: file_path.to_string_lossy().to_string(),
            width: img.width(),
            height: img.height(),
            byte_size: buf.len() as u64,
            perceptual_hash: p_hash.to_string(),
            ocr_text: None,
            is_blurred: false,
            thumbnail_path: Some(thumb_path.to_string_lossy().to_string()),
        })
    }
    
    fn persist_frame(&self, frame: &FrameMeta) -> Result<(), String> {
        let conn = self
            .app_state
            .conn
            .lock()
            .map_err(|_| "DB lock")?;
        conn.execute(
            "INSERT INTO screen_frames (ts, app_name, window_title, file_path, width, height, byte_size, perceptual_hash, ocr_text, is_blurred, thumbnail_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                frame.ts,
                frame.app_name,
                frame.window_title,
                frame.file_path,
                frame.width as i64,
                frame.height as i64,
                frame.byte_size as i64,
                frame.perceptual_hash,
                frame.ocr_text,
                frame.is_blurred as i32,
                frame.thumbnail_path,
            ],
        ).map_err(|e| format!("DB insert: {}", e))?;
        Ok(())
    }
}

impl Clone for CaptureService {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            data_dir: self.data_dir.clone(),
            running: self.running.clone(),
            last_hash: self.last_hash.clone(),
            last_app: self.last_app.clone(),
            app_state: self.app_state.clone(),
        }
    }
}

/// Best-effort active-window info. X11: xdotool. Wayland/KDE: KWin DBus
/// query is interactive, so we fall back to the desktop session name —
/// blocklist still applies to whatever title we can obtain.
fn active_window_best_effort() -> (String, String) {
    // X11 / XWayland path
    if let Ok(out) = std::process::Command::new("xdotool")
        .args(["getactivewindow", "getwindowname"])
        .output()
    {
        if out.status.success() {
            let title = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let app = std::process::Command::new("xdotool")
                .args(["getactivewindow", "getwindowclassname"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "x11".to_string());
            return (app, title);
        }
    }
    let session = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "wayland".to_string());
    (session, String::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn default_config_sane() {
        let cfg = CaptureConfig::default();
        assert_eq!(cfg.interval_secs, 30);
        assert!(!cfg.blocklist.is_empty());
    }
}