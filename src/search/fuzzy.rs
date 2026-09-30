use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use nucleo::pattern::{CaseMatching, Normalization};
use nucleo::{Config, Nucleo, Utf32String};

use crate::core::Note;

/// A lightweight representation of a note for fuzzy matching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuzzyNoteItem {
    pub path: PathBuf,
    pub title: String,
    pub tags: Vec<String>,
}

impl From<&Note> for FuzzyNoteItem {
    fn from(note: &Note) -> Self {
        Self {
            path: note.path.clone(),
            title: note.title.clone(),
            tags: note.tags.clone(),
        }
    }
}

/// A matched result from the fuzzy search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuzzyMatch {
    pub item: FuzzyNoteItem,
    pub score: u32,
}

/// High-speed in-memory fuzzy search engine powered by `nucleo`.
/// Target latency: <0.1ms.
pub struct FuzzyEngine {
    matcher: Mutex<Nucleo<FuzzyNoteItem>>,
    items: Mutex<Vec<FuzzyNoteItem>>,
}

impl FuzzyEngine {
    pub fn new() -> Self {
        Self {
            matcher: Mutex::new(Nucleo::new(Config::DEFAULT, Arc::new(|| ()), None, 1)),
            items: Mutex::new(Vec::new()),
        }
    }

    /// Reload the matcher with the given notes.
    pub fn reindex(&self, notes: &[Note]) {
        let items: Vec<FuzzyNoteItem> = notes.iter().map(FuzzyNoteItem::from).collect();
        *self.items.lock().unwrap() = items;
        self.rebuild_matcher();
    }

    /// Add or update a single note.
    pub fn add_or_update(&self, note: &Note) {
        let item = FuzzyNoteItem::from(note);
        let mut items = self.items.lock().unwrap();
        if let Some(pos) = items.iter().position(|i| i.path == item.path) {
            items[pos] = item;
        } else {
            items.push(item);
        }
        drop(items);
        self.rebuild_matcher();
    }

    /// Remove a note by relative path.
    pub fn remove(&self, path: &Path) {
        let mut items = self.items.lock().unwrap();
        if let Some(pos) = items.iter().position(|i| i.path == path) {
            items.remove(pos);
            drop(items);
            self.rebuild_matcher();
        }
    }

    /// Search across notes matching title, path, and tags.
    pub fn search(&self, pattern: &str, max_results: usize) -> Vec<FuzzyMatch> {
        let items_guard = self.items.lock().unwrap();
        if pattern.trim().is_empty() {
            return items_guard
                .iter()
                .take(max_results)
                .map(|item| FuzzyMatch {
                    item: item.clone(),
                    score: 0,
                })
                .collect();
        }
        drop(items_guard);

        let mut matcher = self.matcher.lock().unwrap();
        matcher.pattern.reparse(
            0,
            pattern,
            CaseMatching::Ignore,
            Normalization::Smart,
            false,
        );

        // Run matching ticks until finished or timed out
        for _ in 0..5 {
            let status = matcher.tick(10);
            if !status.running {
                break;
            }
        }

        let snapshot = matcher.snapshot();
        let total_matches = snapshot.matched_item_count();
        let count = max_results.min(total_matches as usize);

        snapshot
            .matched_items(..count as u32)
            .enumerate()
            .map(|(idx, item)| FuzzyMatch {
                item: item.data.clone(),
                score: (count.saturating_sub(idx)) as u32,
            })
            .collect()
    }

    fn rebuild_matcher(&self) {
        let items = self.items.lock().unwrap().clone();
        let new_matcher = Nucleo::new(Config::DEFAULT, Arc::new(|| ()), None, 1);
        let injector = new_matcher.injector();
        for item in &items {
            let searchable = format!(
                "{} {} {}",
                item.title,
                item.path.to_string_lossy(),
                item.tags.join(" ")
            );
            let utf32 = Utf32String::from(searchable.as_str());
            injector.push(item.clone(), |_, cols| {
                cols[0] = utf32;
            });
        }
        drop(injector);
        let mut new_matcher = new_matcher;
        for _ in 0..5 {
            let status = new_matcher.tick(10);
            if !status.running {
                break;
            }
        }
        *self.matcher.lock().unwrap() = new_matcher;
    }
}


impl Default for FuzzyEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_fuzzy_search_matches_title_path_and_tags() {
        let note1 = Note::parse(
            PathBuf::from("rust/ownership.md"),
            PathBuf::from("/vault/rust/ownership.md"),
            "# Rust Memory Safety\n\n#rust #memory".to_string(),
            Utc::now(),
        )
        .unwrap();

        let note2 = Note::parse(
            PathBuf::from("recipes/pasta.md"),
            PathBuf::from("/vault/recipes/pasta.md"),
            "# Italian Pasta\n\n#cooking #food".to_string(),
            Utc::now(),
        )
        .unwrap();

        let engine = FuzzyEngine::new();
        engine.reindex(&[note1, note2]);

        // Search by title fragment
        let results = engine.search("Memory", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].item.title, "Rust Memory Safety");

        // Search by path fragment
        let results = engine.search("recipes", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].item.title, "Italian Pasta");

        // Search by tag fragment
        let results = engine.search("cooking", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].item.title, "Italian Pasta");
    }

    #[test]
    fn test_fuzzy_add_update_remove() {
        let note1 = Note::parse(
            PathBuf::from("a.md"),
            PathBuf::from("/vault/a.md"),
            "# Title A\n\n#alpha".to_string(),
            Utc::now(),
        )
        .unwrap();

        let engine = FuzzyEngine::new();
        engine.add_or_update(&note1);


        assert_eq!(engine.search("Title", 5).len(), 1);

        // Update
        let note1_updated = Note::parse(
            PathBuf::from("a.md"),
            PathBuf::from("/vault/a.md"),
            "# Brand New Title\n\n#alpha".to_string(),
            Utc::now(),
        )
        .unwrap();

        engine.add_or_update(&note1_updated);

        let results = engine.search("Brand", 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].item.title, "Brand New Title");

        // Remove
        engine.remove(Path::new("a.md"));
        assert_eq!(engine.search("Brand", 5).len(), 0);
    }
}
