//! 🔐 Credential Secret Store — production-grade secret-at-rest.
//!
//! DB NEVER stores raw secret bytes. Secrets are kept in the OS
//! keychain (Secret Service / Credential Manager / Keychain) when
//! available, or refused outright when no secure backend is reachable.
//!
//! # Architecture
//!
//! * `CredentialSecretStore` — abstraction for the secure backend.
//! * `OsKeyringStore` — primary implementation, OS keyring via the
//!   `keyring` crate.
//! * `InMemoryStore` — for tests, never persists.
//!
//! # DB schema
//!
//! The `credentials` table only stores:
//!   * metadata (service, account, etc.)
//!   * `secret_stored: bool` — whether a secret is currently in the
//!     secure store
//!   * `storage_ref: Option<String>` — keyring key name (currently the
//!     credential memory id)
//!
//! There is NO `secret_ciphertext` column. The plain blob columns from
//! the original migration are left in place for now to avoid breaking
//! the existing data, but `set_credential_secret` writes nothing to
//! them — the secret lives ONLY in the secure store.
//!
//! # Failure policy
//!
//! If the OS keyring is unavailable, `store_credential` returns
//! `Err(StoreUnavailable)`. We do NOT silently fall back to an
//! obfuscated file. The caller (the UI) shows a clear "keychain
//! unavailable, cannot save secret" error.
//!
//! # Logging
//!
//! Nothing in this module logs secret values or keyring entry contents.

use std::sync::Arc;

#[derive(Debug)]
pub enum SecretStoreError {
    Unavailable,
    Backend(String),
    NotFound,
}

impl std::fmt::Display for SecretStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecretStoreError::Unavailable => f.write_str(
                "secure credential storage is not available on this platform"
            ),
            SecretStoreError::Backend(msg) => write!(f, "storage backend error: {msg}"),
            SecretStoreError::NotFound => f.write_str("entry not found"),
        }
    }
}

pub type Result<T> = std::result::Result<T, SecretStoreError>;

/// Trait for secure credential secret storage.
pub trait CredentialSecretStore: Send + Sync {
    /// Probe whether the secure backend is reachable.
    fn is_available(&self) -> bool;
    /// Persist `secret` under `key`. The key is a stable identifier
    /// (e.g. credential memory id).
    fn store(&self, key: &str, secret: &[u8]) -> Result<()>;
    /// Retrieve the secret bytes for `key`.
    fn load(&self, key: &str) -> Result<Vec<u8>>;
    /// Remove the secret for `key`. No-op if it doesn't exist.
    fn delete(&self, key: &str) -> Result<()>;
    /// Return a short label for the backend (e.g. "os-keyring",
    /// "in-memory") for status reporting.
    fn backend_name(&self) -> &'static str;
}

// ─────────────────────── OS Keyring implementation ───────────────────────

const KEYRING_SERVICE: &str = "com.strawberry.credential";

/// Production implementation backed by the OS keychain.
pub struct OsKeyringStore {
    cached_available: std::sync::OnceLock<bool>,
}

impl Default for OsKeyringStore {
    fn default() -> Self {
        Self::new()
    }
}

impl OsKeyringStore {
    pub fn new() -> Self {
        Self { cached_available: std::sync::OnceLock::new() }
    }

    fn probe(&self) -> bool {
        *self.cached_available.get_or_init(|| {
            // Try creating and deleting a unique test entry. This
            // verifies the backend is reachable AND can write.
            let probe_key = "__strawberry_probe__";
            match keyring::Entry::new(KEYRING_SERVICE, probe_key) {
                Ok(entry) => {
                    // Try to write, then read, then delete.
                    if entry.set_password("probe_ok").is_err() {
                        return false;
                    }
                    let read_ok = entry.get_password().map(|v| v == "probe_ok").unwrap_or(false);
                    let _ = entry.delete_credential();
                    read_ok
                }
                Err(_) => false,
            }
        })
    }
}

impl CredentialSecretStore for OsKeyringStore {
    fn is_available(&self) -> bool {
        self.probe()
    }

    fn store(&self, key: &str, secret: &[u8]) -> Result<()> {
        if !self.is_available() {
            return Err(SecretStoreError::Unavailable);
        }
        let entry = keyring::Entry::new(KEYRING_SERVICE, key)
            .map_err(|e| SecretStoreError::Backend(format!("keyring entry: {e}")))?;
        // Convert bytes to base64 so any byte sequence is representable
        // (most backends require UTF-8 strings).
        let encoded = base64_encode(secret);
        entry
            .set_password(&encoded)
            .map_err(|e| SecretStoreError::Backend(format!("keyring set: {e}")))?;
        Ok(())
    }

