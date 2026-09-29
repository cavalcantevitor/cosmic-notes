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

