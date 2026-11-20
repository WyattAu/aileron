//! Sync-at-rest encryption via age (scrypt passphrase).
//!
//! Linux-only: age 0.11 pulls i18n-embed -> glib via system-deps, which
//! has no Windows/macOS story in this workspace. Non-linux targets get
//! honest Err stubs (see aileron#16 — age target-gate tracking).

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
        // Pin the accepted work factor to age's canonical scrypt log_n (18):
        // without this, Identity::new derives max = target + 4 from the HOST
        // CPU speed, so slow CI runners refuse files encrypted at 18.
        let mut identity = age::scrypt::Identity::new(secret);
        identity.set_max_work_factor(18);
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

    /// Encrypt at an explicit scrypt work factor. The default wrapper uses
    /// the host-measured factor, which differs between machines and between
    /// encrypt/decrypt probes under parallel test load — tests pin 10
    /// (age's own floor) for stability.
    pub fn encrypt_data_with(
        data: &[u8],
        passphrase: &str,
        log_n: u8,
    ) -> Result<String, anyhow::Error> {
        let secret = age::secrecy::SecretString::from(passphrase.to_string());
        let mut recipient = age::scrypt::Recipient::new(secret);
        recipient.set_work_factor(log_n);
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
        // Pin the accepted work factor to age's canonical scrypt log_n (18):
        // without this, Identity::new derives max = target + 4 from the HOST
        // CPU speed, so slow CI runners refuse files encrypted at 18.
        let mut identity = age::scrypt::Identity::new(secret);
        identity.set_max_work_factor(18);

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

        #[test]
        fn test_encrypt_decrypt_roundtrip() {
            let dir = tempfile::tempdir().unwrap();
            let input = dir.path().join("plain.txt");
            let encrypted = dir.path().join("encrypted.age");
            let decrypted = dir.path().join("decrypted.txt");

            std::fs::write(&input, b"secret message 12345").unwrap();
            encrypt_file(&input, &encrypted, "testpass").unwrap();
            decrypt_file(&encrypted, &decrypted, "testpass").unwrap();

            let original = std::fs::read(&input).unwrap();
            let result = std::fs::read(&decrypted).unwrap();
            assert_eq!(original, result);
        }

        #[test]
        fn test_encrypt_data_armored_roundtrip() {
            let data = b"hello armored world";
            let passphrase = "mypass";

            let armored = encrypt_data_with(data, passphrase, 10).unwrap();
            assert!(armored.contains("BEGIN AGE ENCRYPTED FILE"));

            let decrypted = decrypt_data(&armored, passphrase).unwrap();
            assert_eq!(data.to_vec(), decrypted);
        }

        #[test]
        fn test_is_age_encrypted_binary() {
            let secret = age::secrecy::SecretString::from("pass".to_string());
            let recipient = age::scrypt::Recipient::new(secret);
            let encrypted = age::encrypt(&recipient, b"test").unwrap();

            assert!(is_age_encrypted(&encrypted));
            assert!(!is_age_encrypted(b"not encrypted data"));
        }

        #[test]
        fn test_wrong_passphrase_fails() {
            let dir = tempfile::tempdir().unwrap();
            let input = dir.path().join("plain.txt");
            let encrypted = dir.path().join("encrypted.age");
            let decrypted = dir.path().join("decrypted.txt");

            std::fs::write(&input, b"secret").unwrap();
            encrypt_file(&input, &encrypted, "correct").unwrap();
            let result = decrypt_file(&encrypted, &decrypted, "wrong");
            assert!(result.is_err());
        }
    }
}

#[cfg(target_os = "linux")]
pub use linux_impl::{
    decrypt_data, decrypt_file, encrypt_data, encrypt_data_with, encrypt_file, is_age_encrypted,
};

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

    pub fn encrypt_data_with(
        _data: &[u8],
        _passphrase: &str,
        _log_n: u8,
    ) -> Result<String, anyhow::Error> {
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
pub use stubs::{
    decrypt_data, decrypt_file, encrypt_data, encrypt_data_with, encrypt_file, is_age_encrypted,
};
