use std::path::{Path, PathBuf};
use tracing::warn;

use anyhow::Result;

use super::webdav::{WebdavAuth, WebdavClient, WebdavConfig};

#[derive(Debug, Clone)]
pub enum SyncTarget {
    Local(PathBuf),
    Ssh {
        user_host: String,
        remote_path: String,
    },
    WebDav {
        base_url: String,
        auth: WebdavAuth,
    },
}

impl SyncTarget {
    #[must_use = "ignoring this value may lead to data loss or unexpected behavior"]
    pub fn parse(s: &str) -> Result<Self> {
        if s.starts_with("http://") || s.starts_with("https://") {
            let url = s.to_string();
            Ok(SyncTarget::WebDav {
                base_url: url,
                auth: WebdavAuth::None,
            })
        } else if s.contains('@') && s.contains(':') {
            let colon = s.find(':').expect("guarded by contains(':') check");
            Ok(SyncTarget::Ssh {
                user_host: s[..colon].to_string(),
                remote_path: s[colon + 1..].to_string(),
            })
        } else {
            Ok(SyncTarget::Local(PathBuf::from(s)))
        }
    }

    #[must_use = "ignoring this value may lead to data loss or unexpected behavior"]
    pub fn with_auth(self, auth: WebdavAuth) -> Self {
        match self {
            SyncTarget::WebDav { base_url, .. } => SyncTarget::WebDav { base_url, auth },
            _ => self,
        }
    }

    pub fn display(&self) -> String {
        match self {
            SyncTarget::Local(p) => format!("{}", p.display()),
            SyncTarget::Ssh {
                user_host,
                remote_path,
            } => format!("{user_host}:{remote_path}"),
            SyncTarget::WebDav { base_url, .. } => base_url.clone(),
        }
    }

    pub fn is_webdav(&self) -> bool {
        matches!(self, SyncTarget::WebDav { .. })
    }
}

pub fn push(
    _local_dir: &Path,
    staging_dir: &Path,
    target: &SyncTarget,
    _encrypted: bool,
) -> Result<u64> {
    let files_pushed = std::sync::atomic::AtomicU64::new(0);

    match target {
        SyncTarget::Local(remote_dir) => {
            copy_dir_recursive(staging_dir, remote_dir, &files_pushed)?;
        }
        SyncTarget::Ssh {
            user_host,
            remote_path,
        } => {
            let target_arg = format!("{user_host}:{remote_path}");

            let status = std::process::Command::new("ssh")
                .args([user_host.as_str(), "mkdir", "-p", remote_path.as_str()])
                .status()?;
            if !status.success() {
                warn!("Failed to create remote directory (non-fatal)");
            }

            let status = std::process::Command::new("scp")
                .args([
                    "-r",
                    staging_dir.to_str().unwrap_or(""),
                    target_arg.as_str(),
                ])
                .status()?;
            if !status.success() {
                return Err(anyhow::anyhow!(
                    "scp failed with exit code {:?}",
                    status.code()
                ));
            }
        }
        SyncTarget::WebDav { base_url, auth } => {
            let config = WebdavConfig {
                base_url: base_url.clone(),
                auth: auth.clone(),
                max_retries: 3,
                retry_delay_ms: 1000,
                max_retry_delay_ms: 30000,
            };
            let client = WebdavClient::new(config);
            push_webdav(staging_dir, &client, &files_pushed)?;
        }
    }

    Ok(files_pushed.load(std::sync::atomic::Ordering::Relaxed))
}

