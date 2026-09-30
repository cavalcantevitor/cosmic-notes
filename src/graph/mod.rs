use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::core::Note;

/// Represents a node in the knowledge graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphNode {
    /// Unique identifier (relative path string or unresolved link target)
    pub id: String,
    /// Human-readable label (note title or wikilink target)
    pub label: String,
    /// Vault-relative path if resolved to a real note
    pub path: Option<PathBuf>,
    /// Whether this node represents a link to a note that doesn't exist yet
    pub is_unresolved: bool,
}

/// Represents a directed edge (wikilink) in the knowledge graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub label: Option<String>,
}

/// Serializable representation of the entire graph for UI rendering.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

/// Information about a note that links to the target note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacklinkInfo {
    pub source_path: PathBuf,
    pub source_title: String,
    pub display: Option<String>,
}

/// Information about a link from the current note pointing outward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutgoingLinkInfo {
    pub target: String,
    pub resolved_path: Option<PathBuf>,
    pub display: Option<String>,
}

/// Edge weight containing link metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EdgeData {
    display: Option<String>,
}

/// Bidirectional Knowledge Graph Engine powered by `petgraph`.
pub struct KnowledgeGraph {
    graph: DiGraph<GraphNode, EdgeData>,
    path_to_node: HashMap<PathBuf, NodeIndex>,
    unresolved_to_node: HashMap<String, NodeIndex>,
    notes_cache: Vec<Note>,
}

