use chrono::{DateTime, Utc};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use walkdir::WalkDir;

use crate::core::error::{Result, VaultError};
use crate::core::note::{Note, calculate_content_hash};
use crate::core::write_echo::WriteEchoCache;

pub const SIDECAR_DIR: &str = ".cosmic-notes";
pub const SIDECAR_GITIGNORE: &str = "# Ignore transient indexes and caches\nindex/\ncache/\n*.tmp\n";

/// Headless local-first Vault Engine.
///
/// Encapsulates all disk storage, note scanning, atomic file saving with
/// write-echo cancellation token registration, and safe trash deletion.
#[derive(Debug, Clone)]
pub struct Vault {
    root_path: PathBuf,
    write_echo_cache: Arc<WriteEchoCache>,
}

impl Vault {
    /// Open or initialize a vault at the given root directory.
    pub fn open(root_path: impl AsRef<Path>) -> Result<Self> {
        let root = root_path.as_ref().to_path_buf();
        if !root.exists() {
            fs::create_dir_all(&root).map_err(|e| VaultError::Io {
                path: root.clone(),
                source: e,
            })?;
        }

        let vault = Self {
            root_path: root,
            write_echo_cache: Arc::new(WriteEchoCache::default()),
        };

        vault.ensure_sidecar_initialized()?;

        Ok(vault)
    }

    /// Access the write-echo cancellation cache.
    pub fn write_echo_cache(&self) -> &Arc<WriteEchoCache> {
        &self.write_echo_cache
    }

    /// Get the absolute root path of this vault.
    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    /// Ensure `.cosmic-notes/` sidecar folder and its `.gitignore` are present.
    pub fn ensure_sidecar_initialized(&self) -> Result<()> {
        let sidecar_dir = self.root_path.join(SIDECAR_DIR);
        if !sidecar_dir.exists() {
            fs::create_dir_all(&sidecar_dir).map_err(|e| VaultError::Io {
                path: sidecar_dir.clone(),
                source: e,
            })?;
        }

        let gitignore_path = sidecar_dir.join(".gitignore");
        if !gitignore_path.exists() {
            fs::write(&gitignore_path, SIDECAR_GITIGNORE).map_err(|e| VaultError::Io {
                path: gitignore_path,
                source: e,
            })?;
        }

        Ok(())
    }

    /// Convert a relative note path to an absolute path inside this vault.
    pub fn resolve_path(&self, relative_path: &Path) -> PathBuf {
        self.root_path.join(relative_path)
    }

    /// Check if a path is relative and stays within the vault directory bounds.
    fn validate_relative_path(&self, relative_path: &Path) -> Result<PathBuf> {
        if relative_path.is_absolute() {
            if let Ok(rel) = relative_path.strip_prefix(&self.root_path) {
                return Ok(rel.to_path_buf());
            } else {
                return Err(VaultError::InvalidPath(format!(
                    "Path '{:?}' is outside vault root '{:?}'",
                    relative_path, self.root_path
                )));
            }
        }

        // Avoid directory traversal attacks (e.g. `../`)
        for component in relative_path.components() {
            if component == std::path::Component::ParentDir {
                return Err(VaultError::InvalidPath(format!(
                    "Path '{:?}' attempts parent directory traversal",
                    relative_path
                )));
            }
        }

        Ok(relative_path.to_path_buf())
    }

    /// Recursively scan all `.md` files in the vault.
    pub fn scan_notes(&self) -> Result<Vec<Note>> {
        let mut notes = Vec::new();

        let walker = WalkDir::new(&self.root_path)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| {
                let name = entry.file_name().to_string_lossy();
                // Skip hidden files/directories (.git, .cosmic-notes, etc.)
                if name.starts_with('.') && entry.depth() > 0 {
                    return false;
                }
                true
            });