fn push_webdav(
    staging_dir: &Path,
    client: &WebdavClient,
    counter: &std::sync::atomic::AtomicU64,
) -> Result<()> {
    if !staging_dir.exists() {
        return Ok(());
    }

    if staging_dir.is_file() {
        let remote_path = staging_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        client.upload_file(staging_dir, &remote_path)?;
        counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        return Ok(());
    }

    for entry in std::fs::read_dir(staging_dir)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(staging_dir).unwrap_or(&path);

        if path.is_dir() {
            let dir_name = relative.to_string_lossy();
            client.ensure_collection(&dir_name)?;
            push_webdav_recursive(&path, client, &dir_name, counter)?;
        } else if path.is_file() {
            let remote_path = relative.to_string_lossy().to_string();
            client.upload_file(&path, &remote_path)?;
            counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    Ok(())
}

fn push_webdav_recursive(
    dir: &Path,
    client: &WebdavClient,
    parent_remote: &str,
    counter: &std::sync::atomic::AtomicU64,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = path.file_name().unwrap_or_default().to_string_lossy();

        if path.is_dir() {
            let remote_path = format!("{parent_remote}/{file_name}");
            client.ensure_collection(&remote_path)?;
            push_webdav_recursive(&path, client, &remote_path, counter)?;
        } else if path.is_file() {
            let remote_path = format!("{parent_remote}/{file_name}");
            client.upload_file(&path, &remote_path)?;
            counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    Ok(())
}

pub fn pull(
    _local_dir: &Path,
    staging_dir: &Path,
    target: &SyncTarget,
    _encrypted: bool,
) -> Result<u64> {
    let files_pulled = std::sync::atomic::AtomicU64::new(0);

    match target {
        SyncTarget::Local(remote_dir) => {
            copy_dir_recursive(remote_dir, staging_dir, &files_pulled)?;
        }
        SyncTarget::Ssh {
            user_host,
            remote_path,
        } => {
            let source_arg = format!("{user_host}:{remote_path}");

            let status = std::process::Command::new("scp")
                .args([
                    "-r",
                    source_arg.as_str(),
                    staging_dir.to_str().unwrap_or(""),
                ])
                .status()?;
            if !status.success() {
                return Err(anyhow::anyhow!(
                    "scp failed with exit code {:?}",
                    status.code()
                ));
            }
        }
        SyncTarget::WebDav { base_url, auth } => {
            let config = WebdavConfig {
                base_url: base_url.clone(),
                auth: auth.clone(),
                max_retries: 3,
                retry_delay_ms: 1000,
                max_retry_delay_ms: 30000,
            };
            let client = WebdavClient::new(config);
            pull_webdav(staging_dir, &client, &files_pulled)?;
        }
    }

    Ok(files_pulled.load(std::sync::atomic::Ordering::Relaxed))
}

fn pull_webdav(
    staging_dir: &Path,
    client: &WebdavClient,
    counter: &std::sync::atomic::AtomicU64,
) -> Result<()> {
    std::fs::create_dir_all(staging_dir)?;

    let files = client.propfind("")?;
    for file_info in &files {
        if file_info.is_directory {
            continue;
        }

        let href = file_info.href.trim_start_matches('/');
        if href.is_empty() {
            continue;
        }

        if let Some(data) = client.get(href)? {
            let local_path = staging_dir.join(href);
            if let Some(parent) = local_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&local_path, &data)?;
            counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    Ok(())
}

fn copy_dir_recursive(
    src: &Path,
    dst: &Path,
    counter: &std::sync::atomic::AtomicU64,
) -> Result<()> {
    if !src.exists() {
        return Ok(());
    }

    if src.is_file() {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dst)?;
        counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        return Ok(());
    }

    if src.is_dir() {
        if !dst.exists() {
            std::fs::create_dir_all(dst)?;
        }
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            copy_dir_recursive(&src_path, &dst_path, counter)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ssh_target() {
        let target = SyncTarget::parse("user@server:~/.config/aileron/").unwrap();
        match target {
            SyncTarget::Ssh {
                user_host,
                remote_path,
            } => {
                assert_eq!(user_host, "user@server");
                assert_eq!(remote_path, "~/.config/aileron/");
            }
            _ => panic!("Expected SSH target"),
        }
    }

    #[test]
    fn test_parse_local_target() {
        let target = SyncTarget::parse("/mnt/backup").unwrap();
        match target {
            SyncTarget::Local(p) => assert_eq!(p, PathBuf::from("/mnt/backup")),
            _ => panic!("Expected Local target"),
        }
    }

    #[test]
    fn test_parse_local_path_without_at() {
        let target = SyncTarget::parse("no-at-sign").unwrap();
        assert!(matches!(target, SyncTarget::Local(_)));
    }

    #[test]
    fn test_parse_webdav_target() {
        let target = SyncTarget::parse("https://dav.example.com/aileron").unwrap();
        match target {
            SyncTarget::WebDav { base_url, auth } => {
                assert_eq!(base_url, "https://dav.example.com/aileron");
                assert!(matches!(auth, WebdavAuth::None));
            }
            _ => panic!("Expected WebDav target"),
        }
    }

    #[test]
    fn test_parse_webdav_http() {
        let target = SyncTarget::parse("http://localhost:8080/dav").unwrap();
        assert!(matches!(target, SyncTarget::WebDav { .. }));
    }

    #[test]
    fn test_sync_target_display() {
        let local = SyncTarget::Local(PathBuf::from("/mnt/backup"));
        assert_eq!(local.display(), "/mnt/backup");

        let ssh = SyncTarget::Ssh {
            user_host: "user@host".to_string(),
            remote_path: "~/data".to_string(),
        };
        assert_eq!(ssh.display(), "user@host:~/data");

        let webdav = SyncTarget::WebDav {
            base_url: "https://dav.example.com/aileron".to_string(),
            auth: WebdavAuth::None,
        };
        assert_eq!(webdav.display(), "https://dav.example.com/aileron");
    }

    #[test]
    fn test_webdav_with_auth() {
        let target = SyncTarget::WebDav {
            base_url: "https://dav.example.com".to_string(),
            auth: WebdavAuth::None,
        };
        let target = target.with_auth(WebdavAuth::Basic {
            username: "user".to_string(),
            password: "pass".to_string(),
        });
        match target {
            SyncTarget::WebDav { auth, .. } => {
                assert!(matches!(auth, WebdavAuth::Basic { .. }));
            }
            _ => panic!("Expected WebDav"),
        }
    }

    #[test]
    fn test_is_webdav() {
        let webdav = SyncTarget::WebDav {
            base_url: "https://dav.example.com".to_string(),
            auth: WebdavAuth::None,
        };
        assert!(webdav.is_webdav());

        let local = SyncTarget::Local(PathBuf::from("/tmp"));
        assert!(!local.is_webdav());
    }
}
