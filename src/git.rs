//! Git integration — branch detection and status for the status bar.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Git status information for the current directory.
#[derive(Debug, Clone, Default)]
pub struct GitStatus {
    /// Branch name (e.g., "main", "feature/foo").
    pub branch: Option<String>,
    /// Number of modified (staged + unstaged) files.
    pub modified_count: usize,
    /// Number of untracked files.
    pub untracked_count: usize,
    /// Whether there are uncommitted changes.
    pub is_dirty: bool,
}

impl GitStatus {
    /// Query git status for the given directory.
    /// Returns default (no branch, clean) if not a git repo or git fails.
    pub fn for_dir(dir: &Path) -> Self {
        let output = Command::new("git")
            .args([
                "-C",
                &dir.to_string_lossy(),
                "status",
                "--porcelain=v2",
                "--branch",
            ])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                Self::parse_porcelain_v2(&String::from_utf8_lossy(&out.stdout))
            }
            _ => Self::default(),
        }
    }

    /// Parse `git status --porcelain=v2 --branch` output.
    fn parse_porcelain_v2(output: &str) -> Self {
        let mut status = Self::default();
        for line in output.lines() {
            if let Some(rest) = line.strip_prefix("# branch.head ") {
                status.branch = Some(rest.trim().to_string());
            } else if line.starts_with('1') || line.starts_with('2') {
                status.modified_count += 1;
            } else if line.starts_with('?') {
                status.untracked_count += 1;
            }
        }
        status.is_dirty = status.modified_count > 0 || status.untracked_count > 0;
        status
    }

    /// Format for status bar display, e.g., "main *2" or "main" or "".
    pub fn status_bar_text(&self) -> String {
        match &self.branch {
            Some(branch) => {
                if self.is_dirty {
                    let total = self.modified_count + self.untracked_count;
                    format!("{} *{}", branch, total)
                } else {
                    branch.clone()
                }
            }
            None => String::new(),
        }
    }
}

/// Find the git repository root for the given directory.
/// Returns None if not inside a git repo.
pub fn repo_root(dir: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["-C", &dir.to_string_lossy(), "rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|out| out.status.success())?;

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_porcelain_v2_clean() {
        let output = "# branch.head main\n";
        let status = GitStatus::parse_porcelain_v2(output);
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert!(!status.is_dirty);
        assert_eq!(status.modified_count, 0);
        assert_eq!(status.untracked_count, 0);
    }

    #[test]
    fn test_parse_porcelain_v2_dirty() {
        let output = "# branch.head feature/foo\n1 .M N... 100644 100644 100644 sha1 sha2 src/main.rs\n? path/to/new.txt\n";
        let status = GitStatus::parse_porcelain_v2(output);
        assert_eq!(status.branch.as_deref(), Some("feature/foo"));
        assert!(status.is_dirty);
        assert_eq!(status.modified_count, 1);
        assert_eq!(status.untracked_count, 1);
    }

    #[test]
    fn test_parse_porcelain_v2_no_branch() {
        let output = "";
        let status = GitStatus::parse_porcelain_v2(output);
        assert!(status.branch.is_none());
        assert!(!status.is_dirty);
    }

    #[test]
    fn test_status_bar_text_clean() {
        let status = GitStatus {
            branch: Some("main".into()),
            ..Default::default()
        };
        assert_eq!(status.status_bar_text(), "main");
    }

    #[test]
    fn test_status_bar_text_dirty() {
        let status = GitStatus {
            branch: Some("develop".into()),
            modified_count: 2,
            untracked_count: 1,
            is_dirty: true,
        };
        assert_eq!(status.status_bar_text(), "develop *3");
    }

    #[test]
    fn test_status_bar_text_no_branch() {
        let status = GitStatus::default();
        assert_eq!(status.status_bar_text(), "");
    }

    #[test]
    fn test_for_dir_in_git_repo() {
        let status = GitStatus::for_dir(Path::new("."));
        assert!(status.branch.is_some());
    }

    #[test]
    fn test_repo_root_finds_root() {
        let root = repo_root(Path::new("."));
        assert!(root.is_some());
    }

    #[test]
    fn test_repo_root_not_a_repo() {
        let _ = repo_root(Path::new("/tmp"));
    }
}
