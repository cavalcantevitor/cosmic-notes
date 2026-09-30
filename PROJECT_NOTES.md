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
### 5. Symmetrical 3-Pane Architecture (Docked Explorer & Inspector)
- **Left Column (Notes & Tags Explorer)**:
  - 260px docked panel with `Container::Background` (dark slate navy).
  - Clean "All Notes" view with total note count badge (no artificial "Library" header).
  - Tags section with interactive click-to-filter capability.
  - Notes list with document icons, titles, formatted dates, and rounded selection highlight.
  - Collapsible via `sidebar-places-symbolic` button in the header bar.
- **Center Column (Distraction-Free Workspace)**:
  - Full-height borderless text editor (`cosmic::widget::text_editor`) and live Markdown preview.
  - No redundant tabs or breadcrumbs; pure focus on writing.
- **Right Column (Docked Note Inspector)**:
  - 260px docked panel with `Container::Background` (exact visual symmetry with left column).
  - Properties & Telemetry: Word count, character count, reading time estimate, modified date, and path.
  - Tags Section: Extracted `#tags` with filter triggers.
  - Connections Section: Incoming backlinks and outgoing wikilinks with direct one-click navigation.
  - Toggled independently via `dialog-information-symbolic` button in the header bar.
- **Top Header Bar**:
  - Top-Left: `sidebar-places-symbolic` toggle and clean text menu buttons (`New Note`, `Edit`, `Split`, `Preview`).
  - Top-Right: Pill-rounded inline search bar (`widget::search_input`) expanding directly adjacent to the magnifying glass icon (`system-search-symbolic`), alongside the Inspector toggle icon (`dialog-information-symbolic`).

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

## 6. Implementation Roadmap & Milestones

| Target Milestone | Release Tag | Primary Capability Theme | Status | Scope & Deliverables |
|---|---|---|---|---|
| **M0: Scaffold** | `v0.1.0-alpha` | **Workspace & Foundation** | **Completed** | Cargo 2024 workspace, `.pkgconfig` for `xkbcommon`, `Note`/`Frontmatter` models, `WriteEchoCache`, `VaultWatcher` via `notify-debouncer-full`. |
| **M1: Core Editor** | `v0.2.0-alpha` | **Desktop Shell & Editing** | **Completed** | Native `libcosmic` shell (`src/app.rs`), interactive `cosmic-text` buffer, live split-preview, atomic disk saves, safe trash deletion. |
| **M2: Knowledge & Search** | `v0.3.0-alpha` | **Indexing & Bidirectional Links** | **Completed** | In-memory `nucleo` fuzzy switcher, `tantivy` BM25 inverted index in `.cosmic-notes/index/`, `petgraph` link graph, backlink/orphan resolution. |
| **M3: COSMIC Shell** | `v0.4.0-alpha` | **Symmetrical 3-Pane Workspace** | **Completed** | Dual 260px docked panels (Explorer & Inspector), pill search input, context drawer telemetry, pane grid workspace with `lazy` AST caching (120 FPS target). |
| **M4: Search Experience** | `v0.5.0-beta` | **Modal Overlays & Hotkeys** | **Active** | Floating Quick Switcher palette (`Ctrl+P`), deep-search overlay panel (`Ctrl+Shift+F`), tooltips, keyboard navigation, and zero-match states. |
| **M5: Ecosystem Extensibility** | `v0.6.0-beta` | **Sync & Agent Abstractions** | **Planned** | Trait definitions for `VaultSyncEngine` (`gix`/`git2`) and MCP-compatible `AgentVaultApi` (`cosmic::iced::stream::channel`). |
| **M6: Quality & Governance** | `v0.7.0-beta` | **Quality Engineering & Verification** | **Planned** | Hurff 5-State UI audit across all views, automated smoke & boundary test suite, Antigravity deterministic hooks, and 4-phase Release Checkpoint Matrix. |
| **v1.0: GA Release** | `v1.0.0` | **General Availability** | **Planned** | Packaging (Flatpak/COSMIC Store), settings persistence via `cosmic-config`, first stable user release. |
