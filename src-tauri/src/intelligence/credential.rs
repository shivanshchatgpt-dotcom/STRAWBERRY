//! 🔐 Secure credential storage — API keys and secrets.
//!
//! Uses the OS keychain/keyring when available. Falls back to an obfuscated
//! file in the app data directory when keyring access fails (common in
//! headless/CI environments).
//!
//! SECURITY RULES:
//! - API keys are NEVER logged, displayed in reports, or stored in FTS.
//! - Raw keys are NEVER returned to the frontend except during initial entry.
//! - The frontend receives only safe status (available/unavailable, provider name).
//! - Tests use synthetic fake credentials only.

use std::path::PathBuf;

/// Service name used for keyring entries.
const KEYRING_SERVICE: &str = "com.local.chatmemorytree";

/// Whether the OS keyring is available (cached after first probe).
static KEYRING_AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Probe the OS keyring once and cache the result.
pub fn keyring_available() -> bool {
    *KEYRING_AVAILABLE.get_or_init(|| {
        // Try creating a test entry. If the platform keystore cannot be
        // initialised, Entry::new returns an error.
        match keyring::Entry::new(KEYRING_SERVICE, "__probe__") {
            Ok(_) => true,
            Err(_) => false,
        }
    })
}

/// Store a credential in the OS keychain.
///
/// Falls back to an obfuscated file if the keyring is unavailable.
pub fn store_credential(key: &str, value: &str) -> Result<(), String> {
    if keyring_available() {
        if let Err(e) = store_in_keyring(key, value) {
            eprintln!("Keyring store failed ({e}), falling back to file storage");
            store_in_file(key, value)?;
        } else {
            return Ok(());
        }
    } else {
        store_in_file(key, value)?;
    }
    Ok(())
}

/// Load a credential — tries OS keyring first, then file fallback.
pub fn load_credential(key: &str) -> Result<String, String> {
    if keyring_available() {
        match load_from_keyring(key) {
            Ok(val) => return Ok(val),
            Err(e) => {
                eprintln!("Keyring load failed ({e}), trying file storage");
            }
        }
    }
    load_from_file(key)
}

/// Delete a credential from all storage locations.
pub fn delete_credential(key: &str) -> Result<(), String> {
    let _ = delete_from_keyring(key);
    delete_from_file(key)?;
    Ok(())
}

/// Check if a credential exists without loading its value.
pub fn credential_exists(key: &str) -> bool {
    if keyring_available() && load_from_keyring(key).is_ok() {
        return true;
    }
    file_path(key).exists()
}

/// Check whether a specific credential lives in the OS keyring
/// (as opposed to the file fallback). Useful for status reporting.
pub fn credential_in_keyring(key: &str) -> bool {
    keyring_available() && load_from_keyring(key).is_ok()
}

// ─── Keyring (OS keychain) ─────────────────────────────────────────────────

fn keyring_entry(key: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, key)
        .map_err(|e| format!("Keyring entry error: {e}"))
}

fn store_in_keyring(key: &str, value: &str) -> Result<(), String> {
    let entry = keyring_entry(key)?;
    entry.set_password(value)
        .map_err(|e| format!("Keyring set_password error: {e}"))
}

fn load_from_keyring(key: &str) -> Result<String, String> {
    let entry = keyring_entry(key)?;
    entry.get_password()
        .map_err(|e| format!("Keyring get_password error: {e}"))
}

fn delete_from_keyring(key: &str) -> Result<(), String> {
    let entry = keyring_entry(key)?;
    entry.delete_credential()
        .map_err(|e| format!("Keyring delete error: {e}"))
}

// ─── File-based fallback ────────────────────────────────────────────────────

/// Directory for credential file storage.
fn credential_dir() -> PathBuf {
    let base = if cfg!(target_os = "windows") {
        std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    } else if cfg!(target_os = "macos") {
        std::env::var("HOME")
            .map(|h| PathBuf::from(h).join("Library/Application Support"))
            .unwrap_or_else(|_| PathBuf::from("."))
    } else {
        std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|_| {
                std::env::var("HOME")
                    .map(|h| PathBuf::from(h).join(".local/share"))
            })
            .unwrap_or_else(|_| PathBuf::from("."))
    };
    base.join("com.local.chatmemorytree").join("credentials")
}

