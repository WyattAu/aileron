use std::path::Path;

use aileron::sync::core::{ChunkHash, DeltaAction, FileManifest, SyncManager, SyncManifest};
use aileron::sync::crypto::{decrypt_data, decrypt_file, encrypt_data, encrypt_file};
use aileron::sync::transport::SyncTarget;

fn make_file(dir: &Path, name: &str, content: &[u8]) -> std::path::PathBuf {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, content).unwrap();
    path
}

#[test]
fn test_manifest_computation_content_addressing() {
    let dir = tempfile::tempdir().unwrap();
    let content_a = b"hello world";
    let content_b = b"different content";
    make_file(dir.path(), "alpha.txt", content_a);
    make_file(dir.path(), "sub/beta.txt", content_b);
    make_file(dir.path(), "empty.txt", b"");

    let sm = SyncManager::new(dir.path().to_path_buf());
    let manifest = sm.compute_manifest().unwrap();

    assert_eq!(manifest.files.len(), 3);

    let alpha = &manifest.files["alpha.txt"];
    assert_eq!(
        alpha.blake3_hash,
        blake3::hash(content_a).to_string(),
        "hash must match blake3 of file content"
    );
    assert_eq!(alpha.size, content_a.len() as u64);

    let beta = &manifest.files["sub/beta.txt"];
    assert_eq!(beta.blake3_hash, blake3::hash(content_b).to_string());

    let empty = &manifest.files["empty.txt"];
    assert!(
        empty.chunks.is_empty(),
        "empty file should have zero chunks"
    );
}

#[test]
fn test_manifest_same_content_same_hash() {
    let dir = tempfile::tempdir().unwrap();
    make_file(dir.path(), "a.txt", b"identical");
    make_file(dir.path(), "b.txt", b"identical");

    let sm = SyncManager::new(dir.path().to_path_buf());
    let manifest = sm.compute_manifest().unwrap();

    assert_eq!(
        manifest.files["a.txt"].blake3_hash, manifest.files["b.txt"].blake3_hash,
        "content-addressed: same bytes => same hash"
    );
}

#[test]
fn test_delta_detection_modified_file() {
    let dir = tempfile::tempdir().unwrap();
    make_file(dir.path(), "data.toml", b"version = 1");

    let sm = SyncManager::new(dir.path().to_path_buf());
    let original = sm.compute_manifest().unwrap();
    sm.update_manifest(original);

    make_file(dir.path(), "data.toml", b"version = 2");
    let updated = sm.compute_manifest().unwrap();

    let delta = sm.compute_delta(&updated);
    let upload_actions: Vec<_> = delta
        .iter()
        .filter(|a| matches!(a, DeltaAction::Upload(_) | DeltaAction::UploadChunks(_, _)))
        .collect();

    assert!(
        !upload_actions.is_empty(),
        "modified file should produce an upload action"
    );
}

#[test]
fn test_delta_detection_new_and_removed_files() {
    let dir = tempfile::tempdir().unwrap();
    make_file(dir.path(), "keep.txt", b"stays");

    let sm = SyncManager::new(dir.path().to_path_buf());
    let local = sm.compute_manifest().unwrap();
    sm.update_manifest(local);

    let mut remote = SyncManifest::new();
    remote.files.insert(
        "new_remote.txt".to_string(),
        FileManifest {
            relative_path: "new_remote.txt".to_string(),
            blake3_hash: blake3::hash(b"from remote").to_string(),
            size: 10,
            modified: 0,
            chunks: vec![ChunkHash {
                offset: 0,
                length: 10,
                blake3_hash: blake3::hash(b"from remote").to_string(),
            }],
        },
    );

    let delta = sm.compute_delta(&remote);

    assert!(
        delta
            .iter()
            .any(|a| matches!(a, DeltaAction::Download(p) if p == "new_remote.txt")),
        "should detect remote-only file"
    );
    assert!(
        delta
            .iter()
            .any(|a| matches!(a, DeltaAction::Upload(p) if p == "keep.txt")),
        "should detect local-only file"
    );
}

#[test]
fn test_age_encrypt_decrypt_roundtrip_passphrase() {
    let dir = tempfile::tempdir().unwrap();
    let plaintext = b"integration test secret payload";
    let passphrase = "roundtrip-key-42";

    let input = dir.path().join("plain.bin");
    let encrypted = dir.path().join("cipher.age");
    let decrypted = dir.path().join("out.bin");

    std::fs::write(&input, plaintext).unwrap();
    encrypt_file(&input, &encrypted, passphrase).unwrap();
    decrypt_file(&encrypted, &decrypted, passphrase).unwrap();

    let result = std::fs::read(&decrypted).unwrap();
    assert_eq!(&result[..], plaintext);
}