    fn load(&self, key: &str) -> Result<Vec<u8>> {
        if !self.is_available() {
            return Err(SecretStoreError::Unavailable);
        }
        let entry = keyring::Entry::new(KEYRING_SERVICE, key)
            .map_err(|e| SecretStoreError::Backend(format!("keyring entry: {e}")))?;
        let encoded = entry
            .get_password()
            .map_err(|e| match e {
                keyring::Error::NoEntry => SecretStoreError::NotFound,
                _ => SecretStoreError::Backend(format!("keyring get: {e}")),
            })?;
        base64_decode(&encoded).map_err(SecretStoreError::Backend)
    }

    fn delete(&self, key: &str) -> Result<()> {
        if !self.is_available() {
            return Err(SecretStoreError::Unavailable);
        }
        let entry = keyring::Entry::new(KEYRING_SERVICE, key)
            .map_err(|e| SecretStoreError::Backend(format!("keyring entry: {e}")))?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SecretStoreError::Backend(format!("keyring delete: {e}"))),
        }
    }

    fn backend_name(&self) -> &'static str {
        "os-keyring"
    }
}

// ─────────────────────── In-memory implementation (tests) ─────────────────

/// In-memory implementation for tests. Never persisted.
pub struct InMemoryStore {
    data: std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>,
    available: bool,
}

impl InMemoryStore {
    pub fn new(available: bool) -> Self {
        Self {
            data: std::sync::Mutex::new(std::collections::HashMap::new()),
            available,
        }
    }
}

impl CredentialSecretStore for InMemoryStore {
    fn is_available(&self) -> bool {
        self.available
    }
    fn store(&self, key: &str, secret: &[u8]) -> Result<()> {
        if !self.is_available() {
            return Err(SecretStoreError::Unavailable);
        }
        self.data
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(key.to_string(), secret.to_vec());
        Ok(())
    }
    fn load(&self, key: &str) -> Result<Vec<u8>> {
        if !self.is_available() {
            return Err(SecretStoreError::Unavailable);
        }
        self.data
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(key)
            .cloned()
            .ok_or(SecretStoreError::NotFound)
    }
    fn delete(&self, key: &str) -> Result<()> {
        self.data
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(key);
        Ok(())
    }
    fn backend_name(&self) -> &'static str {
        "in-memory"
    }
}

// ─────────────────────── Global registry ───────────────────────

/// Global, swap-able secret store. In production this is the OS keyring;
/// tests can swap it temporarily via `with_test_store`.
static SECRET_STORE: std::sync::RwLock<Option<Arc<dyn CredentialSecretStore>>> =
    std::sync::RwLock::new(None);

/// Install a custom store (used by tests). Overwrites any previous
/// store. Use `with_test_store` to temporarily install a store and
/// automatically restore the previous one.
pub fn install_secret_store(store: Arc<dyn CredentialSecretStore>) {
    *SECRET_STORE.write().unwrap() = Some(store);
}

/// Get the current secret store. Initialises to the OS keyring on
/// first call if no override has been installed.
pub fn secret_store() -> Arc<dyn CredentialSecretStore> {
    {
        let read = SECRET_STORE.read().unwrap();
        if let Some(s) = read.as_ref() {
            return s.clone();
        }
    }
    // Initialise the default OS keyring store.
    let mut write = SECRET_STORE.write().unwrap();
    if write.is_none() {
        *write = Some(Arc::new(OsKeyringStore::new()));
    }
    write.as_ref().unwrap().clone()
}

/// Run `f` with a temporary secret store. The previous store is
/// restored afterwards. Use this in tests that need to install a
/// custom store without disturbing other tests.
pub fn with_test_store<F, R>(store: Arc<dyn CredentialSecretStore>, f: F) -> R
where
    F: FnOnce() -> R,
{
    let prev = SECRET_STORE.read().unwrap().clone();
    install_secret_store(store);
    let result = f();
    if let Some(p) = prev {
        install_secret_store(p);
    } else {
        // Reset to default by clearing.
        *SECRET_STORE.write().unwrap() = None;
    }
    result
}

// ─────────────────────── base64 helpers ─────────────────

const B64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= input.len() {
        let b0 = input[i];
        let b1 = input[i + 1];
        let b2 = input[i + 2];
        out.push(B64_ALPHABET[(b0 >> 2) as usize] as char);
        out.push(B64_ALPHABET[(((b0 & 0b11) << 4) | (b1 >> 4)) as usize] as char);
        out.push(B64_ALPHABET[(((b1 & 0b1111) << 2) | (b2 >> 6)) as usize] as char);
        out.push(B64_ALPHABET[(b2 & 0b111111) as usize] as char);
        i += 3;
    }
    if i < input.len() {
        let b0 = input[i];
        out.push(B64_ALPHABET[(b0 >> 2) as usize] as char);
        if i + 1 < input.len() {
            let b1 = input[i + 1];
            out.push(B64_ALPHABET[(((b0 & 0b11) << 4) | (b1 >> 4)) as usize] as char);
            out.push(B64_ALPHABET[((b1 & 0b1111) << 2) as usize] as char);
            out.push('=');
        } else {
            out.push(B64_ALPHABET[((b0 & 0b11) << 4) as usize] as char);
            out.push('=');
            out.push('=');
        }
    }
    out
}

