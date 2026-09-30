use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::core::error::Result;
use crate::core::note::Note;
use crate::core::vault::Vault;
use crate::search::VaultIndex;

/// Response payload for reading or writing a note via the agent API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentNoteResponse {
    pub path: PathBuf,
    pub title: String,
    pub tags: Vec<String>,
    pub content: String,
    pub modified: String,
}

impl From<&Note> for AgentNoteResponse {
    fn from(n: &Note) -> Self {
        Self {
            path: n.path.clone(),
            title: n.title.clone(),
            tags: n.tags.clone(),
            content: n.raw_content.clone(),
            modified: n.modified_at.to_rfc3339(),
        }
    }
}

/// Search match returned by the agent API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentSearchResult {
    pub path: PathBuf,
    pub title: String,
    pub score: f32,
    pub snippet: Option<String>,
    pub tags: Vec<String>,
}

/// MCP (Model Context Protocol) tool input parameter schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Programmatic API contract for AI Agents and MCP tools to interact with the vault.
pub trait AgentVaultApi: Send + Sync {
    /// Read the complete contents and metadata of a note.
    fn read_note(&self, path: &Path) -> Result<AgentNoteResponse>;

    /// Create or atomically update a note with write-echo cancellation.
    fn write_note(&self, path: &Path, content: &str) -> Result<AgentNoteResponse>;

    /// Perform a deep full-text and fuzzy search across notes in the vault.
    fn search_notes(&self, query: &str, limit: usize) -> Result<Vec<AgentSearchResult>>;

    /// Retrieve all backlinks pointing to a given note (by relative path).
    fn get_backlinks(&self, path: &Path) -> Result<Vec<PathBuf>>;

    /// List all markdown notes currently in the vault.
    fn list_notes(&self) -> Result<Vec<PathBuf>>;

    /// Safely move a note to the system trash bin.
    fn delete_note(&self, path: &Path) -> Result<()>;
}

/// Default production implementation of `AgentVaultApi` backed by the Vault and VaultIndex.
#[derive(Clone)]
pub struct DefaultAgentVaultApi {
    vault: Arc<Vault>,
    index: Arc<Mutex<VaultIndex>>,
}

impl DefaultAgentVaultApi {
    pub fn new(vault: Arc<Vault>, index: Arc<Mutex<VaultIndex>>) -> Self {
        Self { vault, index }
    }
}

impl AgentVaultApi for DefaultAgentVaultApi {
    fn read_note(&self, path: &Path) -> Result<AgentNoteResponse> {
        let note = self.vault.read_note(path)?;
        Ok(AgentNoteResponse::from(&note))
    }

    fn write_note(&self, path: &Path, content: &str) -> Result<AgentNoteResponse> {
        let note = self.vault.write_note(path, content)?;
        self.index.lock().unwrap().update_note(&note)?;
        Ok(AgentNoteResponse::from(&note))
    }

    fn search_notes(&self, query: &str, limit: usize) -> Result<Vec<AgentSearchResult>> {
        let index_guard = self.index.lock().unwrap();
        let ft_results = index_guard.fulltext_search(query, limit)?;
        let mut results = Vec::new();

        for r in ft_results {
            if let Ok(note) = self.vault.read_note(&r.path) {
                results.push(AgentSearchResult {
                    path: r.path,
                    title: r.title,
                    score: r.score,
                    snippet: r.snippet,
                    tags: note.tags,
                });
            }
        }

        // If fulltext returned fewer than limit, supplement with fuzzy search
        if results.len() < limit {
            let fuzzy_matches = index_guard.quick_search(query, limit - results.len());
            for m in fuzzy_matches {
                if !results.iter().any(|existing| existing.path == m.item.path) {
                    results.push(AgentSearchResult {
                        path: m.item.path,
                        title: m.item.title,
                        score: m.score as f32,
                        snippet: None,
                        tags: m.item.tags,
                    });
                }
            }
        }

        Ok(results)
    }

