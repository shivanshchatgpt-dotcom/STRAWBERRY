//! 🧠 Intelligence commands — AI provider configuration and status.
//!
//! Non-secret config (provider name, URL, model, enabled) → `app_meta` via `config.rs`
//! Secret keys (API keys) → OS keyring / file fallback via `credential.rs`
//!
//! These commands connect the React UI → Tauri backend → intelligence layer.
//! No fake buttons: every control actually connects through to the provider.

use std::sync::Arc;
use tauri::State;
use serde::{Deserialize, Serialize};

use crate::intelligence::{IntelligenceProvider, ProviderKind, ProviderStatus};
use crate::intelligence::{credential, config};
use super::blocking;
use crate::state::AppState;

pub type Cmd<T> = Result<T, String>;

// ─── Status ─────────────────────────────────────────────────────────────────

/// Current AI configuration status (returned to frontend).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStatus {
    /// Is AI enhancement enabled at all?
    pub enabled: bool,
    /// Active provider kind.
    pub active_provider: String,
    /// Provider statuses.
    pub providers: Vec<ProviderStatus>,
    /// Whether the active provider is reachable.
    pub available: bool,
    /// Whether the OS keyring is being used (vs file fallback).
    pub keyring_available: bool,
}

#[tauri::command]
pub async fn ai_get_status(state: State<'_, Arc<AppState>>) -> Cmd<AiStatus> {
    let st = state.inner().clone();
    blocking(st, |app| {
        let conn = app.conn.lock().map_err(|e| e.to_string())?;

        // Read non-secret config from app_meta
        let enabled = config::is_enabled(&conn);
        let active_provider = config::active_provider(&conn);

        let mut providers = vec![ProviderStatus {
            kind: ProviderKind::None,
            available: true,
            model: None,
            endpoint: None,
            last_check_ms: None,
            error: None,
        }];

        // Build provider statuses from stored config
        if active_provider == "ollama" || config::ollama_configured(&conn) {
            let model = config::ollama_model(&conn);
            let p = crate::intelligence::ollama::OllamaProvider::new(model);
            providers.push(p.check_health());
        }
        if active_provider == "byok" || config::byok_configured(&conn) {
            let name = config::byok_name(&conn);
            let base_url = config::byok_url(&conn);
            let model = config::byok_model(&conn);
            let p = crate::intelligence::byok::ByokProvider::new(
                name, base_url, model, "byok-api-key".into(),
            );
            let _ = p.load_key_from_storage();
            providers.push(p.check_health());
        }

        let available = providers.iter().any(|s| s.kind != ProviderKind::None && s.available);

        Ok(AiStatus {
            enabled,
            active_provider,
            providers,
            available,
            keyring_available: credential::keyring_available(),
        })
    })
    .await
}

// ─── Enable / Disable ───────────────────────────────────────────────────────

#[tauri::command]
pub async fn ai_set_enabled(state: State<'_, Arc<AppState>>, enabled: bool) -> Cmd<()> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|e| e.to_string())?;
        config::set_enabled(&conn, enabled)
    })
    .await
}

// ─── Provider Configuration ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigureProviderArgs {
    pub provider: String, // "ollama" or "byok"
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub name: Option<String>,
}

#[tauri::command]
pub async fn ai_configure_provider(
    state: State<'_, Arc<AppState>>,
    args: ConfigureProviderArgs,
) -> Cmd<()> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|e| e.to_string())?;

        let provider = args.provider.clone();
        let model_opt = args.model.clone();
        let name_opt = args.name.clone();
        let base_url_opt = args.base_url.clone();
        let api_key_opt = args.api_key.clone();

        match provider.as_str() {
            "ollama" => {
                let model = model_opt.unwrap_or_else(|| "llama3".into());
                config::set_ollama_model(&conn, &model)?;
                config::set_active_provider(&conn, "ollama")?;
            }
            "byok" => {
                let name = name_opt.unwrap_or_else(|| "Custom Provider".into());
                let base_url = base_url_opt
                    .ok_or("base_url is required for BYOK")?;
                let model = model_opt
                    .ok_or("model is required for BYOK")?;

                // Non-secret metadata → app_meta
                config::set_byok_name(&conn, &name)?;
                config::set_byok_url(&conn, &base_url)?;
                config::set_byok_model(&conn, &model)?;
                config::set_active_provider(&conn, "byok")?;

                // Secret API key → OS keyring / credential store
                if let Some(key) = api_key_opt {
                    credential::store_credential("byok-api-key", &key)
                        .map_err(|e| format!("Failed to save API key: {e}"))?;
                }
            }
            _ => return Err(format!("Unknown provider: {}", provider)),
        }
        Ok(())
    })
    .await
}

// ─── Connection Test ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn ai_test_connection(
    state: State<'_, Arc<AppState>>,
    provider: String,
) -> Cmd<String> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|e| e.to_string())?;

        match provider.as_str() {
            "ollama" => {
                let model = config::ollama_model(&conn);
                let p = crate::intelligence::ollama::OllamaProvider::new(model);
                let status = p.check_health();
                if status.available {
                    Ok("Ollama is running and reachable".into())
                } else {
                    Err(status.error.unwrap_or_else(|| "Ollama not reachable".into()))
                }
            }
            "byok" => {
                let name = config::byok_name(&conn);
                let base_url = config::byok_url(&conn);
                let model = config::byok_model(&conn);

                let p = crate::intelligence::byok::ByokProvider::new(
                    name, base_url, model, "byok-api-key".into(),
                );
                p.load_key_from_storage()?;
                p.test_connection()
            }
            _ => Err(format!("Unknown provider: {provider}")),
        }
    })
    .await
}

// ─── Model Discovery ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn ai_list_models(
    state: State<'_, Arc<AppState>>,
    provider: String,
) -> Cmd<Vec<String>> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|e| e.to_string())?;

        match provider.as_str() {
            "ollama" => {
                let p = crate::intelligence::ollama::OllamaProvider::new("".into());
                p.discover_models()
            }
            "byok" => {
                let name = config::byok_name(&conn);
                let base_url = config::byok_url(&conn);
                let model = config::byok_model(&conn);

                let p = crate::intelligence::byok::ByokProvider::new(
                    name, base_url, model, "byok-api-key".into(),
                );
                p.load_key_from_storage()?;
                p.list_models()
            }
            _ => Err(format!("Unknown provider: {provider}")),
        }
    })
    .await
}

// ─── Delete Credential ──────────────────────────────────────────────────────

#[tauri::command]
pub async fn ai_remove_credential(
    state: State<'_, Arc<AppState>>,
    provider: String,
) -> Cmd<()> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|e| e.to_string())?;

        match provider.as_str() {
            "byok" => {
                // Delete secret from credential store
                let _ = credential::delete_credential("byok-api-key");
                // Delete non-secret config from app_meta
                config::set_byok_name(&conn, "")?;
                config::set_byok_url(&conn, "")?;
                config::set_byok_model(&conn, "gpt-4o")?;
                config::set_active_provider(&conn, "none")?;
            }
            "ollama" => {
                config::set_ollama_model(&conn, "llama3")?;
                config::set_active_provider(&conn, "none")?;
            }
            _ => return Err(format!("Unknown provider: {provider}")),
        }
        Ok(())
    })
    .await
}
