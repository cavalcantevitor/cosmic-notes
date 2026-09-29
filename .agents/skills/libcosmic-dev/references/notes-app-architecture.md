# Notes App Architecture Patterns for COSMIC (`libcosmic`)

## 1. Storage & Domain Model

A robust, local-first notes app can use either flat Markdown files (`.md` in a user-chosen directory) or SQLite (with `sqlx` or `rusqlite`):

```rust
use std::path::PathBuf;
use chrono::{DateTime, Utc};

#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub path: Option<PathBuf>,
    pub modified_at: DateTime<Utc>,
    pub tags: Vec<String>,
}
```

## 2. Editor State & Markdown Rendering

- **Editor**: `cosmic::widget::text_editor::Content` holds the live editable buffer.
- **Preview**: `cosmic::widget::markdown` parses and renders the markdown AST into native COSMIC widgets.
- **Split View / View Modes**:
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum ViewMode {
      EditorOnly,
      Split,
      PreviewOnly,
  }
  ```

## 3. Asynchronous I/O with Tokio

Keep the UI at 60/120+ FPS by offloading file saves, directory scans, and searches into `Task`:

```rust
fn save_note_task(path: PathBuf, content: String) -> Task<cosmic::Action<Message>> {
    cosmic::task::future(async move {
        tokio::fs::write(&path, content).await
            .map_err(|e| e.to_string())
    })
    .map(Message::NoteSaved)
    .into()
}
```

## 4. UI Layout Composition

A typical 3-column or 2-column layout in `libcosmic`:
- **Left Column**: Notebooks, tags, search bar, and note list (`cosmic::widget::scrollable` with selectable list items).
- **Center / Right Column**: Note header (title, tags, modified date) + split pane (editor on left, markdown preview on right).
- **Context Drawer**: Note details, backlinks, word count, metadata inspector.
