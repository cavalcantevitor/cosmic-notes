use std::path::{Path, PathBuf};

use crate::core::note::Note;

/// Represents a folder node in the vault hierarchy tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultFolder {
    pub name: String,
    pub relative_path: PathBuf,
    pub subfolders: Vec<VaultFolder>,
    pub note_paths: Vec<PathBuf>,
}

impl VaultFolder {
    pub fn new(name: impl Into<String>, relative_path: PathBuf) -> Self {
        Self {
            name: name.into(),
            relative_path,
            subfolders: Vec::new(),
            note_paths: Vec::new(),
        }
    }

    /// Total count of notes contained in this folder and all nested subfolders.
    pub fn total_notes_count(&self) -> usize {
        let sub_count: usize = self.subfolders.iter().map(|f| f.total_notes_count()).sum();
        self.note_paths.len() + sub_count
    }

    /// Build a hierarchical tree from disk folders and notes.
    pub fn build_tree(folders: &[PathBuf], notes: &[Note]) -> VaultFolder {
        let mut root = VaultFolder::new("Vault", PathBuf::new());

        // First insert all folder paths
        for folder_path in folders {
            root.insert_folder(folder_path);
        }

        // Then place notes into their respective folder nodes
        for note in notes {
            root.insert_note(&note.path);
        }

        root.sort_recursive();
        root
    }

    fn insert_folder(&mut self, path: &Path) {
        let mut current = self;
        let mut current_path = PathBuf::new();

        for component in path.components() {
            let comp_str = component.as_os_str().to_string_lossy().to_string();
            current_path.push(&comp_str);

            if let Some(pos) = current.subfolders.iter().position(|f| f.name == comp_str) {
                current = &mut current.subfolders[pos];
            } else {
                let new_folder = VaultFolder::new(comp_str, current_path.clone());
                current.subfolders.push(new_folder);
                let last_idx = current.subfolders.len() - 1;
                current = &mut current.subfolders[last_idx];
            }
        }
    }

    fn insert_note(&mut self, note_rel_path: &Path) {
        if let Some(parent) = note_rel_path.parent() {
            if parent.as_os_str().is_empty() {
                if !self.note_paths.contains(&note_rel_path.to_path_buf()) {
                    self.note_paths.push(note_rel_path.to_path_buf());
                }
            } else {
                self.insert_folder(parent);
                let mut current = self;
                for component in parent.components() {
                    let comp_str = component.as_os_str().to_string_lossy().to_string();
                    if let Some(pos) = current.subfolders.iter().position(|f| f.name == comp_str) {
                        current = &mut current.subfolders[pos];
                    }
                }
                if !current.note_paths.contains(&note_rel_path.to_path_buf()) {
                    current.note_paths.push(note_rel_path.to_path_buf());
                }
            }
        } else {
            if !self.note_paths.contains(&note_rel_path.to_path_buf()) {
                self.note_paths.push(note_rel_path.to_path_buf());
            }
        }
    }

    fn sort_recursive(&mut self) {
        self.subfolders.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        self.note_paths.sort_by(|a, b| a.to_string_lossy().to_lowercase().cmp(&b.to_string_lossy().to_lowercase()));
        for sub in &mut self.subfolders {
            sub.sort_recursive();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_vault_folder_tree_construction() {
        let note1 = Note::parse(
            PathBuf::from("root_note.md"),
            PathBuf::from("/vault/root_note.md"),
            "# Root".to_string(),
            Utc::now(),
        ).unwrap();

        let note2 = Note::parse(
            PathBuf::from("work/project/spec.md"),
            PathBuf::from("/vault/work/project/spec.md"),
            "# Spec".to_string(),
            Utc::now(),
        ).unwrap();

        let note3 = Note::parse(
            PathBuf::from("work/meeting.md"),
            PathBuf::from("/vault/work/meeting.md"),
            "# Meeting".to_string(),
            Utc::now(),
        ).unwrap();

        let empty_folder = PathBuf::from("personal/finances");
        let folders = vec![PathBuf::from("work"), PathBuf::from("work/project"), empty_folder];
        let notes = vec![note1, note2, note3];

        let tree = VaultFolder::build_tree(&folders, &notes);

        assert_eq!(tree.name, "Vault");
        assert_eq!(tree.note_paths, vec![PathBuf::from("root_note.md")]);
        assert_eq!(tree.total_notes_count(), 3);

        // Subfolders: personal and work
        assert_eq!(tree.subfolders.len(), 2);
        assert_eq!(tree.subfolders[0].name, "personal");
        assert_eq!(tree.subfolders[1].name, "work");

        // Personal contains finances (empty)
        let personal = &tree.subfolders[0];
        assert_eq!(personal.subfolders.len(), 1);
        assert_eq!(personal.subfolders[0].name, "finances");
        assert_eq!(personal.subfolders[0].total_notes_count(), 0);

        // Work contains meeting.md and project/spec.md
        let work = &tree.subfolders[1];
        assert_eq!(work.note_paths, vec![PathBuf::from("work/meeting.md")]);
        assert_eq!(work.subfolders.len(), 1);
        assert_eq!(work.subfolders[0].name, "project");
        assert_eq!(work.subfolders[0].note_paths, vec![PathBuf::from("work/project/spec.md")]);
    }
}
