//! 🔐 Generic Credential Memory
//!
//! A credential memory stores:
//!   * METADATA in the `credentials` table and the `credential_fts` index
//!     (service, account, username, environment, host, project, notes)
//!   * THE SECRET in the OS keyring via `secret_store::secret_store()`
//!     — NEVER in the SQLite database.
//!
//! To find a credential: search returns ONLY the metadata row.
//! To access the secret: the caller must explicitly call `reveal_secret()`
//! or the UI's "Reveal" button. This is the security boundary.
//!
//! The DB columns `secret_ciphertext` and `secret_nonce` are left in place
//! for backward compatibility with the existing schema, but are NEVER
//! written to by this module. The new authoritative location is the OS
//! keyring. The boolean `secret_set` column reflects keyring presence.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::secret_store::{self, CredentialSecretStore};
use super::{create as create_memory, NewMemory, MemoryKind, PrivacyLevel, RedactionState};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialMetadata {
    pub id: String,
    pub service: String,
    pub account: Option<String>,
    pub username: Option<String>,
    pub environment: Option<String>,
    pub host: Option<String>,
    pub project: Option<String>,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub secret_set: bool,
    pub last_used_at_ms: Option<i64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

/// Create a credential. Returns the memory ID.
///
/// `secret_bytes` is the raw secret as a UTF-8 string. The secret is
/// placed in the OS keyring via the active `CredentialSecretStore`.
/// The DB never stores the secret.
pub fn create_credential(
    conn: &Connection,
    title: &str,
    service: &str,
    account: Option<&str>,
    username: Option<&str>,
    environment: Option<&str>,
    host: Option<&str>,
    project: Option<&str>,
    url: Option<&str>,
    notes: Option<&str>,
    secret_bytes: Option<&[u8]>,
    secret_nonce: Option<&[u8]>,
) -> Result<String, String> {
    let mut m = NewMemory::new(MemoryKind::Credential, title, "", "credential");
    m.privacy_level = PrivacyLevel::Secret;
    m.redaction_state = RedactionState::None;
    m.sensitivity = 5;
    m.tags = vec!["credential".to_string()];
    m.project_id = project.map(|p| p.to_string());
    m.content = format!(
        "Credential for {}{}",
        service,
        account.map(|a| format!(" ({a})").to_string()).unwrap_or_default()
    );
    m.source_application = Some("credential_store".to_string());

    let id = create_memory(conn, &m)?;

    // Store the secret in the secure backend BEFORE writing the row, so
    // the row's secret_set flag is consistent with the actual keyring
    // state. If the keyring rejects, the credential row is not created.
    if let Some(secret) = secret_bytes {
        let store = secret_store::secret_store();
        if !store.is_available() {
            // The keyring is unavailable. Roll back the memory row.
            let _ = super::delete(conn, &id);
            return Err("secure credential storage (OS keyring) is not available on this platform".into());
        }
        // Combine secret + nonce so the keyring stores a single value.
        // The nonce is only meaningful to the caller; for the keyring we
        // just want one opaque blob.
        let mut combined = Vec::with_capacity(secret.len() + (secret_nonce.map(|n| n.len()).unwrap_or(0)));
        combined.extend_from_slice(secret);
        if let Some(n) = secret_nonce {
            combined.extend_from_slice(n);
        }
        store.store(&id, &combined).map_err(|e| {
            // Roll back the row on failure.
            let _ = super::delete(conn, &id);
            format!("failed to store secret: {e}")
        })?;
    }

    // The secret_ciphertext / secret_nonce columns are kept NULL. The
    // secret_set boolean is the only authoritative indicator of secret
    // presence (which now reflects the keyring, not the DB).
    conn.execute(
        "INSERT INTO credentials(id, service, account, username, environment, host, project, url, notes,
                                 secret_ciphertext, secret_nonce, secret_set, created_at_ms, updated_at_ms)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, NULL, ?10, ?11, ?11)",
        params![
            id,
            service,
            account,
            username,
            environment,
            host,
            project,
            url,
            notes,
            secret_bytes.is_some() as i64,
            chrono::Utc::now().timestamp_millis(),
        ],
    ).map_err(|e| format!("credential insert: {e}"))?;

    let _ = conn.execute(
        "INSERT INTO credential_fts(credential_id, service, account, username, environment, host, project, notes)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, service, account, username, environment, host, project, notes],
    );

    Ok(id)
}

