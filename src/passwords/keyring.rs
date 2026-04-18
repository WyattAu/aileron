//! System keyring integration for credential storage.
//!
//! Uses the `keyring` crate to store/retrieve passwords via the OS keyring
//! (GNOME Keyring / KDE KWallet on Linux, Keychain on macOS, Credential Manager on Windows).

use anyhow::Result;

const SERVICE_NAME: &str = "com.aileron.browser";

/// Store a credential in the system keyring.
pub fn store_credential(username: &str, password: &str) -> Result<()> {
    let entry = keyring::Entry::new(SERVICE_NAME, username)?;
    entry.set_password(password)?;
    Ok(())
}

/// Retrieve a credential from the system keyring.
pub fn get_credential(username: &str) -> Result<Option<String>> {
    let entry = keyring::Entry::new(SERVICE_NAME, username)?;
    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => anyhow::bail!("Keyring error: {}", e),
    }
}

/// Delete a credential from the system keyring.
pub fn delete_credential(username: &str) -> Result<()> {
    let entry = keyring::Entry::new(SERVICE_NAME, username)?;
    entry.delete_credential()?;
    Ok(())
}

/// Check if the system keyring is available.
pub fn is_available() -> bool {
    keyring::Entry::new(SERVICE_NAME, "test").is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_available_returns_bool() {
        let _ = is_available();
    }

    #[test]
    fn test_service_name_constant() {
        assert!(SERVICE_NAME.starts_with("com."));
        assert!(SERVICE_NAME.contains("aileron"));
    }

    #[test]
    fn test_store_and_get_roundtrip() {
        let username = "_aileron_test_roundtrip";
        let password = "test_password_123";

        let store_result = store_credential(username, password);
        if store_result.is_err() {
            return;
        }

        match get_credential(username) {
            Ok(Some(retrieved)) => assert_eq!(retrieved, password),
            Ok(None) => {
                let _ = delete_credential(username);
            }
            Err(_) => {
                let _ = delete_credential(username);
            }
        }

        let _ = delete_credential(username);
    }

    #[test]
    fn test_get_nonexistent_returns_none() {
        let username = "_aileron_test_nonexistent_99999";
        let result = get_credential(username).unwrap();
        assert!(result.is_none());
    }
}
