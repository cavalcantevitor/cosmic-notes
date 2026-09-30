# COSMIC Notes

<div align="center">

![COSMIC Desktop Environment](https://img.shields.io/badge/Desktop-COSMIC%20DE-blue?style=for-the-badge&logo=linux)
![Rust 2024](https://img.shields.io/badge/Rust-2024%20Edition-orange?style=for-the-badge&logo=rust)
![libcosmic](https://img.shields.io/badge/GUI-libcosmic%20(Iced)-purple?style=for-the-badge)
![License: GPL v3](https://img.shields.io/badge/License-GPLv3-green.style=for-the-badge)

**A native, high-performance, local-first Markdown notes app built for System76's COSMIC Desktop Environment on Pop!_OS.**

[Features](#-features) • [Architecture](#-architecture) • [Getting Started](#-getting-started) • [Quality Assurance](#-quality-assurance) • [Roadmap](#-roadmap)

</div>

---

## 🌟 Philosophy & Mission

COSMIC Notes is designed around a single uncompromising principle: **Local-First, File-over-App**.

Your notes belong to you. They are standard CommonMark / GitHub Flavored Markdown (`.md`) files stored in a regular filesystem directory on your machine. There are no proprietary database blobs, no vendor lock-in, and full interoperability with tools like Obsidian, VS Code, and Git.

COSMIC Notes pairs this open-file philosophy with the visual elegance and speed of System76's next-generation **COSMIC Desktop Environment**.

---

## ✨ Features

- ⚡ **Native COSMIC Experience**: Built with [`libcosmic`](https://github.com/pop-os/libcosmic) using The Elm Architecture (TEA) and GPU-accelerated rendering. Harmonizes directly with system accents, light/dark themes, and COSMIC HIG.
- 📐 **Symmetrical 3-Pane Navigation**:
  - **Explorer Column (Left, 260px)**: Fast note browsing, folder counts, and clickable `#tag` filtering.
  - **Distraction-Free Canvas (Center)**: Borderless text editor with seamless writing surface and zero outline rings.
  - **Inspector Column (Right, 260px)**: Real-time telemetry (word count, reading time), tag badges, and bidirectional link topology.
- 🪟 **Resizable Split Workspace (`pane_grid`)**:
  - **Edit Mode**: Focused typing canvas.
  - **Split Mode**: Resizable side-by-side editing and live Markdown preview with synchronized cursor-driven scrolling.
  - **Preview Mode**: Clean, full-canvas reading view.
- 🔍 **Dual-Tier Sub-Millisecond Search**:
  - **Microsecond Fuzzy Matcher**: Powered by [`nucleo`](https://github.com/helix-editor/nucleo) for $<0.1\text{ ms}$ search across titles, paths, and tags.
  - **Tantivy BM25 Full-Text Index**: Disk-persisted inverted index with field boosting and keyword snippet highlighting in $<0.5\text{ ms}$.
- 🕸️ **Bidirectional Knowledge Graph**:
  - Author with `[[wikilinks]]`.
  - Automatic incoming backlink detection and outgoing link resolution powered by [`petgraph`](https://github.com/petgraph/petgraph).
  - Visual indicators for resolved notes and uncreated ghost notes (`⤑`).
- 🛡️ **Reliability & Safety**:
  - **Write-Echo Cancellation**: Thread-safe `WriteEchoCache` with atomic tokens prevents filesystem watcher loops during auto-saves.
  - **Safe Deletion**: Deleting notes safely moves files to the system trash bin using the [`trash`](https://crates.io/crates/trash) crate (no unrecoverable deletions).

---

## 🏗️ Architecture

```mermaid
flowchart TD
    subgraph "Desktop Shell (UI Layer)"
        UI["libcosmic / Iced Application"]
        PANE["pane_grid Split Workspace"]
        NAV["Symmetrical 3-Pane Shell"]
    end

    subgraph "Headless Vault Engine (Core)"
        VAULT["Vault File Controller"]
        ECHO["WriteEchoCache"]
        WATCH["Debounced Watcher (notify)"]
        TRASH["System Trash Integration"]
    end

    subgraph "Search & Topology Layer"
        NUC["Nucleo In-Memory Fuzzy Engine"]
        TAN["Tantivy BM25 Inverted Index"]
        GRAPH["Petgraph Bidirectional Topology"]
    end

    UI <-->|"Elm Messages & Tasks"| VAULT
    VAULT --> ECHO
    ECHO --> WATCH
    VAULT --> NUC
    VAULT --> TAN
    VAULT --> GRAPH
```

---

## 🚀 Getting Started

### Prerequisites

- **Rust**: Version 1.85+ (2024 Edition support).
- **System Dependencies** (Pop!_OS / Ubuntu):
  ```bash
  sudo apt install -y build-essential libxkbcommon-dev libwayland-dev pkg-config
  ```

### Build & Run

1. Clone the repository:
   ```bash
   git clone https://github.com/cavalcantevitor/cosmic-notes.git
   cd cosmic-notes
   ```

2. Run the application:
   ```bash
   cargo run --release
   ```

3. Run the automated test suite:
   ```bash
   cargo test --lib --tests
   ```

---

## 🧪 Quality Assurance

COSMIC Notes undergoes continuous **Design QA** and Quality of Life (QoL) audits based on industry-standard UI/UX methodologies:
- Evaluated against [Business of Apps QA Framework](https://www.businessofapps.com/insights/the-role-of-quality-assurance-in-ui-ux-design/) and [AppLighter Mobile/Desktop Testing Pillars](https://www.applighter.com/blog/app-quality-assurance).
- Comprehensive audit report available in [`QOL_REPORT.md`](QOL_REPORT.md).
- Strict Conventional Commits v1.0.0 git standards maintained across all branches.

---

## 🗺️ Roadmap & Milestones

| Target Milestone | Release Tag | Primary Capability Theme | Status | Scope & Deliverables |
|---|---|---|---|---|
| **M0: Scaffold** | `v0.1.0-alpha` | **Workspace & Foundation** | **Completed** | Cargo 2024 workspace, `.pkgconfig` for `xkbcommon`, `Note`/`Frontmatter` models, `WriteEchoCache`, `VaultWatcher` via `notify-debouncer-full`. |
| **M1: Core Editor** | `v0.2.0-alpha` | **Desktop Shell & Editing** | **Completed** | Native `libcosmic` shell (`src/app.rs`), interactive `cosmic-text` buffer, live split-preview, atomic disk saves, safe trash deletion. |
| **M2: Knowledge & Search** | `v0.3.0-alpha` | **Indexing & Bidirectional Links** | **Completed** | In-memory `nucleo` fuzzy switcher, `tantivy` BM25 inverted index in `.cosmic-notes/index/`, `petgraph` link graph, backlink/orphan resolution. |
| **M3: COSMIC Shell** | `v0.4.0-alpha` | **Symmetrical 3-Pane Workspace** | **Completed** | Dual 260px docked panels (Explorer & Inspector), pill search input, context drawer telemetry, pane grid workspace with AST caching (120 FPS target). |
| **M4: Search Experience** | `v0.5.0-beta` | **Modal Overlays & Hotkeys** | **Completed** | Floating Quick Switcher palette (`Ctrl+P`), deep-search overlay panel (`Ctrl+Shift+F`), tooltips, keyboard navigation, and zero-match states. |
| **M5: Ecosystem Extensibility** | `v0.6.0-beta` | **Sync & Agent Abstractions** | **Completed** | Trait definitions for `VaultSyncEngine` (`gix`/`git2`), MCP-compatible `AgentVaultApi` (`read_note`, `write_note`, `search_notes`, `get_backlinks`), JSON schema tool definitions. |
| **M6: Quality & Governance** | `v0.7.0-beta` | **Quality Engineering & Verification** | **Active** | Hurff 5-State UI audit across all views, automated smoke & boundary test suite, Antigravity deterministic hooks, and 4-phase Release Checkpoint Matrix. |
| **v1.0: GA Release** | `v1.0.0` | **General Availability** | **Planned** | Packaging (Flatpak/COSMIC Store), settings persistence via `cosmic-config`, first stable user release. |

---

## 📄 License

Licensed under the **GNU General Public License v3.0 or later** ([GPL-3.0-or-later](LICENSE)).