impl KnowledgeGraph {
    /// Create an empty knowledge graph.
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            path_to_node: HashMap::new(),
            unresolved_to_node: HashMap::new(),
            notes_cache: Vec::new(),
        }
    }

    /// Construct a knowledge graph from a collection of notes.
    pub fn build(notes: &[Note]) -> Self {
        let mut graph = Self::new();
        graph.rebuild(notes);
        graph
    }

    /// Total number of nodes (both real notes and unresolved links).
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Total number of directional links (edges).
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Rebuild the complete graph topology.
    pub fn rebuild(&mut self, notes: &[Note]) {
        self.notes_cache = notes.to_vec();
        self.graph.clear();
        self.path_to_node.clear();
        self.unresolved_to_node.clear();

        // 1. Add all resolved notes as nodes
        for note in notes {
            let node_id = note.path.to_string_lossy().to_string();
            let node_data = GraphNode {
                id: node_id,
                label: note.title.clone(),
                path: Some(note.path.clone()),
                is_unresolved: false,
            };
            let idx = self.graph.add_node(node_data);
            self.path_to_node.insert(note.path.clone(), idx);
        }

        // 2. Resolve wikilinks and add edges (creating unresolved nodes when needed)
        for note in notes {
            let source_idx = match self.path_to_node.get(&note.path) {
                Some(&idx) => idx,
                None => continue,
            };

            for link in &note.wikilinks {
                let target_idx = self.resolve_or_create_target(&link.target, notes);
                let edge_data = EdgeData {
                    display: link.display.clone(),
                };
                self.graph.add_edge(source_idx, target_idx, edge_data);
            }
        }
    }

    /// Add or update a single note and recompute connections.
    pub fn add_or_update(&mut self, note: &Note) {
        if let Some(pos) = self.notes_cache.iter().position(|n| n.path == note.path) {
            self.notes_cache[pos] = note.clone();
        } else {
            self.notes_cache.push(note.clone());
        }
        let notes = std::mem::take(&mut self.notes_cache);
        self.rebuild(&notes);
    }

    /// Remove a note and rebuild graph topology.
    pub fn remove(&mut self, path: &Path) {
        if let Some(pos) = self.notes_cache.iter().position(|n| n.path == path) {
            self.notes_cache.remove(pos);
            let notes = std::mem::take(&mut self.notes_cache);
            self.rebuild(&notes);
        }
    }

    /// Find all incoming backlinks pointing to the given note.
    pub fn backlinks(&self, path: &Path) -> Vec<BacklinkInfo> {
        let node_idx = match self.path_to_node.get(path) {
            Some(&idx) => idx,
            None => return Vec::new(),
        };

        let mut backlinks = Vec::new();
        for edge in self.graph.edges_directed(node_idx, Direction::Incoming) {
            let source_idx = edge.source();
            let source_node = &self.graph[source_idx];
            if let Some(ref source_path) = source_node.path {
                backlinks.push(BacklinkInfo {
                    source_path: source_path.clone(),
                    source_title: source_node.label.clone(),
                    display: edge.weight().display.clone(),
                });
            }
        }

        backlinks
    }

    /// Find all outgoing links originating from the given note.
    pub fn outgoing_links(&self, path: &Path) -> Vec<OutgoingLinkInfo> {
        let node_idx = match self.path_to_node.get(path) {
            Some(&idx) => idx,
            None => return Vec::new(),
        };

        let mut outgoing = Vec::new();
        for edge in self.graph.edges_directed(node_idx, Direction::Outgoing) {
            let target_idx = edge.target();
            let target_node = &self.graph[target_idx];
            outgoing.push(OutgoingLinkInfo {
                target: target_node.label.clone(),
                resolved_path: target_node.path.clone(),
                display: edge.weight().display.clone(),
            });
        }

        outgoing
    }

    /// List all wikilink targets that do not correspond to any note in the vault.
    pub fn unresolved_links(&self) -> Vec<String> {
        self.unresolved_to_node.keys().cloned().collect()
    }

    /// List all notes that have no inbound links and no outbound links.
    pub fn orphan_notes(&self) -> Vec<PathBuf> {
        let mut orphans = Vec::new();

        for (path, &node_idx) in &self.path_to_node {
            let in_degree = self.graph.edges_directed(node_idx, Direction::Incoming).count();
            let out_degree = self.graph.edges_directed(node_idx, Direction::Outgoing).count();

            if in_degree == 0 && out_degree == 0 {
                orphans.push(path.clone());
            }
        }

        orphans
    }

    /// Export nodes and edges for visual rendering.
    pub fn to_graph_data(&self) -> GraphData {
        let mut nodes = Vec::with_capacity(self.graph.node_count());
        for node in self.graph.node_weights() {
            nodes.push(node.clone());
        }

        let mut edges = Vec::with_capacity(self.graph.edge_count());
        for edge in self.graph.edge_references() {
            let source_node = &self.graph[edge.source()];
            let target_node = &self.graph[edge.target()];
            edges.push(GraphEdge {
                source: source_node.id.clone(),
                target: target_node.id.clone(),
                label: edge.weight().display.clone(),
            });
        }

        GraphData { nodes, edges }
    }

    /// Resolve a wikilink target to an existing note, or create an unresolved node.
    fn resolve_or_create_target(&mut self, target: &str, notes: &[Note]) -> NodeIndex {
        let cleaned_target = target.trim();

        // 1. Check if target directly matches an existing note
        for note in notes {
            if self.is_wikilink_match(cleaned_target, note) {
                if let Some(&idx) = self.path_to_node.get(&note.path) {
                    return idx;
                }
            }
        }

        // 2. Check if already registered as an unresolved node
        if let Some(&idx) = self.unresolved_to_node.get(cleaned_target) {
            return idx;
        }

        // 3. Create a new unresolved phantom node
        let unresolved_node = GraphNode {
            id: format!("unresolved:{}", cleaned_target),
            label: cleaned_target.to_string(),
            path: None,
            is_unresolved: true,
        };
        let idx = self.graph.add_node(unresolved_node);
        self.unresolved_to_node.insert(cleaned_target.to_string(), idx);
        idx
    }

    /// Check if a wikilink matches a note by path, file stem, or title (case-insensitive).
    fn is_wikilink_match(&self, target: &str, note: &Note) -> bool {
        let target_lower = target.to_lowercase();

        // Match by title
        if note.title.to_lowercase() == target_lower {
            return true;
        }

        // Match by exact relative path
        if note.path.to_string_lossy().to_lowercase() == target_lower {
            return true;
        }

        // Match path with .md extension appended
        let path_str = note.path.to_string_lossy().to_lowercase();
        if path_str == format!("{}.md", target_lower) {
            return true;
        }

        // Match by file stem (e.g. `ideas.md` matches `[[ideas]]`)
        if let Some(stem) = note.path.file_stem().and_then(|s| s.to_str()) {
            if stem.to_lowercase() == target_lower {
                return true;
            }
        }

        // Match frontmatter aliases if available
        if let Some(ref fm) = note.frontmatter {
            for alias in &fm.aliases {
                if alias.to_lowercase() == target_lower {
                    return true;
                }
            }
        }

        false
    }
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_knowledge_graph_bidirectional_links() {
        let note_a = Note::parse(
            PathBuf::from("a.md"),
            PathBuf::from("/vault/a.md"),
            "# Note A\n\nLinks to [[Note B]] and [[Ghost Note]].".to_string(),
            Utc::now(),
        )
        .unwrap();

        let note_b = Note::parse(
            PathBuf::from("sub/b.md"),
            PathBuf::from("/vault/sub/b.md"),
            "# Note B\n\nLinks back to [[Note A|Custom Display]].".to_string(),
            Utc::now(),
        )
        .unwrap();

        let note_orphan = Note::parse(
            PathBuf::from("orphan.md"),
            PathBuf::from("/vault/orphan.md"),
            "# Lonely Note\n\nNo links anywhere.".to_string(),
            Utc::now(),
        )
        .unwrap();

        let graph = KnowledgeGraph::build(&[note_a, note_b, note_orphan]);

        // Backlinks to Note B should include Note A
        let backlinks_b = graph.backlinks(Path::new("sub/b.md"));
        assert_eq!(backlinks_b.len(), 1);
        assert_eq!(backlinks_b[0].source_title, "Note A");
        assert_eq!(backlinks_b[0].source_path, PathBuf::from("a.md"));

        // Backlinks to Note A should include Note B with custom display text
        let backlinks_a = graph.backlinks(Path::new("a.md"));
        assert_eq!(backlinks_a.len(), 1);
        assert_eq!(backlinks_a[0].source_title, "Note B");
        assert_eq!(backlinks_a[0].display, Some("Custom Display".to_string()));

        // Outgoing links from Note A
        let outgoing_a = graph.outgoing_links(Path::new("a.md"));
        assert_eq!(outgoing_a.len(), 2);
        assert!(outgoing_a.iter().any(|l| l.target == "Note B" && l.resolved_path == Some(PathBuf::from("sub/b.md"))));
        assert!(outgoing_a.iter().any(|l| l.target == "Ghost Note" && l.resolved_path.is_none()));

        // Unresolved links
        let unresolved = graph.unresolved_links();
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0], "Ghost Note");

        // Orphans
        let orphans = graph.orphan_notes();
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0], PathBuf::from("orphan.md"));

        // Export data
        let data = graph.to_graph_data();
        assert_eq!(data.nodes.len(), 4); // A, B, orphan, Ghost Note
        assert_eq!(data.edges.len(), 3); // A -> B, A -> Ghost, B -> A
    }

    #[test]
    fn test_knowledge_graph_updates() {
        let note_a = Note::parse(
            PathBuf::from("a.md"),
            PathBuf::from("/vault/a.md"),
            "# Note A\n\nNo links yet.".to_string(),
            Utc::now(),
        )
        .unwrap();

        let note_b = Note::parse(
            PathBuf::from("b.md"),
            PathBuf::from("/vault/b.md"),
            "# Note B\n\nContent.".to_string(),
            Utc::now(),
        )
        .unwrap();

        let mut graph = KnowledgeGraph::build(&[note_a, note_b]);
        assert_eq!(graph.edge_count(), 0);

        // Update note A to link to note B
        let note_a_updated = Note::parse(
            PathBuf::from("a.md"),
            PathBuf::from("/vault/a.md"),
            "# Note A\n\nNow linking to [[Note B]].".to_string(),
            Utc::now(),
        )
        .unwrap();

        graph.add_or_update(&note_a_updated);
        assert_eq!(graph.edge_count(), 1);
        assert_eq!(graph.backlinks(Path::new("b.md")).len(), 1);

        // Remove note B
        graph.remove(Path::new("b.md"));
        assert_eq!(graph.backlinks(Path::new("b.md")).len(), 0);
    }
}
