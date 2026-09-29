# Workspace Layout & Pane Grid Implementation Patterns

## 1. Pane Grid Model (`cosmic::widget::pane_grid`)

`cosmic::widget::pane_grid` provides resizable split-panes for side-by-side editing and previewing.

```rust
use cosmic::widget::pane_grid::{self, Pane, PaneGrid};
use cosmic::Element;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneKind {
    Editor,
    Preview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    EditorOnly,
    Split,
    PreviewOnly,
}

pub struct WorkspaceState {
    pub panes: pane_grid::State<PaneKind>,
    pub editor_pane: Pane,
    pub preview_pane: Option<Pane>,
    pub view_mode: ViewMode,
}

impl WorkspaceState {
    pub fn new() -> Self {
        let (mut panes, editor_pane) = pane_grid::State::new(PaneKind::Editor);
        Self {
            panes,
            editor_pane,
            preview_pane: None,
            view_mode: ViewMode::EditorOnly,
        }
    }

    pub fn set_view_mode(&mut self, mode: ViewMode) {
        self.view_mode = mode;
        match mode {
            ViewMode::EditorOnly => {
                if let Some(preview) = self.preview_pane.take() {
                    let _ = self.panes.close(preview);
                }
            }
            ViewMode::Split => {
                if self.preview_pane.is_none() {
                    if let Some((new_pane, _split)) = self.panes.split(
                        self.editor_pane,
                        pane_grid::Axis::Vertical,
                        PaneKind::Preview,
                    ) {
                        self.preview_pane = Some(new_pane);
                    }
                }
            }
            ViewMode::PreviewOnly => {
                // Adjust layout to show preview only
            }
        }
    }
}
```

---

## 2. Lazy Markdown Preview (120 FPS Frame-Rate Protection)

Wrapping the rendered Markdown view inside `iced::widget::lazy` ensures that character typing in the editor pane does NOT trigger expensive Markdown AST re-parsing or widget re-allocations.

```rust
use iced::widget::lazy;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LazyPreviewKey {
    pub note_id: String,
    pub content_hash: u64,
}

pub fn calculate_hash(content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

pub fn view_preview<'a, Message: 'a>(
    note_id: &str,
    markdown_content: &'a str,
) -> Element<'a, Message> {
    let key = LazyPreviewKey {
        note_id: note_id.to_string(),
        content_hash: calculate_hash(markdown_content),
    };

    lazy(key, move |_| {
        // This closure ONLY executes when the content_hash or note_id changes!
        render_markdown_ast(markdown_content)
    })
    .into()
}

fn render_markdown_ast<'a, Message: 'a>(content: &'a str) -> Element<'a, Message> {
    // Uses libcosmic's built-in markdown widget
    cosmic::widget::markdown(content).into()
}
```
