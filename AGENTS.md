# COSMIC Notes — Agent Guidelines & Project Operating Manual

This file serves as the mandatory operational guide for AI agents working on the **COSMIC Notes** codebase. Every time a new session starts, adhere strictly to these principles, technical architectures, and development standards.

---

## 1. Project Mission & Identity

* **Project**: **COSMIC Notes**
* **Target Environment**: System76's **COSMIC Desktop Environment** running on **Pop!_OS 24.04+**.
* **Language & Framework**: **Rust (2024 edition, 1.95+)** using **`libcosmic`** (System76's GUI toolkit based on Iced and The Elm Architecture).
* **Core Philosophy**: **Local-First, File-over-App**. User notes are human-readable CommonMark/GFM `.md` files in a standard filesystem directory (vault) fully interoperable with Obsidian.

---

## 2. Architectural Invariants (Do Not Violate)

1. **Strict Engine/UI Decoupling**:
   * Never couple disk I/O, Tantivy queries, or graph operations directly inside UI widgets or the `view()` method.
   * All storage, indexing, and graph operations live in the **Headless Vault Engine** and communicate with the `libcosmic` UI purely through typed Elm messages (`Message`) and asynchronous tasks (`Task<cosmic::Action<Message>>`).
2. **Write-Echo Cancellation**:
   * Any file save initiated by the application must register an atomic write token.
   * The `notify` filesystem watcher must cross-reference incoming OS file events against this token cache to drop self-initiated echoes and avoid redundant re-indexing loops or UI flickers.
3. **Safe Deletions**:
   * Never permanently delete user files with `std::fs::remove_file` directly. Always move files to the system trash bin using the `trash` crate.
4. **Frame-Rate & View Purity (120 FPS Target)**:
   * Keep `cosmic::Application::view(&self)` pure, lightweight, and fast.
   * Always wrap expensive layout trees (such as the rendered Markdown preview) in `iced::widget::lazy` keyed on a `(note_id, content_hash)` tuple to prevent GPU redraw spikes during active typing.
5. **Search Architecture**:
   * **Quick Switcher (`Ctrl+P`)**: Handled by **`nucleo`** in-memory fuzzy matching on titles, paths, and tags for $<0.1\text{ ms}$ response.
   * **Full-Text Vault Search (`Ctrl+Shift+F`)**: Handled by **`Tantivy`** with memory-mapped (`mmap`) inverted indexes for $<0.5\text{ ms}$ BM25 search.
6. **Workspace / Editor Model (Path 2)**:
   * Use `cosmic::widget::pane_grid` to support:
     * **Edit Mode**: Focused typing via `cosmic-text` with syntax highlighting and line numbers.
     * **Split Mode**: Side-by-side Editor and live Markdown preview with synchronized scrolling.
     * **Preview Mode**: Clean reading view.
7. **Hurff Five-State UI Rule**:
   * Every screen, search overlay, and navigation panel must explicitly support the **Hurff Five States**:
     * **Ideal State**: Clear typographic hierarchy and structured lists.
     * **Empty State**: Friendly zero-data visual with clear call-to-action (CTA) (e.g. "No notes found", "Create your first note").
     * **Partial State**: Resilient layout when notes have missing tags or incomplete frontmatter.
     * **Loading State**: Responsive non-blocking cues for async indexing or disk scans.
     * **Error State**: Informative, non-blocking alerts that **never lose uncommitted user input**.
8. **Regression Prevention & Verification Gate**:
   * New iterations must not break existing working workflows. Every milestone or feature pull requires running the full smoke and boundary regression test suite (`cargo test --all-targets`) with zero compiler warnings and zero test failures before committing.

---

## 3. Future-Proofing Contracts

* **Git Sync Readiness**:
  * Vault file operations must implement the `VaultSyncEngine` trait abstraction so pure-Rust Git (`gix` / `git2`) can be plugged in without refactoring vault controllers.
  * Ensure the `.cosmic-notes/` sidecar folder contains an auto-generated `.gitignore` excluding `index/`, `cache/`, and `*.tmp`.
* **AI Agent / MCP Readiness**:
  * Implement all programmatic note operations behind the `AgentVaultApi` trait (`read_note`, `write_note`, `search_notes`, `get_backlinks`).
  * These methods must be 100% compatible with MCP (Model Context Protocol) tool definitions.
  * Reserve an `AI Assistant` tab in the right-hand `context_drawer`.
  * Support streaming LLM token updates via `cosmic::iced::stream::channel`.

---

## 4. Coding Standards & Conventions

1. **Idiomatic Rust**:
   * Strictly avoid unhandled `unwrap()` or `expect()` in production runtime paths. Use `thiserror` for library domain errors and `anyhow` for top-level application orchestration.
   * Keep functions modular, well-commented, and unit-tested.
2. **COSMIC Design Language**:
   * Spacing: Always query dynamic system spacing via `cosmic::theme::spacing()` (`space_xs`, `space_s`, `space_m`, etc.). Never hardcode arbitrary padding pixels unless strictly necessary.
   * Colors & Themes: Rely on `cosmic::theme` and `cosmic_config` for automatic light/dark mode and system accent color adaptation.
   * Icons: Use `cosmic::widget::icon::from_name(...)` to inherit standard COSMIC system icon themes.
3. **Build & Quality Assurance**:
   * Always verify that changes compile cleanly with `cargo check`.
   * Run `cargo clippy -- -D warnings` to enforce zero lint warnings.

---

## 5. Git Commit Standards (Conventional Commits v1.0.0)

Every commit in this repository MUST strictly follow the [Conventional Commits v1.0.0](https://www.conventionalcommits.org/en/v1.0.0/) specification:

### Format:
```text
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

### Allowed Types:
* **`feat`**: A new user-facing feature or capability (correlates with `MINOR` in SemVer).
* **`fix`**: A bug fix (correlates with `PATCH` in SemVer).
* **`docs`**: Documentation changes only (e.g. `PROJECT_NOTES.md`, `README.md`, doc comments).
* **`style`**: Changes that do not affect the meaning of the code (white-space, formatting, missing semi-colons, etc.).
* **`refactor`**: A code change that neither fixes a bug nor adds a feature.
* **`perf`**: A code change that improves performance (e.g. Tantivy index optimizations, lazy rendering).
* **`test`**: Adding missing tests or correcting existing tests.
* **`build`**: Changes that affect the build system or external dependencies (`Cargo.toml`, CI configs).
* **`ci`**: Changes to our CI configuration files and scripts.
* **`chore`**: Other changes that don't modify `src` or test files (e.g. `.gitignore`, release scripts).
* **`revert`**: Reverts a previous commit.

### Rules:
1. **Imperative, lower-case subject**: Write `<description>` in imperative present tense: e.g., `feat(vault): add write-echo cancellation token cache` (NOT `added` or `adds`).
2. **Scope**: Always use a clear scope when modifying a specific subsystem, e.g., `feat(editor): ...`, `fix(watcher): ...`, `perf(search): ...`, `docs(agents): ...`.
3. **Breaking Changes**: A breaking change must be indicated by a `!` immediately before the `:` (e.g., `feat(vault)!: redesign note storage model`) and explained in the footer starting with `BREAKING CHANGE: <explanation>`.

---

## 6. Critical Project Reference Files

* [`PROJECT_NOTES.md`](file:///home/cavalcantevitor/Documents/codex_projects/personal_projects/cosmic_notes/PROJECT_NOTES.md): Comprehensive system specification, search benchmarks, Obsidian comparison, and detailed Git/AI roadmap.
* [`.agents/skills/libcosmic-dev/SKILL.md`](file:///home/cavalcantevitor/Documents/codex_projects/personal_projects/cosmic_notes/.agents/skills/libcosmic-dev/SKILL.md): Expert widget guide, Elm architecture lifecycle, and production patterns extracted from `cosmic-edit`, `cedilla`, and `cosmic-files`.
* [`.agents/skills/libcosmic-dev/references/notes-app-architecture.md`](file:///home/cavalcantevitor/Documents/codex_projects/personal_projects/cosmic_notes/.agents/skills/libcosmic-dev/references/notes-app-architecture.md): Implementation code patterns for vault notes, async tasks, and view mode state.
* [`.agents/skills/libcosmic-dev/references/search-and-indexing.md`](file:///home/cavalcantevitor/Documents/codex_projects/personal_projects/cosmic_notes/.agents/skills/libcosmic-dev/references/search-and-indexing.md): Production implementation patterns for Tantivy BM25 full-text indexing and Nucleo fuzzy matching.
* [`.agents/skills/libcosmic-dev/references/workspace-layout.md`](file:///home/cavalcantevitor/Documents/codex_projects/personal_projects/cosmic_notes/.agents/skills/libcosmic-dev/references/workspace-layout.md): Implementation patterns for `pane_grid` split views and lazy-cached Markdown preview.

---

## 7. Current Project State & Session Handoff (Where to Continue)

### Completed Milestones:
1. **Milestone 0 (M0: Scaffold — `v0.1.0-alpha`)**:
   - Initialized Git repository on branch `main`.
   - `Cargo.toml` configured with Rust 2024 edition (1.95+) and all core dependencies (`libcosmic`, `tokio`, `notify-debouncer-full`, `tantivy`, `nucleo`, `petgraph`, `trash`, `pulldown-cmark`, `serde_yaml`, `thiserror`, `anyhow`).
   - Wayland `xkbcommon` linking configured via local `.pkgconfig/` and `.cargo/config.toml`.
   - Headless Vault Engine (`src/core/`): domain model, frontmatter parsing, `WriteEchoCache`, `VaultWatcher` with debounced file notifications, safe deletion via `trash`.
   - Integration tests passing (`tests/vault_integration.rs`).
2. **Milestone 1 (M1: Core Editor — `v0.2.0-alpha`)**:
   - `cosmic::Application` desktop shell implementation (`src/app.rs`).
   - Multi-line live text editor (`cosmic::widget::text_editor`) with automatic disk persistence and write-echo cancellation.
   - Initial view modes (Edit, Split, Preview) with AST Markdown rendering.
3. **Milestone 2 (M2: Knowledge & Search — `v0.3.0-alpha`)**:
   - Microsecond fuzzy matcher (`nucleo`) in `src/search/fuzzy.rs` for title, path, and tag querying.
   - Sub-millisecond Tantivy BM25 inverted index in `src/search/fulltext.rs` with field boosting and highlighted snippets.
   - Bidirectional knowledge graph (`petgraph`) in `src/graph/mod.rs` resolving wikilinks, backlinks, and orphan notes.
   - Unified `VaultIndex` coordinator in `src/search/mod.rs` reactive to `VaultEvent` disk mutations.
4. **Milestone 3 (M3: COSMIC Shell — `v0.4.0-alpha`)**:
   - Realigned layout to match native **COSMIC Files** (`cosmic-files`).
   - Symmetrical 3-pane navigation shell: 260px left Explorer and 260px right Inspector with `Container::Background` (dark slate navy) and 1px vertical dividers.
   - Header bar with `sidebar-places-symbolic` toggle, text action buttons, and capsule search input (`widget::search_input` with `radius_xl`).
   - Real-time note telemetry (word count, reading time, modified date, relative path), tags badges, and bidirectional backlinks explorer.
   - Resizable side-by-side split layout using `cosmic::widget::pane_grid`.
   - 120 FPS frame-rate protection with cached AST markdown parsing keyed on content hash.
   - Synchronized scrolling in split view.
   - UI/UX Design QA and Quality of Life audit completed ([`QOL_REPORT.md`](file:///home/cavalcantevitor/Documents/codex_projects/personal_projects/cosmic_notes/QOL_REPORT.md)).
   - Full test suite passing (18/18 unit and integration tests passing, 0 warnings).

5. **Milestone 4 (M4: Search Experience — `v0.5.0-beta`)**:
   - Global `Ctrl+P` modal quick-switcher palette with `nucleo` fuzzy matching over note titles, paths, and tags.
   - Global `Ctrl+Shift+F` full-text deep search overlay with Tantivy BM25 highlighted snippets and field boosting.
   - Global keyboard accelerators (`Ctrl+\`, `Ctrl+N`, `Ctrl+1/2/3`, `Ctrl+F`, `Ctrl+P`, `Ctrl+Shift+F`, `Ctrl+I`).
   - Modal arrow navigation (`ArrowDown`, `ArrowUp`, `Enter`, `Escape`).
   - System tooltips on header actions and Hurff zero-match empty search states.
6. **Milestone 5 (M5: Ecosystem Extensibility — `v0.6.0-beta`)**:
   - Trait definition `VaultSyncEngine` (`src/sync/mod.rs`) for pluggable Git synchronization (`GitCliSyncEngine`, `MockSyncEngine`).
   - Trait definition `AgentVaultApi` (`src/agent/mod.rs`) for AI Agents with `DefaultAgentVaultApi`.
   - 100% Model Context Protocol (MCP) tool definitions (`vault_read_note`, `vault_write_note`, `vault_search_notes`, `vault_get_backlinks`, `vault_list_notes`).
   - End-to-end integration and smoke tests in `tests/smoke_regression.rs`.

7. **Milestone 6 (M6: Vault Hierarchy & Folders — `v0.7.0-beta`)**:
   - Collapsible directory tree (chevron-style `▶` / `▼`) with folder icons, note count badges, and indented nesting.
   - Multi-method folder creation: top header button, sidebar `+` button, inline tree row, modal dialog (`Ctrl+Shift+N`), and path-based auto-creation (`Ideas/App/note.md`).
   - Full folder lifecycle: create notes inside folders, create nested subfolders, inline folder rename, and safe folder deletion to trash.
   - UI/UX polish: plain-text Vault header with note count badge, centered zero-match search/filter states, and aligned snippet previews in deep search.
   - Verified with 31 automated tests (`tests/smoke_regression.rs`).

### Active Milestone:
* **Milestone 7 (M7: Quality & Governance — `v0.8.0-beta`)**:
  - Full Hurff 5-State UI audit across all views.
  - Release Checkpoint Matrix (Architecture & Security, UX Resilience, Quantitative Performance, Functional QA & Regression).
  - Continuous deterministic quality verification via `./scripts/verify.sh`.

### Planned Future Milestones:
* **v1.0: GA Release (`v1.0.0`)**:
  - Packaging (Flatpak/COSMIC Store), settings persistence via `cosmic-config`, first stable user release.



