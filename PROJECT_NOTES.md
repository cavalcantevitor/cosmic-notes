# COSMIC Notes — Architecture, Design & Project Reference

This document serves as the permanent architectural reference and engineering roadmap for **COSMIC Notes**, a native, high-performance, local-first Markdown knowledge and note-taking application built for System76's **COSMIC Desktop Environment**.

---

## 1. Core Principles & Philosophy

1. **File-over-App & Local-First**: Notes are plain, human-readable CommonMark/GFM `.md` files residing in an open filesystem directory (vault) chosen by the user. No proprietary databases, no cloud lock-in.
2. **Obsidian Compatibility**: Complete interoperability with Obsidian vaults. A sidecar directory (`.cosmic-notes/`) stores transient index caches and window states without polluting user Markdown files.
3. **Native Desktop Performance**: Built in **Rust** using **`libcosmic`** and the Elm Architecture (TEA).
   - Cold startup under **300ms**.
   - Input latency locked to **60–120 FPS**.
   - Negligible idle memory usage (~25–40 MB) compared to Electron/Tauri apps (~300–800 MB).

---

## 2. System Architecture & Components

```
┌────────────────────────────────────────────────────────────────────────┐
│                        libcosmic UI Layer                              │
│   (TEA: Model, Update, View, Subscriptions, Lazy Rendering, Toasts)  │
└───────────────────▲────────────────────────────────┬───────────────────┘
                    │ Messages (TEA)                 │ User Actions / Edits
                    │                                ▼
┌───────────────────┴────────────────────────────────────────────────────┐
│                      Headless Vault Engine                             │
│  ┌───────────────────────┐  ┌─────────────────┐  ┌──────────────────┐  │
│  │   Filesystem Watcher  │  │ Indexing Engine │  │  Topology Graph  │  │
│  │  (notify + debouncer  │  │ (Tantivy BM25 + │  │  (petgraph for   │  │
│  │  + write-echo cache)  │  │  Nucleo Fuzzy)  │  │  [[wikilinks]])  │  │
│  └───────────────────────┘  └─────────────────┘  └──────────────────┘  │
│  ┌───────────────────────┐  ┌─────────────────┐                        │
│  │    Git Sync Engine    │  │ AI Agent Module │  (Future Extensions)   │
│  │ (Background Tokio Tx) │  │(MCP / Streaming)│                        │
│  └───────────────────────┘  └─────────────────┘                        │
└───────────────────────────────────┬────────────────────────────────────┘
                                    ▼
                         Local Filesystem Vault
                     (*.md files, assets, frontmatter)
```

### A. Storage & Filesystem Layer
* **Filesystem Monitoring (`notify`)**: Captures real-time external modifications (e.g. from Syncthing, Git, or external editors).
* **Write-Echo Cancellation**: Caches an atomic write token whenever the app saves a buffer to disk. When the OS file watcher notifies the app of that modification, the token matches and the event is dropped, preventing redundant re-indexing and UI flickers.
* **Debouncing**: Trailing debouncing queue (100–150ms) to allow multi-stage atomic file writes to settle.
* **Safe Deletions**: Uses the `trash` crate so deleting notes sends them to the system trash bin.

### B. Search & Indexing Engine (Dual-Tier for Max Speed)

#### Performance Rationale & Benchmarks
Tantivy was chosen as the primary full-text engine because it delivers the highest raw performance across all four critical dimensions in the Rust ecosystem:

| Metric / Dimension | Tantivy (Chosen) | SQLite + FTS5 | Ripgrep / Live Linear Grep |
|---|---|---|---|
| **Query Latency (10k notes)** | **< 0.5 ms** (Sub-millisecond) | 2.0 – 6.0 ms (SQL parser & FFI overhead) | 10.0 – 50.0 ms (Linear $O(N)$ scan) |
| **Ingestion / Index Speed** | **~50–100 MB/s per core** (<1s for 10k notes) | ~10–20 MB/s (WAL & B-Tree overhead) | N/A (No persistent index) |
| **Memory Architecture** | **Zero-Copy `mmap`** (Kernel page-cached) | SQLite cache buffer pool in heap | Scans entire memory/disk |
| **CPU Efficiency** | **AVX2 / SIMD bit-packing** (4+ GB/s decompression) | Traditional B-Tree traversals | AVX2 memchr |
| **Scaling Complexity** | **$O(\log N)$** (Consistent across 100k+ notes) | $O(\log N)$ | **$O(N)$** (Degrades linearly) |

