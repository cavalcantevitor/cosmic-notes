use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::core::error::{Result, VaultError};

/// Status report of a vault's git synchronization state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncStatus {
    pub is_git_repo: bool,
    pub branch: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub modified_files: Vec<PathBuf>,
    pub untracked_files: Vec<PathBuf>,
    pub is_clean: bool,
}

impl Default for SyncStatus {
    fn default() -> Self {
        Self {
            is_git_repo: false,
            branch: None,
            ahead: 0,
            behind: 0,
            modified_files: Vec::new(),
            untracked_files: Vec::new(),
            is_clean: true,
        }
    }
}

/// Metadata describing a committed snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitInfo {
    pub hash: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
}

/// Result summary of a pull or push synchronization action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncSummary {
    pub pulled_commits: usize,
    pub pushed_commits: usize,
    pub conflicts: Vec<PathBuf>,
    pub message: String,
}

/// Trait abstraction defining vault synchronization engines.
///
/// Designed to decouple vault business logic from specific git implementations,
/// allowing seamless swapping between CLI git, `gix`, or `git2`.
pub trait VaultSyncEngine: Send + Sync {
    /// Inspect the current git status of the vault directory.
    fn status(&self, vault_path: &Path) -> Result<SyncStatus>;

    /// Initialize a git repository in the vault directory if none exists.
    fn init_repo(&self, vault_path: &Path) -> Result<()>;

    /// Stage all modified and untracked markdown files in the vault.
    fn stage_all(&self, vault_path: &Path) -> Result<usize>;

    /// Create a new commit with the given message.
    fn commit(&self, vault_path: &Path, message: &str) -> Result<CommitInfo>;

    /// Pull upstream changes into the local vault.
    fn pull(&self, vault_path: &Path) -> Result<SyncSummary>;

    /// Push local commits to the upstream repository.
    fn push(&self, vault_path: &Path) -> Result<SyncSummary>;
}

/// Git implementation backed by the system `git` CLI executable.
#[derive(Debug, Clone, Default)]
pub struct GitCliSyncEngine;

impl GitCliSyncEngine {
    pub fn new() -> Self {
        Self
    }