#[test]
fn test_age_encrypt_decrypt_data_armored_roundtrip() {
    let data = b"armored integration test";
    let passphrase = "armor-pass";

    let armored = encrypt_data(data, passphrase).unwrap();
    assert!(armored.contains("BEGIN AGE ENCRYPTED FILE"));

    let result = decrypt_data(&armored, passphrase).unwrap();
    assert_eq!(&result[..], data);
}

#[test]
fn test_wrong_passphrase_fails_to_decrypt() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("secret.txt");
    let encrypted = dir.path().join("secret.age");
    let decrypted = dir.path().join("fail.txt");

    std::fs::write(&input, b"hidden").unwrap();
    encrypt_file(&input, &encrypted, "correct-passphrase").unwrap();

    let result = decrypt_file(&encrypted, &decrypted, "wrong-passphrase");
    assert!(
        result.is_err(),
        "decryption with wrong passphrase must fail"
    );
    assert!(
        !decrypted.exists(),
        "failed decryption must not write output file"
    );
}

#[test]
fn test_manifest_save_load_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("manifest.json");

    let mut manifest = SyncManifest::new();
    manifest.version = 3;
    manifest.last_sync = 99999;
    manifest.files.insert(
        "notes.md".to_string(),
        FileManifest {
            relative_path: "notes.md".to_string(),
            blake3_hash: blake3::hash(b"# notes").to_string(),
            size: 7,
            modified: 1000,
            chunks: vec![ChunkHash {
                offset: 0,
                length: 7,
                blake3_hash: blake3::hash(b"# notes").to_string(),
            }],
        },
    );
    manifest.files.insert(
        "deep/nested/data.json".to_string(),
        FileManifest {
            relative_path: "deep/nested/data.json".to_string(),
            blake3_hash: blake3::hash(b"{}").to_string(),
            size: 2,
            modified: 2000,
            chunks: vec![],
        },
    );

    manifest.save(&path).unwrap();
    let loaded = SyncManifest::load(&path).unwrap();

    assert_eq!(loaded.version, 3);
    assert_eq!(loaded.last_sync, 99999);
    assert_eq!(loaded.files.len(), 2);
    assert_eq!(loaded.files["notes.md"].size, 7);
    assert_eq!(
        loaded.files["notes.md"].blake3_hash,
        blake3::hash(b"# notes").to_string()
    );
    assert_eq!(loaded.files["notes.md"].chunks.len(), 1);
    assert_eq!(loaded.files["deep/nested/data.json"].chunks.len(), 0);
}

#[test]
fn test_sync_target_parse_ssh() {
    let target = SyncTarget::parse("deploy@backup.internal:/var/sync/aileron").unwrap();

    match &target {
        SyncTarget::Ssh {
            user_host,
            remote_path,
        } => {
            assert_eq!(user_host, "deploy@backup.internal");
            assert_eq!(remote_path, "/var/sync/aileron");
        }
        SyncTarget::Local(_) => panic!("expected SSH target"),
    }

    assert_eq!(target.display(), "deploy@backup.internal:/var/sync/aileron");
}

#[test]
fn test_sync_target_parse_local_path() {
    let target = SyncTarget::parse("/mnt/nas/aileron").unwrap();

    match &target {
        SyncTarget::Local(p) => assert_eq!(p.as_path(), std::path::Path::new("/mnt/nas/aileron")),
        SyncTarget::Ssh { .. } => panic!("expected Local target"),
    }

    assert_eq!(target.display(), "/mnt/nas/aileron");
}

#[test]
fn test_sync_target_parse_relative_local_path() {
    let target = SyncTarget::parse("./local-sync").unwrap();
    assert!(matches!(target, SyncTarget::Local(_)));
}

#[test]
fn test_sync_target_ssh_with_tilde() {
    let target = SyncTarget::parse("user@host:~/sync").unwrap();
    match &target {
        SyncTarget::Ssh {
            user_host,
            remote_path,
        } => {
            assert_eq!(user_host, "user@host");
            assert_eq!(remote_path, "~/sync");
        }
        SyncTarget::Local(_) => panic!("expected SSH target"),
    }
}
