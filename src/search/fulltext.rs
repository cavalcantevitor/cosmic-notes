use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tantivy::collector::TopDocs;
use tantivy::directory::MmapDirectory;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::snippet::SnippetGenerator;
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};

use crate::core::error::{Result, VaultError};
use crate::core::Note;

/// Schema fields for Tantivy full-text index.
#[derive(Clone, Debug)]
pub struct VaultSearchSchema {
    pub schema: Schema,
    pub path: Field,
    pub title: Field,
    pub body: Field,
    pub tags: Field,
    pub modified_at: Field,
}

impl VaultSearchSchema {
    pub fn new() -> Self {
        let mut builder = Schema::builder();
        let text_options = TextOptions::default()
            .set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer("default")
                    .set_index_option(IndexRecordOption::WithFreqsAndPositions),
            )
            .set_stored();

        let path = builder.add_text_field("path", STRING | STORED);
        let title = builder.add_text_field("title", text_options.clone());
        let body = builder.add_text_field("body", text_options.clone());
        let tags = builder.add_text_field("tags", text_options);
        let modified_at = builder.add_i64_field("modified_at", INDEXED | STORED);

        Self {
            schema: builder.build(),
            path,
            title,
            body,
            tags,
            modified_at,
        }
    }
}

impl Default for VaultSearchSchema {
    fn default() -> Self {
        Self::new()
    }
}

/// A hit from the full-text search engine.
#[derive(Debug, Clone, PartialEq)]
pub struct FullTextSearchResult {
    pub path: PathBuf,
    pub title: String,
    pub score: f32,
    pub snippet: Option<String>,
}

/// Tier 2 BM25 full-text search engine powered by `tantivy`.
/// Target latency: <0.5ms.
#[derive(Clone)]
pub struct FullTextEngine {
    index: Index,
    reader: IndexReader,
    writer: Arc<Mutex<IndexWriter>>,
    fields: VaultSearchSchema,
}

impl FullTextEngine {
    /// Open an existing Tantivy index or create a new one on disk.
    pub fn open_or_create(index_dir: &Path) -> Result<Self> {
        let fields = VaultSearchSchema::new();
        std::fs::create_dir_all(index_dir).map_err(|e| VaultError::Io {
            path: index_dir.to_path_buf(),
            source: e,
        })?;

        let dir = MmapDirectory::open(index_dir).map_err(|e| {
            VaultError::SearchError(format!("Failed to open index directory: {}", e))
        })?;

        let index = Index::open_or_create(dir, fields.schema.clone()).map_err(|e| {
            VaultError::SearchError(format!("Failed to open or create index: {}", e))
        })?;

        Self::init_engine(index, fields)
    }

    /// Create an in-memory index (primarily for fast unit and integration tests).
    pub fn create_in_ram() -> Result<Self> {
        let fields = VaultSearchSchema::new();
        let index = Index::create_in_ram(fields.schema.clone());
        Self::init_engine(index, fields)
    }

    fn init_engine(index: Index, fields: VaultSearchSchema) -> Result<Self> {
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| {
                VaultError::SearchError(format!("Failed to create index reader: {}", e))
            })?;

        // 50 MB heap buffer for indexing
        let writer = index.writer(50_000_000).map_err(|e| {
            VaultError::SearchError(format!("Failed to create index writer: {}", e))
        })?;

