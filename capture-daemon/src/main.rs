//! 🍓 STRAWBERRY — Clipboard Auto-Capture Daemon
//! Background service that monitors system clipboard and auto-saves
//! selected text to STRAWBERRY with auto-indexing + metadata.
//!
//! Build:  cd strawberry-capture-daemon && cargo build --release
//! Run:    ./target/release/strawberry-capture-daemon

use arboard::Clipboard;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::collections::HashMap;

// API Keys (replace with your actual keys)
const OPENROUTER_KEY: &str = "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";
const API_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

// Content types for auto-classification
#[derive(Debug, Clone)]
enum ContentType {
    PlainText,
    Code,
    Error,
    Url,
    Command,
    Decision,
    ActionItem,
    Json,
    Unknown,
}

impl ContentType {
    fn label(&self) -> &'static str {
        match self {
            Self::PlainText => "note",
            Self::Code => "code",
            Self::Error => "error",
            Self::Url => "url",
            Self::Command => "command",
            Self::Decision => "decision",
            Self::ActionItem => "action_item",
            Self::Json => "json",
            Self::Unknown => "note",
        }
    }
}

/// Classify clipboard content type using regex patterns
fn classify_content(text: &str) -> ContentType {
    let text_lower = text.to_lowercase();
    
    // JSON check
    if text.trim().starts_with('{') || text.trim().starts_with('[') {
        return ContentType::Json;
    }
    
    // URL check
    if text.contains("http://") || text.contains("https://") {
        return ContentType::Url;
    }
    
    // Command patterns
    let command_patterns = ["npm ", "cargo ", "git ", "docker ", "pip ", "python ", "node ", "flutter ", "make "];
    for pattern in command_patterns {
        if text.starts_with(pattern) || text.contains(&format!(" ${pattern}")) {
            return ContentType::Command;
        }
    }
    
    // Error patterns
    let error_patterns = ["error", "exception", "traceback", "failed", "cannot", "invalid", "unauthorized"];
    for pattern in error_patterns {
        if text_lower.contains(pattern) {
            return ContentType::Error;
        }
    }
    
    // Decision patterns
    let decision_patterns = ["decided", "use this", "going with", "we will", "conclusion"];
    for pattern in decision_patterns {
        if text_lower.contains(pattern) {
            return ContentType::Decision;
        }
    }
    
    // Action item patterns
    let action_patterns = ["need to", "todo:", "fix ", "create ", "implement ", "add "];
    for pattern in action_patterns {
        if text_lower.contains(pattern) {
            return ContentType::ActionItem;
        }
    }
    
    // Code block indicators
    if text.contains("```") || text.contains("function ") || text.contains("const ") 
        || text.contains("def ") || text.contains("class ") || text.contains("fn ") {
        return ContentType::Code;
    }
    
    ContentType::PlainText
}

/// Generate keywords using GLM-5.2 (execution partner)
async fn generate_keywords_glm(text: &str) -> Vec<String> {
    let prompt = format!(
        "Extract 5-8 keywords from this content in JSON array format: {}\n\
         Return ONLY valid JSON like: [\"keyword1\", \"keyword2\", \"keyword3\"]",
        &text[..text.len().min(500)]
    );
    
    let client = reqwest::Client::new();
    match client.post(API_URL)
        .header("Authorization", format!("Bearer {}", OPENROUTER_KEY))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": "z-ai/glm-5.2",
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": 100
        }))
        .send()
        .await
    {
        Ok(resp) => {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(reasoning) = json["choices"][0]["message"]["reasoning"].as_str() {
                    // Try to extract JSON array from reasoning
                    if let Some(start) = reasoning.find('[') {
                        if let Some(end) = reasoning.rfind(']') {
                            let json_str = &reasoning[start..=end];
                            if let Ok(kw) = serde_json::from_str::<Vec<String>>(json_str) {
                                return kw;
                            }
                        }
                    }
                }
            }
        }
        Err(e) => eprintln!("GLM keyword error: {}", e),
    }
    
    // Fallback: simple keyword extraction
    fallback_keywords(text)
}