#### Two-Tier Search Implementation
* **Tier 1 (Instant Quick Switcher — `Ctrl+P`)**:
  * **Engine**: `nucleo` (Helix editor's multithreaded pure-Rust fuzzy matcher).
  * **Latency**: `< 0.1 ms` in-memory matching on note titles, paths, and tags at 120 FPS.
* **Tier 2 (Full-Text Deep Vault Search — `Ctrl+Shift+F`)**:
  * **Engine**: `Tantivy` (pure Rust, SIMD bitpacking, memory-mapped `mmap` inverted index).
  * **Latency**: `< 0.5 ms` query time across tens of thousands of notes with BM25 ranking and snippet generation.
* **Knowledge Graph**: `petgraph` maintains in-memory forward edges and backlink maps for `[[wikilinks]]`.

### C. Editor Surface Strategy: Analysis of Obsidian vs. `cosmic-text`

#### Why Mirroring Obsidian's Live Preview 100% Requires an Entire Custom Engine
Obsidian is built on web technologies (CodeMirror 6 inside Chromium/Electron), which relies on:
1. **Replace Decorations**: Hiding characters (e.g. `**` or `#`) from visual rendering without mutating the underlying document buffer.
2. **Widget Decorations**: Injecting arbitrary interactive HTML DOM elements (live checkboxes `<input type="checkbox">`, images, tables) directly inside a line of wrapped text.
3. **Unified Browser Layout Engine**: Chromium Blink measures, breaks lines, and hit-tests across mixed text glyphs and embedded DOM widgets simultaneously.

In native Rust / `libcosmic`, the architecture is strictly bifurcated:
* **`cosmic-text`** handles **glyphs only**: It shapes text along lines. It has no concept of replace decorations or zero-width virtual character hiding, and cannot host GUI widgets inside a text run. Deleting delimiter characters from the buffer changes string length, causing cursor drift, byte-offset desynchronization, and undo/redo corruption.
* **`Iced` / `libcosmic`** handles **widgets only**: It calculates rectangular boxes via Taffy flexbox. It cannot inject a widget inside a glyph run.

#### The Chosen Solution: Path 2 (Mode-Toggle & Split Workspace)
* Built using `cosmic::widget::pane_grid`:
  1. **Edit Mode**: Fast, focused typing using `cosmic-text` buffers with syntax highlighting and line numbers.
  2. **Split Mode**: Side-by-side Editor and live Markdown preview with synchronized scrolling.
  3. **Preview Mode**: Clean, distraction-free rendered Markdown.
* **Frame-Rate Protection**: The preview pane is wrapped in `iced::widget::lazy` keyed on a `(note_id, content_hash)` tuple to avoid re-rendering the widget tree on intermediate keystrokes.
* **Future Evolution**: Evolve toward the **Block-Sliced Model** (Notion/Logseq style: paragraphs as text blocks, interactive checkboxes/tables as native widgets in a column) or in-buffer pseudo-conceal via `AttrsList` alpha transparency.

---

## 3. UI Layout & COSMIC Design System

* **Header Bar**:
  * Left: Vault switcher dropdown, history back/forward navigation.
  * Center: Global search / Quick Switcher (`Ctrl+P`).
  * Right: View mode segmented controls (`Edit` | `Split` | `Preview`), Context Drawer toggle.
* **Left Sidebar (Collapsible)**:
  * Column 1: Folder directory tree, tag taxonomy (`#tag`), favorites.
  * Column 2: Note list for current folder/tag with sorting and filters.
* **Center Surface**:
  * Active document in `pane_grid` (Editor / Preview / Split).
* **Right Context Drawer (`cosmic::app::context_drawer`)**:
  * Document statistics (word count, reading time), backlinks, outgoing links, and future AI Assistant panel.

---

## 4. Architecture Provisions for Future Additions

### A. Git Sync Integration (Provisioning Strategy)

To allow seamless Git synchronization (auto-commit, push/pull, GitHub/GitLab backup) in future updates:

1. **Vault as a Git Repository**:
   - Any vault folder can be initialized as a Git repository (`git init`).
   - Auto-generated `.gitignore` placed in the vault root:
     ```gitignore
     # Ignore transient indexes and caches
     .cosmic-notes/index/
     .cosmic-notes/cache/
     *.tmp
     ```
2. **Storage Abstraction Trait**:
   - Vault file operations will be modeled through an abstract trait:
     ```rust
     pub trait VaultSyncEngine: Send + Sync {
         async fn sync(&self) -> Result<SyncSummary, SyncError>;
         async fn commit_and_push(&self, msg: &str) -> Result<(), SyncError>;
         async fn pull(&self) -> Result<Vec<PathBuf>, SyncError>;
     }
     ```
   - Implemented using pure Rust `gix` (gitoxide) or `git2`.
3. **Background Sync Worker**:
   - Runs as an asynchronous Tokio task inside `cosmic::iced::Subscription`.
   - Emits messages like `Message::SyncStarted`, `Message::SyncCompleted`, `Message::SyncConflict(PathBuf)`.
   - Because our **Write-Echo Cancellation** is already designed, incoming Git pulls will be recognized by the watcher and automatically reload clean buffers without UI freezes.

---

### B. AI Agent Integration (Provisioning Strategy)

To integrate AI capabilities (local LLMs via Ollama/llama.cpp, cloud APIs, auto-tagging, summarization, semantic search, and autonomous note agents):

1. **Headless Engine as Agent Tooling (MCP-Ready)**:
   - The AI Agent interface interacts with the vault through a clean, decoupled API:
     ```rust
     pub trait AgentVaultApi: Send + Sync {
         async fn read_note(&self, path: &Path) -> Result<String, VaultError>;
         async fn write_note(&self, path: &Path, content: &str) -> Result<(), VaultError>;
         async fn append_note(&self, path: &Path, content: &str) -> Result<(), VaultError>;
         async fn search_notes(&self, query: &str) -> Result<Vec<SearchResult>, VaultError>;
         async fn get_backlinks(&self, note_id: &str) -> Result<Vec<String>, VaultError>;
     }
     ```
   - This exact interface can be exposed directly as **MCP Tools (Model Context Protocol)** or native agent function-calling tools.
2. **Asynchronous Streaming via TEA**:
   - Streaming LLM token generation fits naturally into `cosmic::iced::stream::channel`:
     ```rust
     #[derive(Clone, Debug)]
     pub enum AgentMessage {
         TokenStream(String),
         ActionProposed(AgentAction),
         Complete,
         Error(String),
     }
     ```
3. **Dedicated UI Slot in Context Drawer**:
   - The right-hand `context_drawer` will reserve an `Agent` tab alongside `Inspector` and `Backlinks`.
   - Provides a conversational chat interface with the user's active vault context.
4. **Semantic Vector Embeddings (Alongside Tantivy)**:
### 5. Multi-Column Architecture & Native Context Drawer
- **Sidebar Navigation**:
  - Direct inspiration from System76's `cosmic-files` app.
  - Multi-section hierarchy: "Library" (All Notes), "Tags" (interactive tag filtering), and "Notes" (real-time filtered note list).
  - Collapsible via `sidebar-places-symbolic` button in the header bar.
- **Top Header Bar**:
  - Top-Left: Direct text menu buttons (`New Note`, `Edit`, `Split`, `Preview`).
  - Top-Right: Pill-rounded inline search bar (`widget::search_input`) expanding directly adjacent to the magnifying glass icon (`system-search-symbolic`), alongside the Inspector toggle icon (`dialog-information-symbolic`).
- **Native Context Drawer (`cosmic::app::ContextDrawer`)**:
  - Housed on the right side of the window, accessible via header info button or `Ctrl+Space`.
  - Properties & Telemetry: Word count, character count, reading time estimate, modified date, and path.
  - Tags Inspector: Extracted `#tags` with click-to-filter capability.
  - Connections: Interactive bidirectional link topology showing incoming backlinks and outgoing wikilinks.

---

## 5. Technology Stack Summary

| Layer | Library / Crate | Purpose |
|---|---|---|
| **GUI Framework** | `libcosmic` (System76) | Native COSMIC desktop widgets & theming |
| **Runtime & Event Loop**| `iced` / `tokio` | The Elm Architecture (TEA), async subscriptions |
| **Text Typography** | `cosmic-text` | Font shaping, fallback, and multi-line editing |
| **Split Pane Layout** | `cosmic::widget::pane_grid` | User-resizable Editor / Preview split screen |
| **Full-Text Search** | `tantivy` | Sub-millisecond BM25 keyword search |
| **Quick Switcher** | `nucleo` | Microsecond fuzzy matching for `Ctrl+P` |
| **Filesystem Watcher** | `notify` + debouncer | Real-time disk sync & write-echo cancellation |
| **Knowledge Graph** | `petgraph` | Networked thought, `[[wikilinks]]`, backlinks |
| **Safe Deletions** | `trash` | Move deleted notes to system trash bin |
| **Settings Sync** | `cosmic-config` | Syncs app preferences with COSMIC settings |

---

## 6. Implementation Roadmap & Current Status

| Phase | Description | Status | Details |
|---|---|---|---|
| **Phase 0** | Workspace & Tooling Initialization | **Completed** | Git repo on `main`, Cargo.toml with 2024 edition & all core deps, local `.pkgconfig` for xkbcommon linking, root `.gitignore`. |
| **Phase 1** | Headless Vault Engine Foundation | **Completed** | `Note`, `Frontmatter`, inline tags, wikilinks, `WriteEchoCache`, atomic disk saves, safe trash deletion, `VaultWatcher` via `notify-debouncer-full`, passing test suite. |
| **Phase 1+ (Bridge)** | Interactive Desktop Shell & Live Editor | **Completed** | Native `libcosmic` application (`src/app.rs`), note selection sidebar, interactive `text_editor` widget, live Split-view markdown preview, auto-save to vault. |
| **Phase 2** | Dual-Tier Search & Knowledge Graph Engine | **Completed** | In-memory `nucleo` fuzzy matcher (`Ctrl+P`), `tantivy` BM25 inverted index in `.cosmic-notes/index/` (`Ctrl+Shift+F`), and `petgraph` bidirectional link topology with backlinks and orphan detection. |
| **Phase 2.5** | UI/UX & Native COSMIC Files Polish | **Completed** | Native COSMIC Files styling: top-left menu actions (`New Note`, `Edit`, `Split`, `Preview`), top-right search button adjacent to window controls, dark sidebar background, and borderless editor without focus rings. Cleaned clutter (removed tabs, breadcrumbs, pencil icon). |
| **Phase 3** | Multi-Column COSMIC Shell & Context Drawer | **Completed** | Inline header search input, collapsible sidebar with toggle button, tag filtering, and native right-hand Context Drawer with telemetry, tags, and bidirectional links. |
| **Phase 3.5** | Polish & Cohesive Navigation Architecture | **In Progress** | Fix sidebar toggle icon (`sidebar-places-symbolic`), pill-rounded search (`widget::search_input`), and unified visual harmony between left sidebar and right Context Drawer. |
| **Phase 4** | Pane Grid Workspace & 120 FPS Lazy Rendering | **Upcoming** | Resizable split layout via `cosmic::widget::pane_grid`, cached preview with `iced::widget::lazy` keyed on `(note_id, content_hash)`. |
| **Phase 5** | Quick Switcher & Full-Text Search Overlays | **Upcoming** | Global search palette modal (`Ctrl+P`) and full-text search results panel (`Ctrl+Shift+F`). |
| **Phase 6** | Future-Proof Contracts (Git & AI/MCP) | **Upcoming** | `VaultSyncEngine` trait and `AgentVaultApi` MCP-ready tool abstraction. |

