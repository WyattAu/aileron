use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileManifest {
    pub relative_path: String,
    pub blake3_hash: String,
    pub size: u64,
    pub modified: u64,
    pub chunks: Vec<ChunkHash>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChunkHash {
    pub offset: usize,
    pub length: usize,
    pub blake3_hash: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct SyncManifest {
    pub version: u32,
    pub files: BTreeMap<String, FileManifest>,
    pub last_sync: u64,
}

impl SyncManifest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load(path: &Path) -> Result<Self, anyhow::Error> {
        let data = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&data)?)
    }

    pub fn save(&self, path: &Path) -> Result<(), anyhow::Error> {
        let data = serde_json::to_string_pretty(self)?;
        std::fs::write(path, data)?;
        Ok(())
    }
}

pub struct SyncManager {
    local_dir: PathBuf,
    state_dir: PathBuf,
    manifest: RwLock<SyncManifest>,
    syncing: Mutex<bool>,
}

impl SyncManager {
    pub fn new(local_dir: PathBuf) -> Self {
        let state_dir = local_dir.join(".sync");
        std::fs::create_dir_all(&state_dir).ok();

        let manifest_path = state_dir.join("manifest.json");
        let manifest = if manifest_path.exists() {
            SyncManifest::load(&manifest_path).unwrap_or_default()
        } else {
            SyncManifest::new()
        };

        Self {
            local_dir,
            state_dir,
            manifest: RwLock::new(manifest),
            syncing: Mutex::new(false),
        }
    }

    pub fn local_dir(&self) -> &Path {
        &self.local_dir
    }

    pub fn create_db_snapshots(&self) -> Result<HashMap<String, PathBuf>, anyhow::Error> {
        let mut snapshots = HashMap::new();
        let staging = self.state_dir.join("staging");
        std::fs::create_dir_all(&staging)?;

        for entry in walk_files(&self.local_dir) {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "db") {
                let relative = path.strip_prefix(&self.local_dir)?.to_path_buf();
                let staging_path = staging.join(&relative);
                if let Some(parent) = staging_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let src = rusqlite::Connection::open(&path)?;
                let mut dst = rusqlite::Connection::open(&staging_path)?;
                let backup = rusqlite::backup::Backup::new(&src, &mut dst)?;
                backup.step(-1)?;

                snapshots.insert(relative.to_string_lossy().to_string(), staging_path);
            }
        }
        Ok(snapshots)
    }

    pub fn compute_manifest(&self) -> Result<SyncManifest, anyhow::Error> {
        let mut manifest = SyncManifest::new();
        manifest.version = 1;

        for entry in walk_files(&self.local_dir) {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "db") {
                continue;
            }
            if path.starts_with(&self.state_dir) {
                continue;
            }

            let relative = path.strip_prefix(&self.local_dir)?;
            let metadata = entry.metadata()?;
            let data = std::fs::read(&path)?;

            let hash = blake3::hash(&data);

            let chunks = if data.is_empty() {
                vec![]
            } else if data.len() < 1024 {
                vec![ChunkHash {
                    offset: 0,
                    length: data.len(),
                    blake3_hash: blake3::hash(&data).to_string(),
                }]
            } else {
                let chunker = fastcdc::v2020::FastCDC::new(&data, 1024, 4096, 65536);
                chunker
                    .map(|chunk| ChunkHash {
                        offset: chunk.offset,
                        length: chunk.length,
                        blake3_hash: blake3::hash(&data[chunk.offset..chunk.offset + chunk.length])
                            .to_string(),
                    })
                    .collect()
            };

            let modified = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            manifest.files.insert(
                relative.to_string_lossy().to_string(),
                FileManifest {
                    relative_path: relative.to_string_lossy().to_string(),
                    blake3_hash: hash.to_string(),
                    size: metadata.len(),
                    modified,
                    chunks,
                },
            );
        }

        Ok(manifest)
    }

    pub fn compute_delta(&self, remote: &SyncManifest) -> Vec<DeltaAction> {
        let local = self.manifest.read().unwrap_or_else(|e| e.into_inner());
        let mut actions = Vec::new();

        for (path, local_entry) in &local.files {
            match remote.files.get(path) {
                None => actions.push(DeltaAction::Upload(path.clone())),
                Some(remote_entry) if remote_entry.blake3_hash != local_entry.blake3_hash => {
                    let changed_chunks: Vec<_> = local_entry
                        .chunks
                        .iter()
                        .filter(|c| {
                            !remote_entry
                                .chunks
                                .iter()
                                .any(|rc| rc.blake3_hash == c.blake3_hash)
                        })
                        .cloned()
                        .collect();
                    if !changed_chunks.is_empty() {
                        actions.push(DeltaAction::UploadChunks(path.clone(), changed_chunks));
                    }
                }
                Some(_) => {}
            }
        }

        for path in remote.files.keys() {
            if !local.files.contains_key(path) {
                actions.push(DeltaAction::Download(path.clone()));
            }
        }

        for path in local.files.keys() {
            if !remote.files.contains_key(path) {
                actions.push(DeltaAction::DeleteLocal(path.clone()));
            }
        }

        actions
    }

    pub fn update_manifest(&self, manifest: SyncManifest) {
        let mut current = self.manifest.write().unwrap_or_else(|e| e.into_inner());
        *current = manifest;
    }

    pub fn save_manifest(&self) -> Result<(), anyhow::Error> {
        let manifest = self.manifest.read().unwrap_or_else(|e| e.into_inner());
        let path = self.state_dir.join("manifest.json");
        manifest.save(&path)
    }

    pub fn is_syncing(&self) -> bool {
        *self.syncing.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }
}