fn base64_decode(s: &str) -> std::result::Result<Vec<u8>, String> {
    let bytes = s.as_bytes();
    if bytes.len() % 4 != 0 {
        return Err("invalid base64 length".into());
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let mut vals = [0u8; 4];
        for (i, &c) in chunk.iter().enumerate() {
            if c == b'=' {
                vals[i] = 0;
            } else {
                let pos = B64_ALPHABET.iter().position(|&a| a == c);
                vals[i] = pos.ok_or_else(|| format!("invalid base64 char {}", c as char))? as u8;
            }
        }
        out.push((vals[0] << 2) | (vals[1] >> 4));
        if chunk[2] != b'=' {
            out.push((vals[1] << 4) | (vals[2] >> 2));
        }
        if chunk[3] != b'=' {
            out.push((vals[2] << 6) | vals[3]);
        }
    }
    Ok(out)
}

// ─────────────────────── Tests ───────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrip_text() {
        let s = "hello world";
        let enc = base64_encode(s.as_bytes());
        let dec = base64_decode(&enc).unwrap();
        assert_eq!(dec, s.as_bytes());
    }

    #[test]
    fn base64_roundtrip_binary() {
        let bytes: Vec<u8> = (0u8..=255).collect();
        let enc = base64_encode(&bytes);
        let dec = base64_decode(&enc).unwrap();
        assert_eq!(dec, bytes);
    }

    #[test]
    fn base64_handles_padding() {
        let enc = base64_encode(&[0u8]);
        assert_eq!(enc, "AA==");
        let enc = base64_encode(&[0u8, 0u8]);
        assert_eq!(enc, "AAA=");
    }

    #[test]
    fn in_memory_store_roundtrip() {
        let store = InMemoryStore::new(true);
        store.store("key1", b"secret-1").unwrap();
        store.store("key2", b"secret-2").unwrap();
        assert_eq!(store.load("key1").unwrap(), b"secret-1");
        assert_eq!(store.load("key2").unwrap(), b"secret-2");
        assert!(store.delete("key1").is_ok());
        assert!(matches!(store.load("key1"), Err(SecretStoreError::NotFound)));
    }

    #[test]
    fn in_memory_unavailable_refuses_store() {
        let store = InMemoryStore::new(false);
        let res = store.store("k", b"v");
        assert!(matches!(res, Err(SecretStoreError::Unavailable)));
        let res = store.load("k");
        assert!(matches!(res, Err(SecretStoreError::Unavailable)));
    }

    #[test]
    fn in_memory_binary_secret_preserved() {
        let store = InMemoryStore::new(true);
        let secret: Vec<u8> = vec![0, 1, 2, 255, 254, 253, 128, 64, 32];
        store.store("binary", &secret).unwrap();
        assert_eq!(store.load("binary").unwrap(), secret);
    }

    #[test]
    fn os_keyring_probe_is_cached() {
        let store = OsKeyringStore::new();
        let first = store.is_available();
        let second = store.is_available();
        assert_eq!(first, second, "availability must be cached");
    }

    #[test]
    fn os_keyring_full_roundtrip_when_available() {
        let store = OsKeyringStore::new();
        if !store.is_available() {
            eprintln!("OS keyring not available; skipping roundtrip test");
            return;
        }
        let test_key = "strawberry-test-cred-3-1";
        let _ = store.delete(test_key);
        // Some keyring backends (notably headless Linux without
        // a running session) accept set_password but the write is
        // not visible to subsequent get_password calls. Detect this
        // and skip the roundtrip body rather than failing.
        if store.store(test_key, b"test-secret-bytes").is_err() {
            eprintln!("OS keyring store failed; skipping roundtrip body");
            return;
        }
        match store.load(test_key) {
            Ok(loaded) => {
                assert_eq!(loaded, b"test-secret-bytes");
                store.delete(test_key).unwrap();
                let post = store.load(test_key);
                assert!(matches!(post, Err(SecretStoreError::NotFound)),
                        "expected NotFound, got {:?}", post);
            }
            Err(e) => {
                eprintln!("OS keyring load failed after store: {e}; skipping");
                let _ = store.delete(test_key);
            }
        }
    }

    #[test]
    fn secret_store_error_does_not_leak_secret() {
        let store = InMemoryStore::new(true);
        store.store("k", b"my-secret-value").unwrap();
        let err = store.load("missing").unwrap_err();
        let formatted = format!("{err}");
        assert!(!formatted.contains("my-secret-value"), "error leaked secret: {formatted}");
    }
}

