use std::io::{Read, Write};
use std::path::Path;

pub fn encrypt_file(
    input_path: &Path,
    output_path: &Path,
    passphrase: &str,
) -> Result<(), anyhow::Error> {
    let plaintext = std::fs::read(input_path)?;
    let secret = age::secrecy::SecretString::from(passphrase.to_string());
    let recipient = age::scrypt::Recipient::new(secret);
    let encrypted =
        age::encrypt(&recipient, &plaintext).map_err(|e| anyhow::anyhow!("age encrypt: {}", e))?;

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, encrypted)?;
    Ok(())
}

pub fn decrypt_file(
    input_path: &Path,
    output_path: &Path,
    passphrase: &str,
) -> Result<(), anyhow::Error> {
    let ciphertext = std::fs::read(input_path)?;
    let secret = age::secrecy::SecretString::from(passphrase.to_string());
    let identity = age::scrypt::Identity::new(secret);
    let decrypted =
        age::decrypt(&identity, &ciphertext).map_err(|e| anyhow::anyhow!("age decrypt: {}", e))?;

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, decrypted)?;
    Ok(())
}

pub fn encrypt_data(data: &[u8], passphrase: &str) -> Result<String, anyhow::Error> {
    let secret = age::secrecy::SecretString::from(passphrase.to_string());
    let recipient = age::scrypt::Recipient::new(secret);
    let encrypted =
        age::encrypt(&recipient, data).map_err(|e| anyhow::anyhow!("age encrypt: {}", e))?;

    let mut output = Vec::new();
    let mut armored =
        age::armor::ArmoredWriter::wrap_output(&mut output, age::armor::Format::AsciiArmor)
            .map_err(|e| anyhow::anyhow!("age armor wrap: {}", e))?;
    armored
        .write_all(&encrypted)
        .map_err(|e| anyhow::anyhow!("age armor write: {}", e))?;
    armored
        .finish()
        .map_err(|e| anyhow::anyhow!("age armor finish: {}", e))?;

    String::from_utf8(output).map_err(|e| anyhow::anyhow!("armor utf8: {}", e))
}

pub fn decrypt_data(armored: &str, passphrase: &str) -> Result<Vec<u8>, anyhow::Error> {
    let secret = age::secrecy::SecretString::from(passphrase.to_string());
    let identity = age::scrypt::Identity::new(secret);

    let mut reader = age::armor::ArmoredReader::new(armored.as_bytes());
    let mut encrypted = Vec::new();
    reader
        .read_to_end(&mut encrypted)
        .map_err(|e| anyhow::anyhow!("age armor read: {}", e))?;

    let decrypted =
        age::decrypt(&identity, &encrypted).map_err(|e| anyhow::anyhow!("age decrypt: {}", e))?;
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

        let armored = encrypt_data(data, passphrase).unwrap();
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