    fn run_git(&self, vault_path: &Path, args: &[&str]) -> Result<String> {
        let output = Command::new("git")
            .current_dir(vault_path)
            .args(args)
            .output()
            .map_err(|e| VaultError::SyncError(format!("Failed to execute git: {e}")))?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(VaultError::SyncError(format!(
                "git {} failed: {}",
                args.join(" "),
                err.trim()
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

impl VaultSyncEngine for GitCliSyncEngine {
    fn status(&self, vault_path: &Path) -> Result<SyncStatus> {
        let git_dir = vault_path.join(".git");
        if !git_dir.exists() {
            return Ok(SyncStatus::default());
        }

        let branch_out = self.run_git(vault_path, &["rev-parse", "--abbrev-ref", "HEAD"])
            .unwrap_or_else(|_| "HEAD".to_string());
        let branch = Some(branch_out.trim().to_string());

        let status_out = self.run_git(vault_path, &["status", "--porcelain"])?;
        let mut modified = Vec::new();
        let mut untracked = Vec::new();

        for line in status_out.lines() {
            if line.len() < 4 {
                continue;
            }
            let code = &line[0..2];
            let path_str = line[3..].trim();
            let path = PathBuf::from(path_str);

            if code.contains('?') {
                untracked.push(path);
            } else {
                modified.push(path);
            }
        }

        let is_clean = modified.is_empty() && untracked.is_empty();

        Ok(SyncStatus {
            is_git_repo: true,
            branch,
            ahead: 0,
            behind: 0,
            modified_files: modified,
            untracked_files: untracked,
            is_clean,
        })
    }

    fn init_repo(&self, vault_path: &Path) -> Result<()> {
        self.run_git(vault_path, &["init"])?;
        Ok(())
    }

    fn stage_all(&self, vault_path: &Path) -> Result<usize> {
        let status = self.status(vault_path)?;
        let count = status.modified_files.len() + status.untracked_files.len();
        self.run_git(vault_path, &["add", "."])?;
        Ok(count)
    }

    fn commit(&self, vault_path: &Path, message: &str) -> Result<CommitInfo> {
        self.run_git(vault_path, &["commit", "-m", message])?;
        let hash = self.run_git(vault_path, &["rev-parse", "HEAD"])?
            .trim()
            .to_string();

        Ok(CommitInfo {
            hash,
            message: message.to_string(),
            timestamp: Utc::now(),
        })
    }

    fn pull(&self, vault_path: &Path) -> Result<SyncSummary> {
        let res = self.run_git(vault_path, &["pull"]);
        match res {
            Ok(msg) => Ok(SyncSummary {
                pulled_commits: 1,
                pushed_commits: 0,
                conflicts: Vec::new(),
                message: msg.trim().to_string(),
            }),
            Err(e) => Err(e),
        }
    }

    fn push(&self, vault_path: &Path) -> Result<SyncSummary> {
        let res = self.run_git(vault_path, &["push"]);
        match res {
            Ok(msg) => Ok(SyncSummary {
                pulled_commits: 0,
                pushed_commits: 1,
                conflicts: Vec::new(),
                message: msg.trim().to_string(),
            }),
            Err(e) => Err(e),
        }
    }
}

/// Simulated in-memory sync engine for unit testing and offline development.
#[derive(Debug, Clone, Default)]
pub struct MockSyncEngine {
    pub is_repo: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub commits: std::sync::Arc<std::sync::Mutex<Vec<CommitInfo>>>,
}

impl MockSyncEngine {
    pub fn new() -> Self {
        Self {
            is_repo: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
            commits: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }
}

impl VaultSyncEngine for MockSyncEngine {
    fn status(&self, _vault_path: &Path) -> Result<SyncStatus> {
        let is_repo = self.is_repo.load(std::sync::atomic::Ordering::SeqCst);
        Ok(SyncStatus {
            is_git_repo: is_repo,
            branch: if is_repo { Some("main".to_string()) } else { None },
            ahead: 0,
            behind: 0,
            modified_files: Vec::new(),
            untracked_files: Vec::new(),
            is_clean: true,
        })
    }

    fn init_repo(&self, _vault_path: &Path) -> Result<()> {
        self.is_repo.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    fn stage_all(&self, _vault_path: &Path) -> Result<usize> {
        Ok(0)
    }

    fn commit(&self, _vault_path: &Path, message: &str) -> Result<CommitInfo> {
        let info = CommitInfo {
            hash: format!("mock-{}", Utc::now().timestamp_millis()),
            message: message.to_string(),
            timestamp: Utc::now(),
        };
        self.commits.lock().unwrap().push(info.clone());
        Ok(info)
    }

    fn pull(&self, _vault_path: &Path) -> Result<SyncSummary> {
        Ok(SyncSummary {
            pulled_commits: 0,
            pushed_commits: 0,
            conflicts: Vec::new(),
            message: "Already up to date.".to_string(),
        })
    }

    fn push(&self, _vault_path: &Path) -> Result<SyncSummary> {
        Ok(SyncSummary {
            pulled_commits: 0,
            pushed_commits: 1,
            conflicts: Vec::new(),
            message: "Push successful.".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_mock_sync_engine_lifecycle() {
        let engine = MockSyncEngine::new();
        let dir = tempdir().unwrap();

        let status = engine.status(dir.path()).unwrap();
        assert!(status.is_git_repo);
        assert_eq!(status.branch.as_deref(), Some("main"));

        let commit = engine.commit(dir.path(), "test commit").unwrap();
        assert_eq!(commit.message, "test commit");
        assert!(commit.hash.starts_with("mock-"));

        let pull = engine.pull(dir.path()).unwrap();
        assert_eq!(pull.conflicts.len(), 0);

        let push = engine.push(dir.path()).unwrap();
        assert_eq!(push.pushed_commits, 1);
    }
}
