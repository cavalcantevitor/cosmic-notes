pub mod fulltext;
pub mod fuzzy;

pub use fulltext::{FullTextEngine, FullTextSearchResult};
pub use fuzzy::{FuzzyEngine, FuzzyMatch, FuzzyNoteItem};

use std::path::Path;

use crate::core::error::Result;
use crate::core::note::Note;
use crate::core::vault::Vault;
use crate::core::watcher::VaultEvent;
use crate::graph::{BacklinkInfo, KnowledgeGraph, OutgoingLinkInfo};

/// Unified index coordinator managing Tier 1 fuzzy search (`nucleo`),
/// Tier 2 full-text search (`tantivy`), and bidirectional knowledge graph (`petgraph`).
pub struct VaultIndex {
    pub fuzzy: FuzzyEngine,
    pub fulltext: FullTextEngine,
    pub graph: KnowledgeGraph,
}

impl VaultIndex {
    /// Initialize with disk-backed Tantivy index in the specified directory.
    pub fn open_or_create(index_dir: &Path, notes: &[Note]) -> Result<Self> {
        let fuzzy = FuzzyEngine::new();
        let fulltext = FullTextEngine::open_or_create(index_dir)?;
        let graph = KnowledgeGraph::build(notes);

        let mut index = Self {
            fuzzy,
            fulltext,
            graph,
        };
        index.reindex_all(notes)?;
        Ok(index)
    }

    /// In-memory index coordinator (ideal for fast tests and ephemeral vaults).
    pub fn create_in_ram(notes: &[Note]) -> Result<Self> {
        let fuzzy = FuzzyEngine::new();
        let fulltext = FullTextEngine::create_in_ram()?;
        let graph = KnowledgeGraph::build(notes);

        let mut index = Self {
            fuzzy,
            fulltext,
            graph,
        };
        index.reindex_all(notes)?;
        Ok(index)
    }

    /// Re-index all notes across all tiers.
    pub fn reindex_all(&mut self, notes: &[Note]) -> Result<()> {
        self.fuzzy.reindex(notes);
        self.fulltext.reindex_all(notes)?;
        self.graph.rebuild(notes);
        Ok(())
    }

    /// Add or update a note across all tiers.
    pub fn update_note(&mut self, note: &Note) -> Result<()> {
        self.fuzzy.add_or_update(note);
        self.fulltext.index_note(note)?;
        self.graph.add_or_update(note);
        Ok(())
    }

    /// Remove a note across all tiers.
    pub fn remove_note(&mut self, path: &Path) -> Result<()> {
        self.fuzzy.remove(path);
        self.fulltext.remove_note(path)?;
        self.graph.remove(path);
        Ok(())
    }

    /// Reactively handle incoming `VaultEvent` emitted by `VaultWatcher`.
    pub fn handle_vault_event(&mut self, event: &VaultEvent, vault: &Vault) -> Result<()> {
        match event {
            VaultEvent::Created(rel_path) | VaultEvent::Modified(rel_path) => {
                if let Ok(note) = vault.read_note(rel_path) {
                    self.update_note(&note)?;
                }
            }
            VaultEvent::Deleted(rel_path) => {
                self.remove_note(rel_path)?;
            }
            VaultEvent::Renamed { from, to } => {
                self.remove_note(from)?;
                if let Ok(note) = vault.read_note(to) {
                    self.update_note(&note)?;
                }
            }
        }
        Ok(())
    }

    /// Quick Switcher (<0.1ms fuzzy matching on title, path, and tags).
    pub fn quick_search(&self, query: &str, limit: usize) -> Vec<FuzzyMatch> {
        self.fuzzy.search(query, limit)
    }


    /// Full-text BM25 search with highlighted snippets (<0.5ms).
    pub fn fulltext_search(&self, query: &str, limit: usize) -> Result<Vec<FullTextSearchResult>> {
        self.fulltext.search(query, limit)
    }

    /// Backlinks pointing to the given note.
    pub fn backlinks(&self, path: &Path) -> Vec<BacklinkInfo> {
        self.graph.backlinks(path)
    }

    /// Outgoing links originating from the given note.
    pub fn outgoing_links(&self, path: &Path) -> Vec<OutgoingLinkInfo> {
        self.graph.outgoing_links(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;



    #[test]
    fn test_vault_index_coordinator_and_event_handling() {
        let temp_dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(temp_dir.path()).unwrap();

        // Write two notes via vault
        let note1 = vault
            .write_note(
                Path::new("cosmic_desktop.md"),
                "# COSMIC Desktop\n\nWritten in Rust and libcosmic. Links to [[Pop OS]].\n#cosmic #rust",
            )
            .unwrap();

        let note2 = vault
            .write_note(
                Path::new("pop_os.md"),
                "# Pop OS\n\nLinux distribution developed by System76.\n#linux #distro",
            )
            .unwrap();

        let mut index = VaultIndex::create_in_ram(&[note1.clone(), note2.clone()]).unwrap();

        // 1. Test Quick Search
        let quick = index.quick_search("desktop", 5);
        assert_eq!(quick.len(), 1);
        assert_eq!(quick[0].item.title, "COSMIC Desktop");

        // 2. Test Full-Text Search
        let full = index.fulltext_search("System76", 5).unwrap();
        assert_eq!(full.len(), 1);
        assert_eq!(full[0].title, "Pop OS");

        // 3. Test Graph Backlinks
        let backlinks = index.backlinks(Path::new("pop_os.md"));
        assert_eq!(backlinks.len(), 1);
        assert_eq!(backlinks[0].source_title, "COSMIC Desktop");

        // 4. Test Event Handling: Modify note2 to add a backlink
        vault
            .write_note(
                Path::new("pop_os.md"),
                "# Pop OS\n\nLinux distribution developed by System76. Links back to [[COSMIC Desktop]].\n#linux",
            )
            .unwrap();

        index
            .handle_vault_event(&VaultEvent::Modified(PathBuf::from("pop_os.md")), &vault)
            .unwrap();

        let desktop_backlinks = index.backlinks(Path::new("cosmic_desktop.md"));
        assert_eq!(desktop_backlinks.len(), 1);
        assert_eq!(desktop_backlinks[0].source_title, "Pop OS");

        // 5. Test Event Handling: Delete note2
        index
            .handle_vault_event(&VaultEvent::Deleted(PathBuf::from("pop_os.md")), &vault)
            .unwrap();

        let quick_results = index.quick_search("distro", 5);
        let full_results = index.fulltext_search("System76", 5).unwrap();
        let remaining_backlinks = index.backlinks(Path::new("cosmic_desktop.md"));

        assert_eq!(quick_results.len(), 0);
        assert_eq!(full_results.len(), 0);
        assert_eq!(remaining_backlinks.len(), 0);



    }
}
