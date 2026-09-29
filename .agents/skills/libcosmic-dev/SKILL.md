---
name: libcosmic-dev
description: >-
  Expert guidance, architectural patterns, and widget references for building
  native Rust applications for System76's COSMIC Desktop Environment using libcosmic.
  Use this skill whenever creating, modifying, debugging, or designing applications
  built with libcosmic, cosmic-config, or the COSMIC app template.
---

# `libcosmic` Development Guide

`libcosmic` is System76's official GUI toolkit for the COSMIC Desktop Environment, written in Rust on top of the [Iced](https://iced.rs/) GUI library.

---

## 1. Core Architecture: Elm Architecture (Model-Update-View)

`libcosmic` applications implement the `cosmic::Application` trait:

```rust
use cosmic::iced::{Alignment, Length, Subscription, Task};
use cosmic::widget::{self, button, column, container, row, text};
use cosmic::{app::Core, Element};

pub struct AppModel {
    core: Core,
    // Application-specific state
}

#[derive(Clone, Debug)]
pub enum Message {
    // Application events
}

impl cosmic::Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = "com.system76.CosmicNotes";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<cosmic::Action<Self::Message>>) {
        let app = AppModel {
            core,
        };
        (app, Task::none())
    }

    fn header_start(&self) -> Vec<Element<'_, Self::Message>> {
        // Left side of header bar
        vec![]
    }

    fn header_end(&self) -> Vec<Element<'_, Self::Message>> {
        // Right side of header bar
        vec![]
    }

    fn view(&self) -> Element<'_, Self::Message> {
        // Declarative widget tree
        widget::text::title1("Hello COSMIC").into()
    }

    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            // Update state and return asynchronous Tasks
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        // Background async streams, timers, config watchers
        Subscription::none()
    }
}
```

---

## 2. Essential Widgets for Notes Applications

| Widget | Module / Constructor | Purpose |
|---|---|---|
| **Text Editor** | `cosmic::widget::text_editor` | Multi-line plain/code/markdown editor with cursor & syntax support |
| **Markdown** | `cosmic::widget::markdown` (requires `"markdown"` feature) | Native AST-based rendered markdown preview |
| **Pane Grid** | `cosmic::widget::pane_grid` | Resizable split panes (e.g. side-by-side Editor & Markdown Preview) |
| **Header Bar** | Built-in via `header_start()` / `header_end()` | Native titlebar with embedded controls, search inputs, view mode switchers |
| **Navigation Bar** | `cosmic::widget::nav_bar` / `fn nav_model(&self)` | Sidebar navigation for folders, categories, or note lists |
| **Context Drawer** | `cosmic::widget::context_drawer` | Slide-out inspector/drawer for note metadata, word count, tags, settings |
| **Segmented Button**| `cosmic::widget::segmented_button` | Reorderable tabs or view switchers |
| **Text Input** | `cosmic::widget::text_input` | Note title input, search box, tag entry |
| **Scrollable** | `cosmic::widget::scrollable` | Smooth scrolling container for note lists and content |
| **Dialog / Popover** | `cosmic::widget::dialog`, `cosmic::widget::popover` | Confirmation modals (delete note, rename folder, export) |
| **Toaster** | `cosmic::widget::toaster::Toasts` | Toast notifications (e.g., "Saved", "Copied to clipboard") |

---

## 3. Real-World Production Patterns from Native COSMIC Apps

Insights gathered from studying native COSMIC apps (`cosmic-edit`, `cedilla`, `cosmic-files`, `tasks`):

1. **Split-Screen Editor & Preview (`pane_grid`)**:
   - `pane_grid::State<PaneContent>` enables flexible, user-resizable split views (Editor + Markdown Preview) just like `cedilla`.
2. **Text Editing Options**:
   - High-level: `cosmic::widget::text_editor` for standard text editing.
   - Low-level: `cosmic_text` (`Buffer`, `Attrs`, `SyntaxEditor`) for fine-grained cursor, font, and syntax highlighting control (used in `cosmic-edit`).
3. **Safe File Operations**:
   - Use the `trash` crate when deleting notes, allowing users to restore notes from the system trash.
   - Use `notify` / `notify-debouncer-full` to react to file changes if users edit Markdown files externally.
4. **Dialogs & File Pickers**:
   - Use `cosmic_files::dialog::{Dialog, DialogKind, DialogMessage}` for native COSMIC file pickers.
5. **Data Storage & XDG Compliance**:
   - User notes: `dirs::data_dir()` / `~/.local/share/com.system76.CosmicNotes/`
   - Config: `cosmic-config` syncs preferences to `~/.config/cosmic/`
6. **Toast Feedback**:
   - Use `cosmic::widget::toaster::Toasts<Message>` to provide smooth desktop feedback when saving, copying, or exporting.

---

## 3. Styling & Theming

- **Spacing & Units**: Always use system spacing via `cosmic::theme::spacing()`:
  - `spacing().space_xxs`, `space_xs`, `space_s`, `space_m`, `space_l`, `space_xl`, `space_xxl`
- **Dynamic Theming**: `libcosmic` automatically handles COSMIC system light/dark switching and accent colors.
- **Cosmic Icons**: Use `cosmic::widget::icon::from_name(...)` to integrate with standard COSMIC system icons.

---

## 4. State Persistence & Settings

- Use **`cosmic-config`** to store preferences:
  ```rust
  #[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
  pub struct Config {
      pub font_size: u16,
      pub show_preview: bool,
  }
  ```
- Watch configuration changes reactively via `self.core().watch_config::<Config>(Self::APP_ID)`.

---

## 5. Build Requirements & System Dependencies (Linux)

Ensure the following system libraries are present for building with Wayland/wgpu:
- `libfontconfig-dev` / `fontconfig`
- `libfreetype-dev`
- `libxkbcommon-dev`
- `libwayland-dev` / Wayland protocols
- `pkgconf` / `pkg-config`