        for entry in walker {
            let entry = entry.map_err(|e| VaultError::Io {
                path: self.root_path.clone(),
                source: e.into(),
            })?;

            if entry.file_type().is_file() {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
                    if let Ok(rel_path) = path.strip_prefix(&self.root_path) {
                        match self.read_note(rel_path) {
                            Ok(note) => notes.push(note),
                            Err(e) => {
                                tracing::warn!("Failed to read note at '{:?}': {}", path, e);
                            }
                        }
                    }
                }
            }
        }

        // Sort by path for consistent ordering
        notes.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(notes)
    }

    /// Read and parse a single note by its relative path.
    pub fn read_note(&self, relative_path: &Path) -> Result<Note> {
        let rel_path = self.validate_relative_path(relative_path)?;
        let abs_path = self.resolve_path(&rel_path);

        if !abs_path.is_file() {
            return Err(VaultError::NoteNotFound { path: abs_path });
        }

        let raw_content = fs::read_to_string(&abs_path).map_err(|e| VaultError::Io {
            path: abs_path.clone(),
            source: e,
        })?;

        let metadata = fs::metadata(&abs_path).map_err(|e| VaultError::Io {
            path: abs_path.clone(),
            source: e,
        })?;

        let modified_at = metadata
            .modified()
            .map(|t| DateTime::<Utc>::from(t))
            .unwrap_or_else(|_| Utc::now());

        Note::parse(rel_path, abs_path, raw_content, modified_at)
    }

    /// Atomically write/save a note and register its token in the write-echo cache.
    pub fn write_note(&self, relative_path: &Path, content: &str) -> Result<Note> {
        let rel_path = self.validate_relative_path(relative_path)?;
        let abs_path = self.resolve_path(&rel_path);

        // Ensure parent directory exists
        if let Some(parent) = abs_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| VaultError::Io {
                    path: parent.to_path_buf(),
                    source: e,
                })?;
            }
        }

        let content_hash = calculate_content_hash(content);

        // Register write-echo cancellation token before writing to disk
        self.write_echo_cache
            .register(&abs_path, Some(content_hash));

        // Atomic write: write to temporary file, sync, then atomically replace
        let temp_filename = format!(
            ".tmp_{}_{}",
            std::process::id(),
            abs_path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "note".to_string())
        );
        let temp_path = abs_path
            .parent()
            .unwrap_or(&self.root_path)
            .join(temp_filename);

        fs::write(&temp_path, content).map_err(|e| VaultError::Io {
            path: temp_path.clone(),
            source: e,
        })?;

        fs::rename(&temp_path, &abs_path).map_err(|e| VaultError::Io {
            path: abs_path.clone(),
            source: e,
        })?;

        let modified_at = Utc::now();
        Note::parse(rel_path, abs_path, content.to_string(), modified_at)
    }

    /// Safely delete a note by moving it to the system trash bin.
    pub fn delete_note(&self, relative_path: &Path) -> Result<()> {
        let rel_path = self.validate_relative_path(relative_path)?;
        let abs_path = self.resolve_path(&rel_path);

        if !abs_path.exists() {
            return Err(VaultError::NoteNotFound { path: abs_path });
        }

        // Register echo token for deletion so the watcher can ignore the removal echo if needed
        self.write_echo_cache.register(&abs_path, None);

        trash::delete(&abs_path).map_err(|e| VaultError::TrashError {
            path: abs_path,
            message: e.to_string(),
        })?;

        Ok(())
    }

    /// Rename or move a note within the vault.
    pub fn rename_note(&self, from_rel: &Path, to_rel: &Path) -> Result<Note> {
        let from_rel = self.validate_relative_path(from_rel)?;
        let to_rel = self.validate_relative_path(to_rel)?;

        let from_abs = self.resolve_path(&from_rel);
        let to_abs = self.resolve_path(&to_rel);

        if !from_abs.is_file() {
            return Err(VaultError::NoteNotFound { path: from_abs });
        }

        if let Some(parent) = to_abs.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| VaultError::Io {
                    path: parent.to_path_buf(),
                    source: e,
                })?;
            }
        }

        // Register echo tokens for both source and target
        self.write_echo_cache.register(&from_abs, None);
        self.write_echo_cache.register(&to_abs, None);

        fs::rename(&from_abs, &to_abs).map_err(|e| VaultError::Io {
            path: to_abs.clone(),
            source: e,
        })?;

        self.read_note(&to_rel)
    }

    /// Create a new folder on disk within the vault.
    pub fn create_folder(&self, relative_path: &Path) -> Result<PathBuf> {
        let rel_path = self.validate_relative_path(relative_path)?;
        let abs_path = self.resolve_path(&rel_path);

        if !abs_path.exists() {
            fs::create_dir_all(&abs_path).map_err(|e| VaultError::Io {
                path: abs_path.clone(),
                source: e,
            })?;
        }

        Ok(rel_path)
    }

    /// Rename an existing directory within the vault.
    pub fn rename_folder(&self, from_rel: &Path, to_rel: &Path) -> Result<()> {
        let from_rel = self.validate_relative_path(from_rel)?;
        let to_rel = self.validate_relative_path(to_rel)?;

        let from_abs = self.resolve_path(&from_rel);
        let to_abs = self.resolve_path(&to_rel);

        if !from_abs.is_dir() {
            return Err(VaultError::NoteNotFound { path: from_abs });
        }

        if let Some(parent) = to_abs.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| VaultError::Io {
                    path: parent.to_path_buf(),
                    source: e,
                })?;
            }
        }

        fs::rename(&from_abs, &to_abs).map_err(|e| VaultError::Io {
            path: to_abs,
            source: e,
        })?;

        Ok(())
    }

    /// Safely delete a directory and all contained files by moving to system trash.
    pub fn delete_folder(&self, relative_path: &Path) -> Result<()> {
        let rel_path = self.validate_relative_path(relative_path)?;
        let abs_path = self.resolve_path(&rel_path);

        if !abs_path.exists() {
            return Err(VaultError::NoteNotFound { path: abs_path });
        }

        trash::delete(&abs_path).map_err(|e| VaultError::TrashError {
            path: abs_path,
            message: e.to_string(),
        })?;

        Ok(())
    }

    /// Scan all directories within the vault (excluding sidecar and hidden dot-dirs).
    pub fn scan_folders(&self) -> Result<Vec<PathBuf>> {
        let mut folders = Vec::new();

        for entry in WalkDir::new(&self.root_path)
            .min_depth(1)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !name.starts_with('.') && name != SIDECAR_DIR
            })
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_dir() {
                if let Ok(rel) = entry.path().strip_prefix(&self.root_path) {
                    folders.push(rel.to_path_buf());
                }
            }
        }

        folders.sort();
        Ok(folders)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_vault_init_and_sidecar() {
        let dir = tempdir().unwrap();
        let _vault = Vault::open(dir.path()).unwrap();

        let sidecar_gitignore = dir.path().join(SIDECAR_DIR).join(".gitignore");
        assert!(sidecar_gitignore.exists());
        let gitignore_content = fs::read_to_string(&sidecar_gitignore).unwrap();
        assert_eq!(gitignore_content, SIDECAR_GITIGNORE);
    }

    #[test]
    fn test_vault_write_read_scan_and_delete() {
        let dir = tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();

        let note1_path = Path::new("nested/note1.md");
        let content1 = "# Note 1\nSome initial content.";
        let note1 = vault.write_note(note1_path, content1).unwrap();
        assert_eq!(note1.title, "Note 1");

        let note2_path = Path::new("note2.md");
        let content2 = "---\ntitle: Second\n---\nHello!";
        vault.write_note(note2_path, content2).unwrap();

        // Scan notes
        let notes = vault.scan_notes().unwrap();
        assert_eq!(notes.len(), 2);

        // Read note
        let read = vault.read_note(note1_path).unwrap();
        assert_eq!(read.body, content1);

        // Safe delete (moves to trash)
        vault.delete_note(note1_path).unwrap();
        assert!(!vault.resolve_path(note1_path).exists());
    }

    #[test]
    fn test_vault_folder_lifecycle() {
        let dir = tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();

        // 1. Create folders
        let created = vault.create_folder(Path::new("projects/rust")).unwrap();
        assert_eq!(created, PathBuf::from("projects/rust"));
        assert!(vault.resolve_path(&created).is_dir());

        let folders = vault.scan_folders().unwrap();
        assert!(folders.contains(&PathBuf::from("projects")));
        assert!(folders.contains(&PathBuf::from("projects/rust")));

        // 2. Rename folder
        vault.rename_folder(Path::new("projects/rust"), Path::new("projects/cosmic")).unwrap();
        assert!(!vault.resolve_path(Path::new("projects/rust")).exists());
        assert!(vault.resolve_path(Path::new("projects/cosmic")).is_dir());

        // 3. Delete folder to trash
        vault.delete_folder(Path::new("projects")).unwrap();
        assert!(!vault.resolve_path(Path::new("projects")).exists());
    }
}
