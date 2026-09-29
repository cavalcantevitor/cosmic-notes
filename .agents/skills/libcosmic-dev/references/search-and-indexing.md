# Tantivy & Nucleo Search Implementation Patterns

## 1. Tantivy Full-Text Indexing Engine

Tantivy provides sub-millisecond BM25 full-text search across local Markdown files.

### Schema Definition
```rust
use std::path::{Path, PathBuf};
use tantivy::schema::*;
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy};
use tantivy::query::QueryParser;
use tantivy::collector::TopDocs;

#[derive(Clone)]
pub struct VaultSearchSchema {
    pub schema: Schema,
    pub id: Field,
    pub title: Field,
    pub path: Field,
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

        let id = builder.add_text_field("id", STRING | STORED);
        let title = builder.add_text_field("title", text_options.clone());
        let path = builder.add_text_field("path", STRING | STORED);
        let body = builder.add_text_field("body", text_options);
        let tags = builder.add_text_field("tags", STRING | STORED);
        let modified_at = builder.add_i64_field("modified_at", INDEXED | STORED);

        Self {
            schema: builder.build(),
            id,
            title,
            path,
            body,
            tags,
            modified_at,
        }
    }
}
```

### Index Initialization (Memory-Mapped on Disk)
```rust
pub struct SearchEngine {
    index: Index,
    reader: IndexReader,
    fields: VaultSearchSchema,
}

impl SearchEngine {
    pub fn open_or_create(index_dir: &Path, fields: VaultSearchSchema) -> tantivy::Result<Self> {
        std::fs::create_dir_all(index_dir)?;
        let index = Index::open_or_create(
            tantivy::directory::MmapDirectory::open(index_dir)?,
            fields.schema.clone(),
        )?;

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        Ok(Self { index, reader, fields })
    }

    pub fn search(&self, query_str: &str, limit: usize) -> tantivy::Result<Vec<SearchResult>> {
        let searcher = self.reader.searcher();
        let query_parser = QueryParser::for_index(&self.index, vec![self.fields.title, self.fields.body]);
        let query = query_parser.parse_query(query_str)?;

        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;
        let mut results = Vec::new();

        for (score, doc_address) in top_docs {
            let retrieved_doc: TantivyDocument = searcher.doc(doc_address)?;
            // Extract fields and snippets
            results.push(SearchResult {
                score,
                // field extraction...
            });
        }
        Ok(results)
    }
}
```

---

## 2. Nucleo Quick Switcher (`Ctrl+P`)

Nucleo performs multithreaded fuzzy matching in $<0.1\text{ ms}$ over thousands of note titles and paths.

```rust
use nucleo::{Config, Nucleo, Utf32String};

pub struct QuickSwitcher {
    matcher: Nucleo<NoteItem>,
}

#[derive(Clone, Debug)]
pub struct NoteItem {
    pub id: String,
    pub title: String,
    pub rel_path: String,
}

impl QuickSwitcher {
    pub fn new() -> Self {
        Self {
            matcher: Nucleo::new(Config::DEFAULT, Arc::new(|| ()), None, 1),
        }
    }

    pub fn reload_notes(&mut self, notes: Vec<NoteItem>) {
        let injector = self.matcher.injector();
        for item in notes {
            let title_utf32 = Utf32String::from(item.title.as_str());
            injector.push(item, |_, cols| {
                cols[0] = title_utf32.clone();
            });
        }
    }

    pub fn query(&mut self, pattern: &str, max_results: usize) -> Vec<NoteItem> {
        self.matcher.pattern.reparse(
            0,
            pattern,
            nucleo::pattern::CaseMatching::Ignore,
            nucleo::pattern::Normalization::Smart,
        );
        self.matcher.tick(10); // Run match cycle with 10ms timeout

        let snapshot = self.matcher.snapshot();
        snapshot
            .matched_items(..max_results.min(snapshot.matched_item_count()))
            .map(|item| item.data.clone())
            .collect()
    }
}
```
