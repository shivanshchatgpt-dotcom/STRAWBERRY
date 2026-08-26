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

/// Screen capture service — runs in background thread.
/// 
/// Note: This is a simplified implementation. Full cross-platform capture
/// with xcap 0.9+ requires platform-specific implementations.
#[allow(dead_code)]
pub struct CaptureService {
    config: CaptureConfig,
    data_dir: PathBuf,
    running: Arc<Mutex<bool>>,
    last_hash: Arc<Mutex<Option<String>>>,
    last_app: Arc<Mutex<Option<String>>>,
    // Using a simple connection wrapper that's thread-safe
    db_conn: Arc<Mutex<rusqlite::Connection>>,
}

impl CaptureService {
    pub fn new(config: CaptureConfig, data_dir: PathBuf, db_conn: Arc<Mutex<rusqlite::Connection>>) -> Self {
        Self {
            config,
            data_dir,
            running: Arc::new(Mutex::new(false)),
            last_hash: Arc::new(Mutex::new(None)),
            last_app: Arc::new(Mutex::new(None)),
            db_conn,
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
        // Stub: In a real implementation, this would use xcap 0.9+ for X11/Wayland
        // For now, return an error to indicate capture is not fully implemented
        Err("Screen capture not fully implemented yet. Install grim (Wayland) or xcap (X11) and implement capture_screen().".to_string())
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
        let conn = self.db_conn.lock().map_err(|_| "DB lock")?;
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
            db_conn: self.db_conn.clone(),
        }
    }
}

// Platform-specific window info
#[cfg(target_os = "linux")]
fn get_active_window_linux() -> (String, String) {
    // Try xdotool first
    if let Ok(output) = std::process::Command::new("xdotool")
        .args(["getactivewindow", "getwindowname"])
        .output()
    {
        if output.status.success() {
            let title = String::from_utf8_lossy(&output.stdout).trim().to_string();
            // Try to get app name from WM_CLASS
            if let Ok(output) = std::process::Command::new("xdotool")
                .args(["getactivewindow", "getwindowclassname"])
                .output()
            {
                if output.status.success() {
                    let class = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    return (class, title);
                }
            }
            return ("unknown".to_string(), title);
        }
        ("unknown".to_string(), "unknown".to_string())
    } else {
        ("unknown".to_string(), "unknown".to_string())
    }
}

#[cfg(not(target_os = "linux"))]
fn get_active_window_linux() -> (String, String) {
    ("unknown".to_string(), "unknown".to_string())
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