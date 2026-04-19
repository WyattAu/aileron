use std::path::{Path, PathBuf};
use tracing::warn;

use anyhow::Result;

#[derive(Debug, Clone)]
pub enum SyncTarget {
    Local(PathBuf),
    Ssh {
        user_host: String,
        remote_path: String,
    },
}

impl SyncTarget {
    pub fn parse(s: &str) -> Result<Self> {
        if s.contains('@') && s.contains(':') {
            let colon = s.find(':').unwrap();
            Ok(SyncTarget::Ssh {
                user_host: s[..colon].to_string(),
                remote_path: s[colon + 1..].to_string(),
            })
        } else {
            Ok(SyncTarget::Local(PathBuf::from(s)))
        }
    }

    pub fn display(&self) -> String {
        match self {
            SyncTarget::Local(p) => format!("{}", p.display()),
            SyncTarget::Ssh {
                user_host,
                remote_path,
            } => format!("{}:{}", user_host, remote_path),
        }
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
            let target_arg = format!("{}:{}", user_host, remote_path);

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
    }

    Ok(files_pushed.load(std::sync::atomic::Ordering::Relaxed))
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
            let source_arg = format!("{}:{}", user_host, remote_path);

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
    }

    Ok(files_pulled.load(std::sync::atomic::Ordering::Relaxed))
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
            SyncTarget::Local(_) => panic!("Expected SSH target"),
        }
    }

    #[test]
    fn test_parse_local_target() {
        let target = SyncTarget::parse("/mnt/backup").unwrap();
        match target {
            SyncTarget::Local(p) => assert_eq!(p, PathBuf::from("/mnt/backup")),
            SyncTarget::Ssh { .. } => panic!("Expected Local target"),
        }
    }

    #[test]
    fn test_parse_local_path_without_at() {
        let target = SyncTarget::parse("no-at-sign").unwrap();
        assert!(matches!(target, SyncTarget::Local(_)));
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
    }
}
