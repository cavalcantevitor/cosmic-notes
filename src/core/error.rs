use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VaultError {
    #[error("I/O error at '{path}': {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to parse YAML frontmatter in '{path}': {message}")]
    FrontmatterParse {
        path: PathBuf,
        message: String,
    },

    #[error("Note not found at '{path}'")]
    NoteNotFound {
        path: PathBuf,
    },

    #[error("Invalid note path: {0}")]
    InvalidPath(String),

    #[error("Failed to move note to trash at '{path}': {message}")]
    TrashError {
        path: PathBuf,
        message: String,
    },

    #[error("Watcher error: {0}")]
    WatcherError(String),

    #[error("Search index error: {0}")]
    SearchError(String),

    #[error("Query parse error: {0}")]
    QueryError(String),

    #[error("Vault sync error: {0}")]
    SyncError(String),

    #[error("Agent API error: {0}")]
    AgentApiError(String),
}


pub type Result<T> = std::result::Result<T, VaultError>;
