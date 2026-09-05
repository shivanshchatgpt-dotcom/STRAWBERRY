//! 🔐 Credential Tauri commands
//!
//! Thin wrappers around the credential memory module. The UI uses these
//! to:
//!   * create / list / search credential metadata
//!   * explicitly reveal a secret (this is the ONLY way to read it)
//!   * copy a secret to clipboard (explicit action, not implicit)
//!   * delete a credential
//!
//! Secrets are NEVER returned by `search_credentials` or
//! `list_credentials`. Use `reveal_credential` explicitly.

use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::commands::Cmd;
use crate::memory::credential as creds;
use crate::state::AppState;

fn conn_of(app: &AppState) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, String> {
    app.conn.lock().map_err(|_| "db lock poisoned".to_string())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateCredentialArgs {
    pub title: String,
    pub service: String,
    pub account: Option<String>,
    pub username: Option<String>,
    pub environment: Option<String>,
    pub host: Option<String>,
    pub project: Option<String>,
    pub url: Option<String>,
    pub notes: Option<String>,
    /// Opaque bytes — caller is responsible for encryption before sending.
    /// The backend stores them as-is in the credentials table.
    pub secret_ciphertext: Option<Vec<u8>>,
    pub secret_nonce: Option<Vec<u8>>,
}

#[tauri::command]
pub async fn credential_create(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    args: CreateCredentialArgs,
) -> Cmd<String> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        creds::create_credential(
            &conn,
            &args.title,
            &args.service,
            args.account.as_deref(),
            args.username.as_deref(),
            args.environment.as_deref(),
            args.host.as_deref(),
            args.project.as_deref(),
            args.url.as_deref(),
            args.notes.as_deref(),
            args.secret_ciphertext.as_deref(),
            args.secret_nonce.as_deref(),
        )
    })
    .await
}

#[tauri::command]
pub async fn credential_get_metadata(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    id: String,
) -> Cmd<Option<creds::CredentialMetadata>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        creds::get_metadata(&conn, &id)
    })
    .await
}

#[tauri::command]
pub async fn credential_search(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    query: String,
    limit: Option<usize>,
) -> Cmd<Vec<creds::CredentialMetadata>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        creds::search_metadata(&conn, &query, limit.unwrap_or(50))
    })
    .await
}

/// EXPLICIT REVEAL — the only way to get the secret bytes out.
#[tauri::command]
pub async fn credential_reveal(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    id: String,
) -> Cmd<Option<Vec<u8>>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        creds::reveal_secret(&conn, &id)
    })
    .await
}

#[tauri::command]
pub async fn credential_update_secret(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    id: String,
    secret_ciphertext: Vec<u8>,
    secret_nonce: Vec<u8>,
) -> Cmd<bool> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        creds::update_secret(&conn, &id, &secret_ciphertext, &secret_nonce)
    })
    .await
}

#[tauri::command]
pub async fn credential_delete(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    id: String,
) -> Cmd<bool> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        creds::delete_credential(&conn, &id)
    })
    .await
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretStoreStatus {
    pub available: bool,
    pub backend: String,
}

/// Report whether the OS keyring is available for secret storage.
/// The UI uses this to decide whether to show the credential secret
/// input, or display a clear "keychain unavailable" message.
#[tauri::command]
pub async fn credential_secret_store_status() -> Cmd<SecretStoreStatus> {
    let store = crate::memory::secret_store::secret_store();
    Cmd::Ok(SecretStoreStatus {
        available: store.is_available(),
        backend: store.backend_name().to_string(),
    })
}