    fn get_backlinks(&self, path: &Path) -> Result<Vec<PathBuf>> {
        let index_guard = self.index.lock().unwrap();
        let backlinks = index_guard.backlinks(path);
        Ok(backlinks.into_iter().map(|b| b.source_path).collect())
    }

    fn list_notes(&self) -> Result<Vec<PathBuf>> {
        let notes = self.vault.scan_notes()?;
        Ok(notes.into_iter().map(|n| n.path).collect())
    }

    fn delete_note(&self, path: &Path) -> Result<()> {
        self.vault.delete_note(path)?;
        self.index.lock().unwrap().remove_note(path)?;
        Ok(())
    }
}

/// Standard Model Context Protocol (MCP) tool schemas for COSMIC Notes.
pub fn mcp_tool_definitions() -> Vec<McpToolDefinition> {
    vec![
        McpToolDefinition {
            name: "vault_read_note".to_string(),
            description: "Read the full Markdown content, title, tags, and metadata of a note in the COSMIC Notes vault.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path to the markdown file inside the vault, e.g. 'work/meeting.md'"
                    }
                },
                "required": ["path"]
            }),
        },
        McpToolDefinition {
            name: "vault_write_note".to_string(),
            description: "Create or update a Markdown note in the vault with atomic disk persistence, frontmatter support, and write-echo cancellation.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path for the markdown file"
                    },
                    "content": {
                        "type": "string",
                        "description": "Raw markdown text content including optional YAML frontmatter"
                    }
                },
                "required": ["path", "content"]
            }),
        },
        McpToolDefinition {
            name: "vault_search_notes".to_string(),
            description: "Perform BM25 full-text and fuzzy search across notes in the vault, returning matched snippets and scores.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search keyword or natural language query"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of search results to return (default 10)",
                        "default": 10
                    }
                },
                "required": ["query"]
            }),
        },
        McpToolDefinition {
            name: "vault_get_backlinks".to_string(),
            description: "Get all notes in the vault that link to the specified note path via wikilinks [[Target]].".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path to the target note, e.g. 'work/project.md'"
                    }
                },
                "required": ["path"]
            }),
        },
        McpToolDefinition {
            name: "vault_list_notes".to_string(),
            description: "List relative paths of all markdown notes in the vault.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_mcp_tool_definitions_validity() {
        let tools = mcp_tool_definitions();
        assert_eq!(tools.len(), 5);
        for tool in tools {
            assert!(!tool.name.is_empty());
            assert!(!tool.description.is_empty());
            assert!(tool.input_schema.is_object());
        }
    }

    #[test]
    fn test_agent_vault_api_operations() {
        let dir = tempdir().unwrap();
        let vault = Arc::new(Vault::open(dir.path()).unwrap());
        let notes = vault.scan_notes().unwrap();
        let index = Arc::new(Mutex::new(VaultIndex::open_or_create(&dir.path().join(".cosmic-notes/index"), &notes).unwrap()));

        let api = DefaultAgentVaultApi::new(vault, index);

        // 1. Write note
        let written = api.write_note(
            Path::new("agent_note.md"),
            "---\ntitle: Agent Note\ntags: [ai, assistant]\n---\n# Agent Note\nContent created by agent.",
        ).unwrap();
        assert_eq!(written.title, "Agent Note");
        assert_eq!(written.tags, vec!["ai", "assistant"]);

        // 2. Read note
        let read = api.read_note(Path::new("agent_note.md")).unwrap();
        assert_eq!(read.title, "Agent Note");
        assert!(read.content.contains("Content created by agent"));

        // 3. List notes
        let list = api.list_notes().unwrap();
        assert_eq!(list, vec![PathBuf::from("agent_note.md")]);

        // 4. Search notes
        let search = api.search_notes("created by agent", 5).unwrap();
        assert!(!search.is_empty());
        assert_eq!(search[0].path, Path::new("agent_note.md"));

        // 5. Delete note
        api.delete_note(Path::new("agent_note.md")).unwrap();
        let list_after = api.list_notes().unwrap();
        assert!(list_after.is_empty());
    }
}