fn file_path(key: &str) -> PathBuf {
    // Sanitize the key to prevent path traversal
    let safe_key: String = key
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    credential_dir().join(format!("{safe_key}.cred"))
}

/// Simple XOR obfuscation — NOT cryptographic security, but prevents
/// casual reading of the file. The real protection comes from file
/// permissions and the OS keychain being preferred.
fn simple_obfuscate(data: &[u8], key_seed: u8) -> Vec<u8> {
    data.iter()
        .enumerate()
        .map(|(i, b)| b ^ key_seed.wrapping_add(i as u8))
        .collect()
}

fn store_in_file(key: &str, value: &str) -> Result<(), String> {
    let dir = credential_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create credential dir: {e}"))?;

    let path = file_path(key);
    let obfuscated = simple_obfuscate(value.as_bytes(), 0xAB);
    std::fs::write(&path, &obfuscated)
        .map_err(|e| format!("Failed to write credential file: {e}"))?;

    // Set restrictive permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }

    Ok(())
}

fn load_from_file(key: &str) -> Result<String, String> {
    let path = file_path(key);
    let data = std::fs::read(&path).map_err(|e| format!("Credential file not found: {e}"))?;
    let deobfuscated = simple_obfuscate(&data, 0xAB);
    String::from_utf8(deobfuscated).map_err(|e| format!("Invalid credential data: {e}"))
}

fn delete_from_file(key: &str) -> Result<(), String> {
    let path = file_path(key);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("Failed to delete credential: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_obfuscate_roundtrip() {
        let original = b"sk-test123456789abcdef";
        let obfuscated = simple_obfuscate(original, 0xAB);
        assert_ne!(&obfuscated, original);
        let restored = simple_obfuscate(&obfuscated, 0xAB);
        assert_eq!(&restored, original);
    }

    #[test]
    fn file_store_and_load_roundtrip() {
        let test_key = "test-cred-roundtrip";
        let test_value = "fake-api-key-12345";

        store_in_file(test_key, test_value).unwrap();
        let loaded = load_from_file(test_key).unwrap();
        assert_eq!(loaded, test_value);
        delete_from_file(test_key).unwrap();
        assert!(!file_path(test_key).exists());
    }

    #[test]
    fn file_path_sanitizes_key() {
        let p = file_path("../../../etc/passwd");
        let path_str = p.to_string_lossy();
        assert!(!path_str.contains(".."), "path traversal must be sanitized");
        assert!(path_str.ends_with(".cred"));
    }

    #[test]
    fn credential_exists_after_store() {
        let test_key = "test-cred-exists";
        assert!(!credential_exists(test_key));
        store_in_file(test_key, "value").unwrap();
        assert!(credential_exists(test_key));
        delete_from_file(test_key).unwrap();
    }

    #[test]
    fn synthetic_key_never_logged() {
        // Verify that the obfuscation doesn't leak the original
        let key = "sk-superSecretKey12345";
        let obf = simple_obfuscate(key.as_bytes(), 0xAB);
        let obf_str = String::from_utf8_lossy(&obf);
        assert!(!obf_str.contains("superSecret"), "obfuscated must not contain original");
    }

    #[test]
    fn keyring_availability_is_cached() {
        // The result should be consistent across calls
        let first = keyring_available();
        let second = keyring_available();
        assert_eq!(first, second);
    }

    #[test]
    fn store_load_delete_roundtrip_via_fallback() {
        // Use file-based path explicitly to test the fallback mechanism
        let test_key = "test-fallback-roundtrip";
        let test_value = "fallback-secret-xyz";

        // Ensure clean state
        let _ = delete_from_file(test_key);
        assert!(!file_path(test_key).exists());

        store_in_file(test_key, test_value).unwrap();
        let loaded = load_from_file(test_key).unwrap();
        assert_eq!(loaded, test_value);
        assert!(credential_exists(test_key));

        delete_from_file(test_key).unwrap();
        assert!(!credential_exists(test_key));
    }
}