        Ok(Self {
            index,
            reader,
            writer: Arc::new(Mutex::new(writer)),
            fields,
        })
    }

    /// Add or update a note in the full-text index.
    pub fn index_note(&self, note: &Note) -> Result<()> {
        let mut writer = self.writer.lock().map_err(|_| {
            VaultError::SearchError("Failed to acquire index writer lock".into())
        })?;

        let path_str = note.path.to_string_lossy().to_string();
        let term = Term::from_field_text(self.fields.path, &path_str);
        writer.delete_term(term);

        let doc = doc!(
            self.fields.path => path_str,
            self.fields.title => note.title.clone(),
            self.fields.body => note.body.clone(),
            self.fields.tags => note.tags.join(" "),
            self.fields.modified_at => note.modified_at.timestamp(),
        );

        writer.add_document(doc).map_err(|e| {
            VaultError::SearchError(format!("Failed to add document to index: {}", e))
        })?;

        writer.commit().map_err(|e| {
            VaultError::SearchError(format!("Failed to commit index changes: {}", e))
        })?;

        self.reader.reload().map_err(|e| {
            VaultError::SearchError(format!("Failed to reload index reader: {}", e))
        })?;

        Ok(())
    }

    /// Remove a note from the index by relative path.
    pub fn remove_note(&self, path: &Path) -> Result<()> {
        let mut writer = self.writer.lock().map_err(|_| {
            VaultError::SearchError("Failed to acquire index writer lock".into())
        })?;

        let path_str = path.to_string_lossy().to_string();
        let term = Term::from_field_text(self.fields.path, &path_str);
        writer.delete_term(term);

        writer.commit().map_err(|e| {
            VaultError::SearchError(format!("Failed to commit index deletion: {}", e))
        })?;

        self.reader.reload().map_err(|e| {
            VaultError::SearchError(format!("Failed to reload index reader: {}", e))
        })?;

        Ok(())
    }

    /// Re-index all notes from scratch.
    pub fn reindex_all(&self, notes: &[Note]) -> Result<()> {
        let mut writer = self.writer.lock().map_err(|_| {
            VaultError::SearchError("Failed to acquire index writer lock".into())
        })?;

        writer.delete_all_documents().map_err(|e| {
            VaultError::SearchError(format!("Failed to clear index: {}", e))
        })?;

        for note in notes {
            let path_str = note.path.to_string_lossy().to_string();
            let doc = doc!(
                self.fields.path => path_str,
                self.fields.title => note.title.clone(),
                self.fields.body => note.body.clone(),
                self.fields.tags => note.tags.join(" "),
                self.fields.modified_at => note.modified_at.timestamp(),
            );
            writer.add_document(doc).map_err(|e| {
                VaultError::SearchError(format!("Failed to index note '{}': {}", note.title, e))
            })?;
        }

        writer.commit().map_err(|e| {
            VaultError::SearchError(format!("Failed to commit full reindex: {}", e))
        })?;

        self.reader.reload().map_err(|e| {
            VaultError::SearchError(format!("Failed to reload index reader: {}", e))
        })?;

        Ok(())
    }

    /// Execute a BM25 full-text query with snippet generation.
    pub fn search(&self, query_str: &str, limit: usize) -> Result<Vec<FullTextSearchResult>> {
        if query_str.trim().is_empty() {
            return Ok(Vec::new());
        }

        let searcher = self.reader.searcher();
        let mut query_parser = QueryParser::for_index(
            &self.index,
            vec![self.fields.title, self.fields.body, self.fields.tags],
        );
        // Boost title matches higher than body
        query_parser.set_field_boost(self.fields.title, 2.0);

        let query = query_parser.parse_query(query_str).map_err(|e| {
            VaultError::QueryError(format!("Query syntax error in '{}': {}", query_str, e))
        })?;

        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(limit))
            .map_err(|e| {
                VaultError::SearchError(format!("Error executing search: {}", e))
            })?;

        let snippet_generator = SnippetGenerator::create(&searcher, &*query, self.fields.body).ok();

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let retrieved_doc: TantivyDocument = searcher.doc(doc_address).map_err(|e| {
                VaultError::SearchError(format!("Error retrieving document: {}", e))
            })?;

            let path_val = retrieved_doc
                .get_first(self.fields.path)
                .and_then(|v| v.as_str())
                .unwrap_or_default();

            let title_val = retrieved_doc
                .get_first(self.fields.title)
                .and_then(|v| v.as_str())
                .unwrap_or_default();

            let snippet = snippet_generator
                .as_ref()
                .map(|generator| generator.snippet_from_doc(&retrieved_doc).to_html());

            results.push(FullTextSearchResult {
                path: PathBuf::from(path_val),
                title: title_val.to_string(),
                score,
                snippet,
            });
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_fulltext_indexing_and_search() {
        let engine = FullTextEngine::create_in_ram().expect("Failed to create in-ram index");

        let note1 = Note::parse(
            PathBuf::from("rust_concurrency.md"),
            PathBuf::from("/vault/rust_concurrency.md"),
            "# Rust Concurrency\n\nChannels and mutexes provide synchronization in multi-threaded programs.\n#rust #concurrency".to_string(),
            Utc::now(),
        )
        .unwrap();

        let note2 = Note::parse(
            PathBuf::from("cooking_baking.md"),
            PathBuf::from("/vault/cooking_baking.md"),
            "# Sourdough Baking\n\nFermentation requires flour, water, salt, and time.\n#bread #baking".to_string(),
            Utc::now(),
        )
        .unwrap();

        engine.index_note(&note1).unwrap();
        engine.index_note(&note2).unwrap();

        // Search for body term
        let results = engine.search("synchronization", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Rust Concurrency");
        assert!(results[0].snippet.is_some());
        let snippet = results[0].snippet.as_ref().unwrap();
        assert!(snippet.contains("synchronization") || snippet.contains("<b>") || snippet.contains("<mark>"));

        // Search for tag
        let results = engine.search("baking", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Sourdough Baking");

        // Search with no match
        let results = engine.search("astronomy", 10).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_fulltext_update_and_delete() {
        let engine = FullTextEngine::create_in_ram().expect("Failed to create in-ram index");

        let note = Note::parse(
            PathBuf::from("draft.md"),
            PathBuf::from("/vault/draft.md"),
            "# Initial Draft\n\nSecret codeword is AppleBanana.".to_string(),
            Utc::now(),
        )
        .unwrap();

        engine.index_note(&note).unwrap();
        assert_eq!(engine.search("AppleBanana", 5).unwrap().len(), 1);

        // Update note with new content
        let note_updated = Note::parse(
            PathBuf::from("draft.md"),
            PathBuf::from("/vault/draft.md"),
            "# Initial Draft\n\nSecret codeword is CherryDate.".to_string(),
            Utc::now(),
        )
        .unwrap();

        engine.index_note(&note_updated).unwrap();
        assert_eq!(engine.search("AppleBanana", 5).unwrap().len(), 0);
        assert_eq!(engine.search("CherryDate", 5).unwrap().len(), 1);

        // Remove note
        engine.remove_note(Path::new("draft.md")).unwrap();
        assert_eq!(engine.search("CherryDate", 5).unwrap().len(), 0);
    }

    #[test]
    fn test_fulltext_disk_persistence() {
        let temp_dir = tempfile::tempdir().unwrap();
        let index_path = temp_dir.path().join("index");

        {
            let engine = FullTextEngine::open_or_create(&index_path).unwrap();
            let note = Note::parse(
                PathBuf::from("persistent.md"),
                PathBuf::from("/vault/persistent.md"),
                "# Persistent Note\n\nPersisted content on disk.".to_string(),
                Utc::now(),
            )
            .unwrap();
            engine.index_note(&note).unwrap();
        }

        // Reopen index from disk
        let reopened = FullTextEngine::open_or_create(&index_path).unwrap();
        let results = reopened.search("Persisted", 5).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Persistent Note");
    }
}