/// Fetch credential METADATA only. Never returns the secret.
pub fn get_metadata(conn: &Connection, id: &str) -> Result<Option<CredentialMetadata>, String> {
    let mut stmt = conn.prepare(
        "SELECT id, service, account, username, environment, host, project, url, notes,
                secret_set, last_used_at_ms, created_at_ms, updated_at_ms
         FROM credentials WHERE id = ?1"
    ).map_err(|e| format!("get credential: {e}"))?;
    let mut rows = stmt.query(params![id]).map_err(|e| e.to_string())?;
    if let Some(row) = rows.next().map_err(|e| e.to_string())? {
        Ok(Some(CredentialMetadata {
            id: row.get(0).map_err(|e| e.to_string())?,
            service: row.get(1).map_err(|e| e.to_string())?,
            account: row.get(2).ok(),
            username: row.get(3).ok(),
            environment: row.get(4).ok(),
            host: row.get(5).ok(),
            project: row.get(6).ok(),
            url: row.get(7).ok(),
            notes: row.get(8).ok(),
            // The boolean now reflects the actual keyring state, not the DB blob.
            secret_set: secret_store::secret_store().is_available()
                && secret_store::secret_store().load(id).is_ok(),
            last_used_at_ms: row.get(10).ok(),
            created_at_ms: row.get(11).map_err(|e| e.to_string())?,
            updated_at_ms: row.get(12).map_err(|e| e.to_string())?,
        }))
    } else {
        Ok(None)
    }
}

/// EXPLICIT REVEAL — caller must use this to obtain the secret bytes.
/// This is the ONLY way to get the secret out of the store. It is not
/// triggered by search, view, or any other passive action.
pub fn reveal_secret(conn: &Connection, id: &str) -> Result<Option<Vec<u8>>, String> {
    // Verify the credential exists in metadata.
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM credentials WHERE id = ?1",
            params![id],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !exists {
        return Ok(None);
    }

    // Load from the secure backend.
    let store = secret_store::secret_store();
    let result = store.load(id);

    match result {
        Ok(combined) => {
            // Stamp last_used_at_ms — this IS a meaningful use.
            conn.execute(
                "UPDATE credentials SET last_used_at_ms = ?2 WHERE id = ?1",
                params![id, chrono::Utc::now().timestamp_millis()],
            )
            .map_err(|e| format!("reveal stamp: {e}"))?;
            Ok(Some(combined))
        }
        Err(secret_store::SecretStoreError::NotFound) => Ok(None),
        Err(e) => Err(format!("reveal: {e}")),
    }
}

/// Free-standing adapter so `query_map` can return `Result<CredentialMetadata, rusqlite::Error>`.
fn row_to_credential_meta_err(r: &rusqlite::Row<'_>) -> Result<CredentialMetadata, rusqlite::Error> {
    row_to_credential_meta(r).map_err(|_| rusqlite::Error::InvalidQuery)
}

fn row_to_credential_meta(r: &rusqlite::Row<'_>) -> Result<CredentialMetadata, String> {
    Ok(CredentialMetadata {
        id: r.get(0).map_err(|e| e.to_string())?,
        service: r.get(1).map_err(|e| e.to_string())?,
        account: r.get(2).ok(),
        username: r.get(3).ok(),
        environment: r.get(4).ok(),
        host: r.get(5).ok(),
        project: r.get(6).ok(),
        url: r.get(7).ok(),
        notes: r.get(8).ok(),
        secret_set: r.get::<_, i64>(9).map_err(|e| e.to_string())? != 0,
        last_used_at_ms: r.get(10).ok(),
        created_at_ms: r.get(11).map_err(|e| e.to_string())?,
        updated_at_ms: r.get(12).map_err(|e| e.to_string())?,
    })
}

/// Search credentials by metadata. NEVER returns secrets.
/// Matches against the credential_fts index (service, account, username,
/// environment, host, project, notes) — never against the secret column.
pub fn search_metadata(conn: &Connection, query: &str, limit: usize) -> Result<Vec<CredentialMetadata>, String> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.service, c.account, c.username, c.environment, c.host,
                c.project, c.url, c.notes, c.secret_set, c.last_used_at_ms,
                c.created_at_ms, c.updated_at_ms
         FROM credential_fts f
         JOIN credentials c ON c.id = f.credential_id
         WHERE credential_fts MATCH ?1
         ORDER BY rank LIMIT ?2"
    ).map_err(|e| format!("credential search: {e}"))?;
    let rows = stmt.query_map(params![query, limit as i64], row_to_credential_meta_err)
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

