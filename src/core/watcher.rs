use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, NoCache};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

use crate::core::error::{Result, VaultError};
use crate::core::write_echo::WriteEchoCache;

/// High-level events emitted by the vault filesystem watcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
}

/// Filesystem watcher running in the background.
///
/// It debounces incoming events, cross-references each event against the
/// `WriteEchoCache` to drop self-initiated changes, filters non-markdown files,
/// and sends clean `VaultEvent` messages to the UI and search indexer.
pub struct VaultWatcher {
    _debouncer: Debouncer<RecommendedWatcher, NoCache>,
}

impl VaultWatcher {
    pub fn start(
        vault_root: PathBuf,
        write_echo_cache: Arc<WriteEchoCache>,
        tx: UnboundedSender<VaultEvent>,
        debounce_duration: Duration,
    ) -> Result<Self> {
        let root_for_callback = vault_root.clone();

        let mut debouncer = new_debouncer(
            debounce_duration,
            None,
            move |result: DebounceEventResult| {
                match result {
                    Ok(events) => {
                        for event in events {
                            process_debounced_event(
                                &event,
                                &root_for_callback,
                                &write_echo_cache,
                                &tx,
                            );
                        }
                    }
                    Err(errors) => {
                        for error in errors {
                            tracing::warn!("Filesystem watcher error: {:?}", error);
                        }
                    }
                }
            },
        )
        .map_err(|e| VaultError::WatcherError(e.to_string()))?;

        debouncer
            .watch(&vault_root, RecursiveMode::Recursive)
            .map_err(|e| VaultError::WatcherError(e.to_string()))?;

        Ok(Self {
            _debouncer: debouncer,
        })
    }
}

fn process_debounced_event(
    event: &notify_debouncer_full::DebouncedEvent,
    vault_root: &Path,
    write_echo_cache: &WriteEchoCache,
    tx: &UnboundedSender<VaultEvent>,
) {
    use notify::EventKind;

    for path in &event.paths {
        // Filter out non-markdown files and hidden/sidecar paths
        if !is_relevant_markdown_path(path, vault_root) {
            continue;
        }

        // Check Write-Echo Cancellation
        // If this event was initiated by our own save/write/delete, drop it!
        if write_echo_cache.consume_echo(path, None) {
            tracing::debug!("Write-echo cancelled for: {:?}", path);
            continue;
        }

        if let Ok(rel_path) = path.strip_prefix(vault_root) {
            let rel_path = rel_path.to_path_buf();
            match event.kind {
                EventKind::Create(_) => {
                    let _ = tx.send(VaultEvent::Created(rel_path));
                }
                EventKind::Modify(_) => {
                    let _ = tx.send(VaultEvent::Modified(rel_path));
                }
                EventKind::Remove(_) => {
                    let _ = tx.send(VaultEvent::Deleted(rel_path));
                }
                EventKind::Any | EventKind::Other => {
                    // If file still exists on disk, treat as Modified, otherwise Deleted
                    if path.exists() {
                        let _ = tx.send(VaultEvent::Modified(rel_path));
                    } else {
                        let _ = tx.send(VaultEvent::Deleted(rel_path));
                    }
                }
                _ => {}
            }
        }
    }
}

/// Checks if a file path is a user note and not a hidden file or sidecar artifact.
pub fn is_relevant_markdown_path(path: &Path, vault_root: &Path) -> bool {
    // Only monitor `.md` files
    if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
        return false;
    }

    // Ignore hidden files and `.cosmic-notes/`
    if let Ok(rel) = path.strip_prefix(vault_root) {
        for comp in rel.components() {
            let s = comp.as_os_str().to_string_lossy();
            if s.starts_with('.') {
                return false;
            }
        }
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_is_relevant_markdown_path() {
        let root = Path::new("/vault");
        assert!(is_relevant_markdown_path(Path::new("/vault/note.md"), root));
        assert!(is_relevant_markdown_path(Path::new("/vault/sub/note.md"), root));
        assert!(!is_relevant_markdown_path(Path::new("/vault/image.png"), root));
        assert!(!is_relevant_markdown_path(Path::new("/vault/.git/note.md"), root));
        assert!(!is_relevant_markdown_path(Path::new("/vault/.cosmic-notes/cache/note.md"), root));
        assert!(!is_relevant_markdown_path(Path::new("/vault/.tmp_note.md"), root));
    }
}