/// Fallback keyword extraction (no AI needed)
fn fallback_keywords(text: &str) -> Vec<String> {
    let stop_words = ["the", "a", "an", "is", "are", "was", "were", "be", "to", "of", "and", "in", "that", "it", "for", "on", "with", "this", "as", "at", "by", "from", "or", "but", "not", "you", "all", "can", "have", "has", "had", "do", "does", "did", "will", "would", "could", "should", "may", "might", "must", "shall", "i", "me", "my", "we", "our", "they", "their", "what", "which", "who", "whom", "when", "where", "why", "how"];
    
    text.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| w.len() > 3 && !stop_words.contains(&w.as_str()))
        .take(8)
        .collect()
}

/// Save to STRAWBERRY backend via Tauri IPC
fn save_to_strawberry(content_type: &ContentType, text: &str, keywords: &[String]) -> Result<(), String> {
    println!("🍓 SAVING TO STRAWBERRY:");
    println!("  Type: {}", content_type.label());
    println!("  Text: {}", &text[..text.len().min(100)]);
    println!("  Keywords: {:?}", keywords);
    
    // TODO: Integrate with existing STRAWBERRY Tauri backend
    // For now, save to local file as proof of concept
    let home = std::env::var("HOME").unwrap_or_default();
    let capture_dir = format!("{}/.strawberry/captures", home);
    std::fs::create_dir_all(&capture_dir).ok();
    
    let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let filename = format!("{}/{}.json", capture_dir, timestamp.replace(":", "-"));
    
    let capture = serde_json::json!({
        "type": content_type.label(),
        "text": text,
        "keywords": keywords,
        "timestamp": timestamp,
        "source": "clipboard-daemon"
    });
    
    std::fs::write(&filename, serde_json::to_string_pretty(&capture).unwrap())
        .map_err(|e| e.to_string())?;
    
    println!("  ✅ Saved to: {}", filename);
    Ok(())
}

/// Main clipboard monitoring loop
async fn clipboard_monitor(stop_flag: Arc<AtomicBool>) {
    let mut clipboard = match Clipboard::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to access clipboard: {}", e);
            return;
        }
    };
    
    let mut last_content = String::new();
    
    println!("🍓 STRAWBERRY Clipboard Daemon Started!");
    println!("   Monitoring clipboard... (Ctrl+C to stop)");
    println!("");
    
    while !stop_flag.load(Ordering::Relaxed) {
        if let Ok(content) = clipboard.get_text() {
            // Only process if content changed and is not empty
            if !content.is_empty() && content != last_content {
                last_content = content.clone();
                
                // Classify content type
                let content_type = classify_content(&content);
                
                // Skip very short content (probably accidental copy)
                if content.len() < 10 {
                    continue;
                }
                
                // Skip very long content (>10KB)
                if content.len() > 10_000 {
                    println!("⚠️  Content too long ({} chars), skipping...", content.len());
                    continue;
                }
                
                // Generate keywords asynchronously (GLM-5.2)
                let keywords = generate_keywords_glm(&content).await;
                
                // Save to STRAWBERRY
                if let Err(e) = save_to_strawberry(&content_type, &content, &keywords) {
                    eprintln!("❌ Save error: {}", e);
                }
            }
        }
        
        // Check every 500ms
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    
    println!("🍓 STRAWBERRY Daemon Stopped.");
}

#[tokio::main]
async fn main() {
    println!("");
    println!("╔═══════════════════════════════════════════╗");
    println!("║   🍓 STRAWBERRY CLIPBOARD AUTO-CAPTURE    ║");
    println!("║      Zero-Work Second Brain               ║");
    println!("╚═══════════════════════════════════════════╝");
    println!("");
    
    // Setup Ctrl+C handler
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = stop_flag.clone();
    
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        println!("\n🛑 Stopping...");
        stop_flag_clone.store(true, Ordering::Relaxed);
    });
    
    // Start monitoring
    clipboard_monitor(stop_flag).await;
}