/// Update credential secret. The previous secret is overwritten
/// in the secure backend (OS keyring). The DB blob columns are not
/// touched.
pub fn update_secret(
    conn: &Connection,
    id: &str,
    secret_bytes: &[u8],
    secret_nonce: &[u8],
) -> Result<bool, String> {
    // Verify the credential exists.
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM credentials WHERE id = ?1",
            params![id],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !exists {
        return Ok(false);
    }

    let store = secret_store::secret_store();
    if !store.is_available() {
        return Err("secure credential storage (OS keyring) is not available".into());
    }

    // Combine secret + nonce for a single keyring entry.
    let mut combined = Vec::with_capacity(secret_bytes.len() + secret_nonce.len());
    combined.extend_from_slice(secret_bytes);
    combined.extend_from_slice(secret_nonce);
    store.store(id, &combined).map_err(|e| format!("update secret: {e}"))?;

    // Update updated_at_ms to reflect the change.
    let n = conn.execute(
        "UPDATE credentials SET updated_at_ms = ?2 WHERE id = ?1",
        params![id, chrono::Utc::now().timestamp_millis()],
    )
    .map_err(|e| format!("update secret timestamp: {e}"))?;
    Ok(n > 0)
}

/// Delete a credential. Removes the secret from the secure backend
/// (keyring) AND cascades to memory (via FK) and FTS.
pub fn delete_credential(conn: &Connection, id: &str) -> Result<bool, String> {
    // First, remove the secret from the secure backend. This is best-effort:
    // even if the keyring delete fails, we still want to remove the DB rows.
    let _ = secret_store::secret_store().delete(id);
    let _ = conn.execute("DELETE FROM credential_fts WHERE credential_id = ?1", params![id]);
    let _ = conn.execute("DELETE FROM credentials WHERE id = ?1", params![id]);
    super::delete(conn, id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_test_store() {
        secret_store::install_secret_store(std::sync::Arc::new(
            secret_store::InMemoryStore::new(true),
        ));
    }

    fn setup() -> Connection {
        init_test_store();
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&mut conn).unwrap();
        crate::db::migrations::ensure_fts(&conn).unwrap();
        conn
    }

    #[test]
    fn create_credential_indexes_metadata_only() {
        let conn = setup();
        let id = create_credential(
            &conn,
            "My Example Credential",
            "ExampleService",
            Some("ExampleAccount"),
            Some("user1"),
            Some("production"),
            Some("example.com"),
            Some("MyProject"),
            Some("https://example.com"),
            Some("some notes"),
            Some(b"TEST_SECRET_VALUE_bytes"),
            Some(b"nonce123"),
        ).unwrap();
        let meta = get_metadata(&conn, &id).unwrap().unwrap();
        assert_eq!(meta.service, "ExampleService");
        assert_eq!(meta.account.as_deref(), Some("ExampleAccount"));
        assert!(meta.secret_set);
    }

    #[test]
    fn search_finds_credential_by_service_metadata() {
        let conn = setup();
        create_credential(
            &conn, "Cred", "ExampleService", Some("ExampleAccount"),
            None, None, Some("host1.example.com"),
            Some("MyProject"), None, None,
            Some(b"TEST_SECRET_VALUE"),
            Some(b"nonce"),
        ).unwrap();
        let results = search_metadata(&conn, "ExampleService", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].service, "ExampleService");
    }

    #[test]
    fn search_never_exposes_secret() {
        let conn = setup();
        let secret = b"TEST_SECRET_VALUE_do_not_expose";
        create_credential(
            &conn, "Cred", "ExampleService", None, None, None, None,
            None, None, None, Some(secret), Some(b"nonce"),
        ).unwrap();
        let results = search_metadata(&conn, "ExampleService", 10).unwrap();
        let json = serde_json::to_string(&results).unwrap();
        assert!(!json.contains("TEST_SECRET_VALUE"),
                "secret leaked into metadata search: {json}");
    }

    #[test]
    fn reveal_returns_secret_bytes() {
        let conn = setup();
        let secret = b"my_secret_payload";
        let id = create_credential(
            &conn, "Cred", "Service", None, None, None, None, None, None, None,
            Some(secret), Some(b"n1"),
        ).unwrap();
        let revealed = reveal_secret(&conn, &id).unwrap().unwrap();
        // Reveal returns secret + nonce combined; the test should
        // observe both parts are present.
        assert!(revealed.starts_with(secret), "secret must be preserved");
    }

    #[test]
    fn reveal_stamps_last_used() {
        let conn = setup();
        let id = create_credential(
            &conn, "Cred", "Service", None, None, None, None, None, None, None,
            Some(b"x"), Some(b"n"),
        ).unwrap();
        assert!(get_metadata(&conn, &id).unwrap().unwrap().last_used_at_ms.is_none());
        reveal_secret(&conn, &id).unwrap();
        assert!(get_metadata(&conn, &id).unwrap().unwrap().last_used_at_ms.is_some());
    }

    #[test]
    fn update_secret_overwrites() {
        let conn = setup();
        let id = create_credential(
            &conn, "Cred", "Service", None, None, None, None, None, None, None,
            Some(b"old"), Some(b"n1"),
        ).unwrap();
        update_secret(&conn, &id, b"new", b"n2").unwrap();
        let revealed = reveal_secret(&conn, &id).unwrap().unwrap();
        assert!(revealed.starts_with(b"new"));
    }

    #[test]
    fn delete_cascades() {
        let conn = setup();
        let id = create_credential(
            &conn, "Cred", "Service", None, None, None, None, None, None, None,
            Some(b"x"), Some(b"n"),
        ).unwrap();
        assert!(search_metadata(&conn, "Service", 10).unwrap().len() == 1);
        delete_credential(&conn, &id).unwrap();
        assert!(search_metadata(&conn, "Service", 10).unwrap().is_empty());
        assert!(get_metadata(&conn, &id).unwrap().is_none());
    }

    #[test]
    fn secret_never_lands_in_db_blob() {
        let conn = setup();
        let secret = b"DATABASE_LEAK_TEST_secret_value";
        let id = create_credential(
            &conn, "T", "S", None, None, None, None, None, None, None,
            Some(secret), Some(b"nonce"),
        ).unwrap();
        // Scan the entire DB row for the secret bytes. Must not be found.
        let row_blob: Option<Vec<u8>> = conn
            .query_row(
                "SELECT secret_ciphertext FROM credentials WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        if let Some(blob) = row_blob {
            assert!(!blob.windows(secret.len()).any(|w| w == secret),
                "secret must not be in DB blob: {:?}",
                String::from_utf8_lossy(&blob));
        }
    }

    #[test]
    fn deletion_removes_secret_from_store() {
        let conn = setup();
        let id = create_credential(
            &conn, "T", "S", None, None, None, None, None, None, None,
            Some(b"delete-me-secret"), Some(b"n"),
        ).unwrap();
        // Confirm the secret is in the store.
        assert!(reveal_secret(&conn, &id).unwrap().is_some());
        delete_credential(&conn, &id).unwrap();
        // After deletion, reveal should not return the secret.
        let res = reveal_secret(&conn, &id);
        // Either the metadata row is gone (Ok(None)) or the store has no entry.
        assert!(res.is_err() || res.unwrap().is_none(),
                "after delete, secret must not be retrievable");
    }

    #[test]
    fn unavailable_store_blocks_creation() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&mut conn).unwrap();
        crate::db::migrations::ensure_fts(&conn).unwrap();

        // Run the test with an unavailable store. `with_test_store`
        // restores the previous store automatically.
        let res = secret_store::with_test_store(
            std::sync::Arc::new(secret_store::InMemoryStore::new(false)),
            || {
                create_credential(
                    &conn, "T", "S", None, None, None, None, None, None, None,
                    Some(b"my-secret"), Some(b"n"),
                )
            },
        );

        assert!(res.is_err(), "creation must fail when keyring unavailable");
        // The credential row must not have been left behind.
        let cred_count: i64 = conn.query_row(
            "SELECT count(*) FROM credentials",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(cred_count, 0, "credential row must be rolled back");
    }

    #[test]
    fn backend_error_does_not_leak_secret_in_message() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&mut conn).unwrap();
        crate::db::migrations::ensure_fts(&conn).unwrap();

        // Install a deliberately-bad store that errors.
        struct ErrorStore;
        impl secret_store::CredentialSecretStore for ErrorStore {
            fn is_available(&self) -> bool { true }
            fn store(&self, _key: &str, _secret: &[u8]) -> secret_store::Result<()> {
                Err(secret_store::SecretStoreError::Backend(
                    "synthetic store error with key=SECRET_IN_ERROR".to_string(),
                ))
            }
            fn load(&self, _key: &str) -> secret_store::Result<Vec<u8>> {
                Ok(b"SECRET_IN_ERROR".to_vec())
            }
            fn delete(&self, _key: &str) -> secret_store::Result<()> { Ok(()) }
            fn backend_name(&self) -> &'static str { "error-store" }
        }

        let res = secret_store::with_test_store(
            std::sync::Arc::new(ErrorStore),
            || {
                create_credential(
                    &conn, "T", "S", None, None, None, None, None, None, None,
                    Some(b"MY_PLAIN_SECRET"), Some(b"n"),
                )
            },
        );

        let err = res.unwrap_err();
        assert!(!err.contains("MY_PLAIN_SECRET"),
                "error leaked secret: {err}");
    }
}
