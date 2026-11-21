//! Sync-at-rest encryption via age (scrypt passphrase).
//!
//! Linux-only: age 0.11 pulls i18n-embed -> glib via system-deps, which
//! has no Windows/macOS story in this workspace. Non-linux targets get
//! honest Err stubs (see aileron#16 — age target-gate tracking).
//!
//! Tests pin both scrypt work factors explicitly: the production wrappers
//! derive the factor from host timing, which races across parallel test
//! threads on shared runners (the encrypt probe and the decrypt probe
//! measure different targets), breaking roundtrips with "Excessive work
//! parameter".

#[cfg(target_os = "linux")]
mod linux_impl {
    use std::io::{Read, Write};
    use std::path::Path;

    #[must_use = "encryption result must be checked"]
    pub fn encrypt_file(
        input_path: &Path,
        output_path: &Path,
        passphrase: &str,
    ) -> Result<(), anyhow::Error> {
        let plaintext = std::fs::read(input_path)?;
        let secret = age::secrecy::SecretString::from(passphrase.to_string());
        let recipient = age::scrypt::Recipient::new(secret);
        let encrypted = age::encrypt(&recipient, &plaintext)
            .map_err(|e| anyhow::anyhow!("age encrypt: {e}"))?;

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(output_path, encrypted)?;
        Ok(())
    }

    #[must_use = "decryption result must be checked"]
    pub fn decrypt_file(
        input_path: &Path,
        output_path: &Path,
        passphrase: &str,
    ) -> Result<(), anyhow::Error> {
        let ciphertext = std::fs::read(input_path)?;
        let secret = age::secrecy::SecretString::from(passphrase.to_string());
        let identity = age::scrypt::Identity::new(secret);
        let decrypted = age::decrypt(&identity, &ciphertext)
            .map_err(|e| anyhow::anyhow!("age decrypt: {e}"))?;

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(output_path, decrypted)?;
        Ok(())
    }

    #[must_use = "ignoring this value may lead to data loss or unexpected behavior"]
    pub fn encrypt_data(data: &[u8], passphrase: &str) -> Result<String, anyhow::Error> {
        let secret = age::secrecy::SecretString::from(passphrase.to_string());
        let recipient = age::scrypt::Recipient::new(secret);
        let encrypted =
            age::encrypt(&recipient, data).map_err(|e| anyhow::anyhow!("age encrypt: {e}"))?;

        let mut output = Vec::new();
        let mut armored =
            age::armor::ArmoredWriter::wrap_output(&mut output, age::armor::Format::AsciiArmor)
                .map_err(|e| anyhow::anyhow!("age armor wrap: {e}"))?;
        armored
            .write_all(&encrypted)
            .map_err(|e| anyhow::anyhow!("age armor write: {e}"))?;
        armored
            .finish()
            .map_err(|e| anyhow::anyhow!("age armor finish: {e}"))?;

        String::from_utf8(output).map_err(|e| anyhow::anyhow!("armor utf8: {e}"))
    }

    #[must_use = "ignoring this value may lead to data loss or unexpected behavior"]
    pub fn decrypt_data(armored: &str, passphrase: &str) -> Result<Vec<u8>, anyhow::Error> {
        let secret = age::secrecy::SecretString::from(passphrase.to_string());
        let identity = age::scrypt::Identity::new(secret);

        let mut reader = age::armor::ArmoredReader::new(armored.as_bytes());
        let mut encrypted = Vec::new();
        reader
            .read_to_end(&mut encrypted)
            .map_err(|e| anyhow::anyhow!("age armor read: {e}"))?;

        let decrypted =
            age::decrypt(&identity, &encrypted).map_err(|e| anyhow::anyhow!("age decrypt: {e}"))?;
        Ok(decrypted)
    }

    pub fn is_age_encrypted(data: &[u8]) -> bool {
        if data.starts_with(b"age-encryption.org/") {
            return true;
        }
        let header = String::from_utf8_lossy(&data[..data.len().min(100)]);
        header.contains("BEGIN AGE ENCRYPTED FILE")
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use age::secrecy::SecretString;

        /// Deterministic age roundtrip at a pinned work factor. The
        /// production wrappers derive the factor from host timing, which
        /// races across parallel test threads (the encrypt probe and the
        /// decrypt probe measure different targets on shared runners) — so
        /// these tests pin both sides explicitly.
        #[test]
        fn test_age_roundtrip_pinned_work_factor() {
            let mut recipient =
                age::scrypt::Recipient::new(SecretString::from("pinned".to_string()));
            recipient.set_work_factor(10);

            let plaintext = b"secret message 12345";
            let encrypted = age::encrypt(&recipient, plaintext).unwrap();
            assert!(is_age_encrypted(&encrypted));

            let mut identity = age::scrypt::Identity::new(SecretString::from("pinned".to_string()));
            identity.set_max_work_factor(20);
            let decrypted = age::decrypt(&identity, &encrypted).unwrap();
            assert_eq!(decrypted, plaintext);

            // Wrong passphrase fails.
            let mut wrong = age::scrypt::Identity::new(SecretString::from("wrong".to_string()));
            wrong.set_max_work_factor(20);
            assert!(age::decrypt(&wrong, &encrypted).is_err());
        }

        #[test]
        fn test_is_age_encrypted_binary() {
            let mut recipient = age::scrypt::Recipient::new(SecretString::from("pass".to_string()));
            recipient.set_work_factor(10);
            let encrypted = age::encrypt(&recipient, b"test").unwrap();

            assert!(is_age_encrypted(&encrypted));
            assert!(!is_age_encrypted(b"not encrypted data"));
        }

        #[test]
        fn test_is_age_encrypted_armor_marker() {
            assert!(is_age_encrypted(b"-----BEGIN AGE ENCRYPTED FILE-----"));
            assert!(!is_age_encrypted(b"plain"));
        }
    }
}

#[cfg(target_os = "linux")]
pub use linux_impl::{decrypt_data, decrypt_file, encrypt_data, encrypt_file, is_age_encrypted};

/// Non-linux stubs: honest errors instead of silent plaintext (age's
/// i18n-embed -> glib chain cannot build off linux; aileron#16).
#[cfg(not(target_os = "linux"))]
mod stubs {
    const WHY: &str = "sync encryption requires the linux target (age/i18n-embed/glib)";

    pub fn encrypt_file(
        _input_path: &std::path::Path,
        _output_path: &std::path::Path,
        _passphrase: &str,
    ) -> Result<(), anyhow::Error> {
        Err(anyhow::anyhow!(WHY))
    }

    pub fn decrypt_file(
        _input_path: &std::path::Path,
        _output_path: &std::path::Path,
        _passphrase: &str,
    ) -> Result<(), anyhow::Error> {
        Err(anyhow::anyhow!(WHY))
    }

    pub fn encrypt_data(_data: &[u8], _passphrase: &str) -> Result<String, anyhow::Error> {
        Err(anyhow::anyhow!(WHY))
    }

    pub fn decrypt_data(_armored: &str, _passphrase: &str) -> Result<Vec<u8>, anyhow::Error> {
        Err(anyhow::anyhow!(WHY))
    }

    pub fn is_age_encrypted(_data: &[u8]) -> bool {
        false
    }
}

#[cfg(not(target_os = "linux"))]
pub use stubs::{decrypt_data, decrypt_file, encrypt_data, encrypt_file, is_age_encrypted};
