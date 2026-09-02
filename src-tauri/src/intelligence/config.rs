//! ⚙️ AI Configuration — non-secret provider settings stored in `app_meta`.
//!
//! Secret keys (API keys) go through `credential.rs` (OS keyring / file fallback).
//! Non-secret config (provider name, URL, model, enabled state) goes through
//! this module into the `app_meta` key-value table.
//!
//! This separation ensures:
//! - API keys never touch SQLite
//! - Config survives restarts
//! - Config is queryable for status display
//! - Config is notindexed or logged

use rusqlite::Connection;

// ─── Key constants ──────────────────────────────────────────────────────────

const KEY_AI_ENABLED: &str = "ai_enabled";
const KEY_AI_ACTIVE_PROVIDER: &str = "ai_active_provider";
const KEY_OLLAMA_MODEL: &str = "ai_ollama_model";
const KEY_BYOK_NAME: &str = "ai_byok_name";
const KEY_BYOK_URL: &str = "ai_byok_url";
const KEY_BYOK_MODEL: &str = "ai_byok_model";

// ─── Read helpers ───────────────────────────────────────────────────────────

fn meta_get(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM app_meta WHERE key = ?1",
        [key],
        |r| r.get::<_, String>(0),
    )
    .ok()
}

fn meta_set(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO app_meta(key, value) VALUES(?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![key, value],
    )
    .map_err(|e| format!("Failed to set {key}: {e}"))?;
    Ok(())
}

// ─── Public API ─────────────────────────────────────────────────────────────

/// Is AI enhancement enabled?
pub fn is_enabled(conn: &Connection) -> bool {
    meta_get(conn, KEY_AI_ENABLED).as_deref() == Some("1")
}

/// Set the AI enabled flag.
pub fn set_enabled(conn: &Connection, enabled: bool) -> Result<(), String> {
    meta_set(conn, KEY_AI_ENABLED, if enabled { "1" } else { "0" })
}

/// Get the active provider kind ("none", "ollama", "byok").
pub fn active_provider(conn: &Connection) -> String {
    meta_get(conn, KEY_AI_ACTIVE_PROVIDER)
        .unwrap_or_else(|| "none".into())
}

/// Set the active provider kind.
pub fn set_active_provider(conn: &Connection, provider: &str) -> Result<(), String> {
    meta_set(conn, KEY_AI_ACTIVE_PROVIDER, provider)
}

/// Get the Ollama model name.
pub fn ollama_model(conn: &Connection) -> String {
    meta_get(conn, KEY_OLLAMA_MODEL)
        .unwrap_or_else(|| "llama3".into())
}

/// Set the Ollama model name.
pub fn set_ollama_model(conn: &Connection, model: &str) -> Result<(), String> {
    meta_set(conn, KEY_OLLAMA_MODEL, model)
}

/// Get BYOK provider name.
pub fn byok_name(conn: &Connection) -> String {
    meta_get(conn, KEY_BYOK_NAME)
        .unwrap_or_else(|| "Custom".into())
}

/// Set BYOK provider name.
pub fn set_byok_name(conn: &Connection, name: &str) -> Result<(), String> {
    meta_set(conn, KEY_BYOK_NAME, name)
}

/// Get BYOK base URL.
pub fn byok_url(conn: &Connection) -> String {
    meta_get(conn, KEY_BYOK_URL)
        .unwrap_or_default()
}

/// Set BYOK base URL.
pub fn set_byok_url(conn: &Connection, url: &str) -> Result<(), String> {
    meta_set(conn, KEY_BYOK_URL, url)
}

/// Get BYOK model name.
pub fn byok_model(conn: &Connection) -> String {
    meta_get(conn, KEY_BYOK_MODEL)
        .unwrap_or_else(|| "gpt-4o".into())
}

/// Set BYOK model name.
pub fn set_byok_model(conn: &Connection, model: &str) -> Result<(), String> {
    meta_set(conn, KEY_BYOK_MODEL, model)
}

/// Check if any BYOK config exists.
pub fn byok_configured(conn: &Connection) -> bool {
    meta_get(conn, KEY_BYOK_URL).is_some()
}

/// Check if any Ollama config exists.
pub fn ollama_configured(conn: &Connection) -> bool {
    meta_get(conn, KEY_OLLAMA_MODEL).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS app_meta (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn enabled_roundtrip() {
        let conn = test_conn();
        assert!(!is_enabled(&conn));
        set_enabled(&conn, true).unwrap();
        assert!(is_enabled(&conn));
        set_enabled(&conn, false).unwrap();
        assert!(!is_enabled(&conn));
    }

    #[test]
    fn active_provider_roundtrip() {
        let conn = test_conn();
        assert_eq!(active_provider(&conn), "none");
        set_active_provider(&conn, "ollama").unwrap();
        assert_eq!(active_provider(&conn), "ollama");
        set_active_provider(&conn, "byok").unwrap();
        assert_eq!(active_provider(&conn), "byok");
    }

    #[test]
    fn ollama_model_roundtrip() {
        let conn = test_conn();
        assert_eq!(ollama_model(&conn), "llama3");
        set_ollama_model(&conn, "codellama").unwrap();
        assert_eq!(ollama_model(&conn), "codellama");
    }

    #[test]
    fn byok_config_roundtrip() {
        let conn = test_conn();
        assert!(!byok_configured(&conn));
        assert_eq!(byok_name(&conn), "Custom");
        assert_eq!(byok_url(&conn), "");
        assert_eq!(byok_model(&conn), "gpt-4o");

        set_byok_name(&conn, "OpenAI").unwrap();
        set_byok_url(&conn, "https://api.openai.com/v1").unwrap();
        set_byok_model(&conn, "gpt-4o-mini").unwrap();

        assert!(byok_configured(&conn));
        assert_eq!(byok_name(&conn), "OpenAI");
        assert_eq!(byok_url(&conn), "https://api.openai.com/v1");
        assert_eq!(byok_model(&conn), "gpt-4o-mini");
    }

    #[test]
    fn upsert_overwrites() {
        let conn = test_conn();
        set_ollama_model(&conn, "model1").unwrap();
        set_ollama_model(&conn, "model2").unwrap();
        assert_eq!(ollama_model(&conn), "model2");
    }
}