#[derive(Debug, Clone)]
pub enum DeltaAction {
    Upload(String),
    UploadChunks(String, Vec<ChunkHash>),
    Download(String),
    DeleteLocal(String),
}

fn walk_files(dir: &Path) -> Vec<std::fs::DirEntry> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                files.extend(walk_files(&entry.path()));
            } else if entry.file_type().is_ok_and(|t| t.is_file()) {
                files.push(entry);
            }
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_file(dir: &Path, name: &str, content: &[u8]) -> PathBuf {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).ok();
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn test_manifest_save_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");

        let mut manifest = SyncManifest::new();
        manifest.version = 1;
        manifest.last_sync = 12345;
        manifest.files.insert(
            "test.txt".to_string(),
            FileManifest {
                relative_path: "test.txt".to_string(),
                blake3_hash: blake3::hash(b"hello").to_string(),
                size: 5,
                modified: 1000,
                chunks: vec![],
            },
        );

        manifest.save(&path).unwrap();
        let loaded = SyncManifest::load(&path).unwrap();
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.last_sync, 12345);
        assert_eq!(loaded.files.len(), 1);
        assert_eq!(loaded.files["test.txt"].size, 5);
    }

    #[test]
    fn test_compute_manifest() {
        let dir = tempfile::tempdir().unwrap();
        make_file(dir.path(), "config.toml", b"homepage = \"test\"");
        make_file(dir.path(), "sub/data.json", b"{\"key\": 42}");
        make_file(dir.path(), "empty.txt", b"");

        let sm = SyncManager::new(dir.path().to_path_buf());
        let manifest = sm.compute_manifest().unwrap();

        assert_eq!(manifest.files.len(), 3);
        assert!(manifest.files.contains_key("config.toml"));
        assert!(manifest.files.contains_key("sub/data.json"));
        assert!(manifest.files.contains_key("empty.txt"));

        let config_entry = &manifest.files["config.toml"];
        assert_eq!(config_entry.size, 17);
        assert_eq!(
            config_entry.blake3_hash,
            blake3::hash(b"homepage = \"test\"").to_string()
        );
    }

    #[test]
    fn test_delta_new_file() {
        let mut remote = SyncManifest::new();
        remote.files.insert(
            "new.txt".to_string(),
            FileManifest {
                relative_path: "new.txt".to_string(),
                blake3_hash: "abc".to_string(),
                size: 10,
                modified: 0,
                chunks: vec![],
            },
        );

        let sm = SyncManager::new(tempfile::tempdir().unwrap().path().to_path_buf());
        let actions = sm.compute_delta(&remote);

        assert!(
            actions
                .iter()
                .any(|a| matches!(a, DeltaAction::Download(p) if p == "new.txt"))
        );
    }

    #[test]
    fn test_delta_deleted_file() {
        let mut local = SyncManifest::new();
        local.files.insert(
            "old.txt".to_string(),
            FileManifest {
                relative_path: "old.txt".to_string(),
                blake3_hash: "abc".to_string(),
                size: 10,
                modified: 0,
                chunks: vec![],
            },
        );
        let remote = SyncManifest::new();

        let sm = SyncManager::new(tempfile::tempdir().unwrap().path().to_path_buf());
        sm.update_manifest(local);
        let actions = sm.compute_delta(&remote);

        assert!(
            actions
                .iter()
                .any(|a| matches!(a, DeltaAction::DeleteLocal(p) if p == "old.txt"))
        );
    }

    #[test]
    fn test_delta_unchanged() {
        let mut manifest = SyncManifest::new();
        let entry = FileManifest {
            relative_path: "same.txt".to_string(),
            blake3_hash: blake3::hash(b"same").to_string(),
            size: 4,
            modified: 0,
            chunks: vec![],
        };
        manifest.files.insert("same.txt".to_string(), entry.clone());

        let sm = SyncManager::new(tempfile::tempdir().unwrap().path().to_path_buf());
        sm.update_manifest(manifest.clone());

        let actions = sm.compute_delta(&manifest);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_chunk_hashing_small_file() {
        let dir = tempfile::tempdir().unwrap();
        make_file(dir.path(), "small.txt", b"hi");

        let sm = SyncManager::new(dir.path().to_path_buf());
        let manifest = sm.compute_manifest().unwrap();

        let entry = &manifest.files["small.txt"];
        assert_eq!(entry.chunks.len(), 1);
        assert_eq!(entry.chunks[0].offset, 0);
        assert_eq!(entry.chunks[0].length, 2);
    }

    #[test]
    fn test_chunk_hashing_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        make_file(dir.path(), "empty.txt", b"");

        let sm = SyncManager::new(dir.path().to_path_buf());
        let manifest = sm.compute_manifest().unwrap();

        let entry = &manifest.files["empty.txt"];
        assert!(entry.chunks.is_empty());
    }
}
