use cosmic::app::{ContextDrawer, Core, Task};
use cosmic::iced::widget::scrollable::{self as iced_scrollable, RelativeOffset};
use cosmic::iced::Length;
use cosmic::widget::pane_grid;
use cosmic::widget::text_editor::{self as te, Action as EditorAction, Content as EditorContent};
use cosmic::widget::{self, button, container, scrollable, text, Column, Id as ScrollId, Row};
use cosmic::Element;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::core::{Note, Vault, VaultEvent};
use crate::search::VaultIndex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Editor,
    Split,
    Preview,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneKind {
    Editor,
    Preview,
}

pub fn calculate_content_hash(content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

#[derive(Clone, Debug)]
pub enum Message {
    SelectNote(PathBuf),
    CreateNewNote,
    CreateNoteWithTitle(String),
    SetViewMode(ViewMode),
    EditorAction(EditorAction),
    PaneResized(pane_grid::ResizeEvent),
    LinkClicked,
    SearchInputChanged(String),
    ToggleSearch,
    ToggleSidebar,
    ToggleContextDrawer,
    FilterByTag(Option<String>),
    CloseNoteTab(PathBuf),
    VaultFileEvent(VaultEvent),
    // M4: Search Experience & Modals
    OpenQuickSwitcher,
    CloseQuickSwitcher,
    QuickSwitcherInputChanged(String),
    QuickSwitcherSelect(PathBuf),
    OpenFullTextSearch,
    CloseFullTextSearch,
    FullTextSearchInputChanged(String),
    CloseModals,
    ModalNavigateDown,
    ModalNavigateUp,
    ModalSelectCurrent,
}

pub struct AppModel {
    core: Core,
    vault: Vault,
    vault_index: VaultIndex,
    notes: Vec<Note>,
    open_tabs: Vec<PathBuf>,
    selected_note_path: Option<PathBuf>,
    active_note: Option<Note>,
    editor_content: EditorContent,
    parsed_markdown: Vec<widget::markdown::Item>,
    content_hash: u64,
    panes: pane_grid::State<PaneKind>,
    view_mode: ViewMode,
    search_query: String,
    search_active: bool,
    show_sidebar: bool,
    show_context: bool,
    selected_tag: Option<String>,
    // M4: Search Experience & Modals
    show_quick_switcher: bool,
    quick_switcher_query: String,
    quick_switcher_results: Vec<crate::search::FuzzyMatch>,
    quick_switcher_selected: usize,
    show_fulltext_search: bool,
    fulltext_query: String,
    fulltext_results: Vec<crate::search::FullTextSearchResult>,
    fulltext_selected: usize,
}

impl AppModel {
    pub fn new(core: Core, vault_path: PathBuf) -> (Self, Task<Message>) {
        let vault = Vault::open(&vault_path).unwrap_or_else(|_| {
            let fallback = dirs::document_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("CosmicNotes");
            Vault::open(&fallback).expect("Failed to initialize fallback vault")
        });

        // Scan notes on startup
        let mut scanned = vault.scan_notes().unwrap_or_default();

        // If vault is completely empty, create a welcome note
        if scanned.is_empty() {
            let welcome_content = r#"---
title: Welcome to COSMIC Notes
tags:
  - cosmic
  - welcome
  - markdown
---

# Welcome to COSMIC Notes

COSMIC Notes is a native, high-performance, local-first note-taking app built specifically for the **COSMIC Desktop Environment**.

### Features:
- **Local-First & Obsidian Compatible**: Your notes are plain CommonMark `.md` files on your disk.
- **Ultra-Fast**: Sub-millisecond dual-tier search and reactive filesystem monitoring.
- **Write-Echo Cancellation**: Avoids file watcher loops when saving notes.
- **Knowledge Graph**: Bidirectional `[[wikilinks]]` and backlink discovery.

Click **New Note** above or start editing right here!
"#;
            if let Ok(welcome_note) = vault.write_note(Path::new("Welcome.md"), welcome_content) {
                scanned.push(welcome_note);
            }
        }

        let index_dir = vault.root_path().join(".cosmic-notes").join("index");
        let vault_index = VaultIndex::open_or_create(&index_dir, &scanned).unwrap_or_else(|_| {
            VaultIndex::create_in_ram(&scanned).expect("Failed to initialize in-ram index")
        });

        let selected = scanned.first().map(|n| n.path.clone());
        let active = scanned.first().cloned();
        let editor_content = active
            .as_ref()
            .map(|n| EditorContent::with_text(&n.raw_content))
            .unwrap_or_else(EditorContent::new);

        let content_hash = active
            .as_ref()
            .map(|n| calculate_content_hash(&n.raw_content))
            .unwrap_or(0);

        let parsed_markdown = active
            .as_ref()
            .map(|n| widget::markdown::parse(&n.body).collect())
            .unwrap_or_default();

        let open_tabs = selected.iter().cloned().collect();

        // Initialize 50/50 vertical pane grid for Split mode
        let (mut panes, editor_pane) = pane_grid::State::new(PaneKind::Editor);
        let _ = panes.split(pane_grid::Axis::Vertical, editor_pane, PaneKind::Preview);

        let app = Self {
            core,
            vault,
            vault_index,
            notes: scanned,
            open_tabs,
            selected_note_path: selected,
            active_note: active,
            editor_content,
            parsed_markdown,
            content_hash,
            panes,
            view_mode: ViewMode::Split,
            search_query: String::new(),
            search_active: false,
            show_sidebar: true,
            show_context: false,
            selected_tag: None,
            show_quick_switcher: false,
            quick_switcher_query: String::new(),
            quick_switcher_results: Vec::new(),
            quick_switcher_selected: 0,
            show_fulltext_search: false,
            fulltext_query: String::new(),
            fulltext_results: Vec::new(),
            fulltext_selected: 0,
        };

        (app, Task::none())
    }

    pub fn select_note(&mut self, path: PathBuf) {
        if !self.open_tabs.contains(&path) {
            self.open_tabs.push(path.clone());
        }
        if let Ok(note) = self.vault.read_note(&path) {
            self.editor_content = EditorContent::with_text(&note.raw_content);
            self.content_hash = calculate_content_hash(&note.raw_content);
            self.parsed_markdown = widget::markdown::parse(&note.body).collect();
            self.active_note = Some(note);
            self.selected_note_path = Some(path);
        }
    }

    pub fn create_note_with_title(&mut self, title: String) {
        let clean_title = title.trim();
        if clean_title.is_empty() {
            return;
        }
        let safe_filename = format!("{}.md", clean_title.replace('/', "_").replace('\\', "_"));
        let initial_content = format!("# {}\n\nStart typing your note here...", clean_title);
        if let Ok(note) = self.vault.write_note(Path::new(&safe_filename), &initial_content) {
            let _ = self.vault_index.update_note(&note);
            self.select_note(note.path.clone());
            self.notes.push(note);
        }
    }

    pub fn view_quick_switcher(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();

        let search_input = widget::search_input("Jump to note by title, path, or #tag... (Esc to close)", &self.quick_switcher_query)
            .on_input(Message::QuickSwitcherInputChanged)
            .width(Length::Fill);

        let mut results_col = Column::new().spacing(spacing.space_xxs);

        if self.quick_switcher_results.is_empty() {
            if self.quick_switcher_query.trim().is_empty() {
                let empty_box = Column::new()
                    .spacing(spacing.space_xs)
                    .padding([spacing.space_m, spacing.space_s])
                    .align_x(cosmic::iced::Alignment::Center)
                    .push(widget::icon::from_name("edit-find-symbolic").size(24))
                    .push(text::body("Type to find notes across your vault"))
                    .push(text::caption("Navigate with ↑↓ • Select with Enter • Cancel with Esc"));
                results_col = results_col.push(empty_box);
            } else {
                let not_found_box = Column::new()
                    .spacing(spacing.space_s)
                    .padding([spacing.space_m, spacing.space_s])
                    .align_x(cosmic::iced::Alignment::Center)
                    .push(widget::icon::from_name("system-search-symbolic").size(24))
                    .push(text::body(format!("No notes matching \"{}\"", self.quick_switcher_query)))
                    .push(
                        button::text(format!("Create note \"{}.md\"", self.quick_switcher_query.trim()))
                            .class(cosmic::theme::Button::Suggested)
                            .on_press(Message::CreateNoteWithTitle(self.quick_switcher_query.trim().to_string())),
                    );
                results_col = results_col.push(not_found_box);
            }
        } else {
            for (idx, m) in self.quick_switcher_results.iter().enumerate() {
                let is_highlighted = idx == self.quick_switcher_selected;
                let path = m.item.path.clone();

                let doc_icon = widget::icon::from_name("text-x-generic-symbolic").size(16);
                let title_text = text::body(if m.item.title.is_empty() {
                    "Untitled".to_string()
                } else {
                    m.item.title.clone()
                });

                let path_text = text::caption(m.item.path.to_string_lossy().to_string());

                let mut row = Row::new()
                    .spacing(spacing.space_xs)
                    .align_y(cosmic::iced::Alignment::Center)
                    .push(doc_icon)
                    .push(Column::new().push(title_text).push(path_text).width(Length::Fill));

                if !m.item.tags.is_empty() {
                    let tags_preview = m.item.tags.iter().take(2).map(|t| format!("#{t}")).collect::<Vec<_>>().join(" ");
                    row = row.push(text::caption(tags_preview));
                }

                let mut btn = button::custom(row)
                    .width(Length::Fill)
                    .padding([spacing.space_xs, spacing.space_s])
                    .on_press(Message::QuickSwitcherSelect(path));

                if is_highlighted {
                    btn = btn.class(cosmic::theme::Button::Suggested);
                } else {
                    btn = btn.class(cosmic::theme::Button::Text);
                }

                results_col = results_col.push(btn);
            }
        }

        let card_content = Column::new()
            .spacing(spacing.space_m)
            .push(
                Row::new()
                    .spacing(spacing.space_s)
                    .align_y(cosmic::iced::Alignment::Center)
                    .push(widget::icon::from_name("edit-find-symbolic").size(18))
                    .push(text::title3("Quick Switcher (Ctrl+P)").width(Length::Fill))
                    .push(
                        button::icon(widget::icon::from_name("window-close-symbolic").size(16))
                            .class(cosmic::theme::Button::Text)
                            .on_press(Message::CloseModals),
                    ),
            )
            .push(search_input)
            .push(scrollable(results_col).height(Length::Fixed(320.0)));

        let card = container(card_content)
            .class(cosmic::theme::Container::Card)
            .width(Length::Fixed(560.0))
            .padding(spacing.space_m);

        container(card)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }

    pub fn view_fulltext_search(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();

        let search_input = widget::search_input("Search entire vault contents (BM25)...", &self.fulltext_query)
            .on_input(Message::FullTextSearchInputChanged)
            .width(Length::Fill);

        let mut results_col = Column::new().spacing(spacing.space_xs);

        if self.fulltext_results.is_empty() {
            if self.fulltext_query.trim().is_empty() {
                let empty_box = Column::new()
                    .spacing(spacing.space_xs)
                    .padding([spacing.space_m, spacing.space_s])
                    .align_x(cosmic::iced::Alignment::Center)
                    .push(widget::icon::from_name("folder-saved-search-symbolic").size(28))
                    .push(text::body("Deep Full-Text Vault Search"))
                    .push(text::caption("Search keywords, sentences, code blocks, and tags across every document."));
                results_col = results_col.push(empty_box);
            } else {
                let not_found_box = Column::new()
                    .spacing(spacing.space_s)
                    .padding([spacing.space_m, spacing.space_s])
                    .align_x(cosmic::iced::Alignment::Center)
                    .push(widget::icon::from_name("system-search-symbolic").size(24))
                    .push(text::body(format!("No notes contain \"{}\"", self.fulltext_query)))
                    .push(text::caption("Try different keywords or broader search terms."));
                results_col = results_col.push(not_found_box);
            }
        } else {
            for (idx, r) in self.fulltext_results.iter().enumerate() {
                let is_highlighted = idx == self.fulltext_selected;
                let path = r.path.clone();

                let title_row = Row::new()
                    .spacing(spacing.space_xs)
                    .align_y(cosmic::iced::Alignment::Center)
                    .push(widget::icon::from_name("text-x-generic-symbolic").size(16))
                    .push(text::body(&r.title).width(Length::Fill))
                    .push(text::caption(r.path.to_string_lossy().to_string()));

                let mut card_col = Column::new().spacing(spacing.space_xxs).push(title_row);

                if let Some(ref snippet) = r.snippet {
                    let cleaned = strip_html_tags(snippet);
                    card_col = card_col.push(text::caption(cleaned));
                }

                let mut btn = button::custom(card_col)
                    .width(Length::Fill)
                    .padding([spacing.space_xs, spacing.space_s])
                    .on_press(Message::SelectNote(path));

                if is_highlighted {
                    btn = btn.class(cosmic::theme::Button::Suggested);
                } else {
                    btn = btn.class(cosmic::theme::Button::Text);
                }

                results_col = results_col.push(btn);
            }
        }

        let card_content = Column::new()
            .spacing(spacing.space_m)
            .push(
                Row::new()
                    .spacing(spacing.space_s)
                    .align_y(cosmic::iced::Alignment::Center)
                    .push(widget::icon::from_name("folder-saved-search-symbolic").size(18))
                    .push(text::title3("Full-Text Search (Ctrl+Shift+F)").width(Length::Fill))
                    .push(
                        button::icon(widget::icon::from_name("window-close-symbolic").size(16))
                            .class(cosmic::theme::Button::Text)
                            .on_press(Message::CloseModals),
                    ),
            )
            .push(search_input)
            .push(scrollable(results_col).height(Length::Fixed(360.0)));

        let card = container(card_content)
            .class(cosmic::theme::Container::Card)
            .width(Length::Fixed(620.0))
            .padding(spacing.space_m);

        container(card)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }
}

fn strip_html_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut inside = false;
    for c in s.chars() {
        if c == '<' {
            inside = true;
        } else if c == '>' {
            inside = false;
        } else if !inside {
            out.push(c);
        }
    }
    out
}

impl cosmic::Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = PathBuf;
    type Message = Message;
    const APP_ID: &'static str = "com.system76.CosmicNotes";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, flags: Self::Flags) -> (Self, Task<Self::Message>) {
        Self::new(core, flags)
    }

    fn header_start(&self) -> Vec<Element<'_, Self::Message>> {
        let sidebar_toggle = button::icon(widget::icon::from_name("sidebar-places-symbolic").size(18))
            .class(if self.show_sidebar {
                cosmic::theme::Button::Text
            } else {
                cosmic::theme::Button::Suggested
            })
            .on_press(Message::ToggleSidebar);
        let sidebar_tip = widget::tooltip(
            sidebar_toggle,
            "Toggle Explorer Sidebar (Ctrl+\\)",
            widget::tooltip::Position::Bottom,
        );

        let menu_btn = |label: &'static str, tip: &'static str, msg: Message, is_active: bool| {
            let b = button::text(label)
                .class(if is_active {
                    cosmic::theme::Button::Suggested
                } else {
                    cosmic::theme::Button::Text
                })
                .on_press(msg);
            widget::tooltip(b, tip, widget::tooltip::Position::Bottom).into()
        };

        vec![
            sidebar_tip.into(),
            menu_btn("New Note", "Create New Note (Ctrl+N)", Message::CreateNewNote, false),
            menu_btn("Edit", "Editor Mode (Ctrl+1)", Message::SetViewMode(ViewMode::Editor), self.view_mode == ViewMode::Editor),
            menu_btn("Split", "Split Mode (Ctrl+2)", Message::SetViewMode(ViewMode::Split), self.view_mode == ViewMode::Split),
            menu_btn("Preview", "Preview Mode (Ctrl+3)", Message::SetViewMode(ViewMode::Preview), self.view_mode == ViewMode::Preview),
        ]
    }

    fn header_end(&self) -> Vec<Element<'_, Self::Message>> {
        let mut items: Vec<Element<'_, Self::Message>> = Vec::new();

        if self.search_active {
            let search_input = widget::search_input("Search notes...", &self.search_query)
                .on_input(Message::SearchInputChanged)
                .width(Length::Fixed(240.0));
            items.push(search_input.into());
        }

        let search_icon_name = if self.search_active {
            "edit-clear-symbolic"
        } else {
            "system-search-symbolic"
        };

        let search_btn = button::icon(widget::icon::from_name(search_icon_name).size(18))
            .class(if self.search_active {
                cosmic::theme::Button::Suggested
            } else {
                cosmic::theme::Button::Text
            })
            .on_press(Message::ToggleSearch);
        let search_tip = widget::tooltip(
            search_btn,
            "Filter Notes List (Ctrl+F)",
            widget::tooltip::Position::Bottom,
        );
        items.push(search_tip.into());

        // Quick Switcher Button
        let qs_btn = button::icon(widget::icon::from_name("edit-find-symbolic").size(18))
            .class(if self.show_quick_switcher {
                cosmic::theme::Button::Suggested
            } else {
                cosmic::theme::Button::Text
            })
            .on_press(Message::OpenQuickSwitcher);
        let qs_tip = widget::tooltip(
            qs_btn,
            "Quick Switcher (Ctrl+P)",
            widget::tooltip::Position::Bottom,
        );
        items.push(qs_tip.into());

        // Deep Full-Text Search Button
        let ft_btn = button::icon(widget::icon::from_name("folder-saved-search-symbolic").size(18))
            .class(if self.show_fulltext_search {
                cosmic::theme::Button::Suggested
            } else {
                cosmic::theme::Button::Text
            })
            .on_press(Message::OpenFullTextSearch);
        let ft_tip = widget::tooltip(
            ft_btn,
            "Full-Text Deep Search (Ctrl+Shift+F)",
            widget::tooltip::Position::Bottom,
        );
        items.push(ft_tip.into());

        let info_btn = button::icon(widget::icon::from_name("dialog-information-symbolic").size(18))
            .class(if self.show_context {
                cosmic::theme::Button::Suggested
            } else {
                cosmic::theme::Button::Text
            })
            .on_press(Message::ToggleContextDrawer);
        let info_tip = widget::tooltip(
            info_btn,
            "Note Details & Backlinks (Ctrl+I)",
            widget::tooltip::Position::Bottom,
        );
        items.push(info_tip.into());

        items
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::SelectNote(path) => {
                self.select_note(path);
            }
            Message::CreateNewNote => {
                let note_count = self.notes.len() + 1;
                let filename = format!("Untitled_{note_count}.md");
                let initial_content = format!("# Untitled {}\n\nStart typing your note here...", note_count);
                if let Ok(note) = self.vault.write_note(Path::new(&filename), &initial_content) {
                    let _ = self.vault_index.update_note(&note);
                    self.select_note(note.path.clone());
                    self.notes.push(note);
                }
            }
            Message::CreateNoteWithTitle(title) => {
                self.create_note_with_title(title);
                self.show_quick_switcher = false;
                self.show_fulltext_search = false;
            }
            Message::EditorAction(action) => {
                let is_edit = action.is_edit();
                self.editor_content.perform(action);

                if is_edit {
                    let current_text = self.editor_content.text();
                    let new_hash = calculate_content_hash(&current_text);
                    if new_hash != self.content_hash {
                        self.content_hash = new_hash;
                        self.parsed_markdown = widget::markdown::parse(&current_text).collect();
                    }

                    // Auto-save to vault with write-echo cancellation
                    if let Some(ref path) = self.selected_note_path {
                        if let Ok(updated_note) = self.vault.write_note(path, &current_text) {
                            let _ = self.vault_index.update_note(&updated_note);
                            if let Some(n) = self.notes.iter_mut().find(|n| n.path == *path) {
                                *n = updated_note.clone();
                            }
                            self.active_note = Some(updated_note);
                        }
                    }
                }

                if self.view_mode == ViewMode::Split {
                    let cursor = self.editor_content.cursor();
                    let line = cursor.position.line;
                    let total_lines = self.editor_content.line_count().max(1);
                    let progress = (line as f32 / total_lines as f32).clamp(0.0, 1.0);
                    return iced_scrollable::snap_to(
                        ScrollId::new("preview-scroll"),
                        RelativeOffset { x: None, y: Some(progress) },
                    );
                }
            }
            Message::PaneResized(event) => {
                self.panes.resize(event.split, event.ratio);
            }
            Message::SetViewMode(mode) => {
                self.view_mode = mode;
            }
            Message::LinkClicked => {}
            Message::SearchInputChanged(query) => {
                self.search_query = query;
            }
            Message::ToggleSearch => {
                self.search_active = !self.search_active;
                if !self.search_active {
                    self.search_query.clear();
                }
            }
            Message::ToggleSidebar => {
                self.show_sidebar = !self.show_sidebar;
            }
            Message::ToggleContextDrawer => {
                self.show_context = !self.show_context;
                self.core.set_show_context(self.show_context);
            }
            Message::FilterByTag(tag) => {
                self.selected_tag = tag;
            }
            Message::CloseNoteTab(path) => {
                self.open_tabs.retain(|p| p != &path);
                if self.selected_note_path.as_ref() == Some(&path) {
                    let next = self.open_tabs.last().cloned();
                    if let Some(next_path) = next {
                        self.select_note(next_path);
                    } else {
                        self.active_note = None;
                        self.selected_note_path = None;
                        self.editor_content = EditorContent::new();
                        self.content_hash = 0;
                        self.parsed_markdown.clear();
                    }
                }
            }
            Message::VaultFileEvent(event) => {
                let _ = self.vault_index.handle_vault_event(&event, &self.vault);
                if let Ok(scanned) = self.vault.scan_notes() {
                    self.notes = scanned;
                }
            }
            Message::OpenQuickSwitcher => {
                self.show_quick_switcher = true;
                self.show_fulltext_search = false;
                self.quick_switcher_query.clear();
                self.quick_switcher_results = self.vault_index.quick_search("", 15);
                self.quick_switcher_selected = 0;
            }
            Message::CloseQuickSwitcher => {
                self.show_quick_switcher = false;
            }
            Message::QuickSwitcherInputChanged(query) => {
                self.quick_switcher_query = query;
                self.quick_switcher_results = self.vault_index.quick_search(&self.quick_switcher_query, 15);
                self.quick_switcher_selected = 0;
            }
            Message::QuickSwitcherSelect(path) => {
                self.show_quick_switcher = false;
                self.select_note(path);
            }
            Message::OpenFullTextSearch => {
                self.show_fulltext_search = true;
                self.show_quick_switcher = false;
                self.fulltext_query.clear();
                self.fulltext_results.clear();
                self.fulltext_selected = 0;
            }
            Message::CloseFullTextSearch => {
                self.show_fulltext_search = false;
            }
            Message::FullTextSearchInputChanged(query) => {
                self.fulltext_query = query;
                self.fulltext_results = self.vault_index.fulltext_search(&self.fulltext_query, 20).unwrap_or_default();
                self.fulltext_selected = 0;
            }
            Message::CloseModals => {
                if self.show_quick_switcher || self.show_fulltext_search {
                    self.show_quick_switcher = false;
                    self.show_fulltext_search = false;
                } else if self.search_active {
                    self.search_active = false;
                    self.search_query.clear();
                } else if self.show_context {
                    self.show_context = false;
                    self.core.set_show_context(false);
                }
            }
            Message::ModalNavigateDown => {
                if self.show_quick_switcher && !self.quick_switcher_results.is_empty() {
                    self.quick_switcher_selected = (self.quick_switcher_selected + 1)
                        .min(self.quick_switcher_results.len().saturating_sub(1));
                } else if self.show_fulltext_search && !self.fulltext_results.is_empty() {
                    self.fulltext_selected = (self.fulltext_selected + 1)
                        .min(self.fulltext_results.len().saturating_sub(1));
                }
            }
            Message::ModalNavigateUp => {
                if self.show_quick_switcher {
                    self.quick_switcher_selected = self.quick_switcher_selected.saturating_sub(1);
                } else if self.show_fulltext_search {
                    self.fulltext_selected = self.fulltext_selected.saturating_sub(1);
                }
            }
            Message::ModalSelectCurrent => {
                if self.show_quick_switcher {
                    if let Some(m) = self.quick_switcher_results.get(self.quick_switcher_selected) {
                        let path = m.item.path.clone();
                        self.show_quick_switcher = false;
                        self.select_note(path);
                    } else if !self.quick_switcher_query.trim().is_empty() {
                        let title = self.quick_switcher_query.trim().to_string();
                        self.create_note_with_title(title);
                        self.show_quick_switcher = false;
                    }
                } else if self.show_fulltext_search {
                    if let Some(r) = self.fulltext_results.get(self.fulltext_selected) {
                        let path = r.path.clone();
                        self.show_fulltext_search = false;
                        self.select_note(path);
                    }
                }
            }
        }
        Task::none()
    }

    fn on_search(&mut self) -> Task<Self::Message> {
        self.search_active = !self.search_active;
        if !self.search_active {
            self.search_query.clear();
        }
        Task::none()
    }

    fn on_escape(&mut self) -> Task<Self::Message> {
        if self.search_active {
            self.search_active = false;
            self.search_query.clear();
        } else if self.show_context {
            self.show_context = false;
            self.core.set_show_context(false);
        }
        Task::none()
    }

    fn dialog(&self) -> Option<Element<'_, Self::Message>> {
        if self.show_quick_switcher {
            Some(self.view_quick_switcher())
        } else if self.show_fulltext_search {
            Some(self.view_fulltext_search())
        } else {
            None
        }
    }

    fn subscription(&self) -> cosmic::iced::Subscription<Self::Message> {
        use cosmic::iced::keyboard::{self, Key, key::Named};

        let global_sub = keyboard::listen().filter_map(|event| {
            if let keyboard::Event::KeyPressed { key, modifiers, .. } = event {
                if modifiers.control() {
                    match key.as_ref() {
                        Key::Character("p") | Key::Character("P") => Some(Message::OpenQuickSwitcher),
                        Key::Character("f") | Key::Character("F") if modifiers.shift() => Some(Message::OpenFullTextSearch),
                        Key::Character("f") | Key::Character("F") => Some(Message::ToggleSearch),
                        Key::Character("n") | Key::Character("N") => Some(Message::CreateNewNote),
                        Key::Character("1") => Some(Message::SetViewMode(ViewMode::Editor)),
                        Key::Character("2") => Some(Message::SetViewMode(ViewMode::Split)),
                        Key::Character("3") => Some(Message::SetViewMode(ViewMode::Preview)),
                        Key::Character("\\") => Some(Message::ToggleSidebar),
                        Key::Character("i") | Key::Character("I") => Some(Message::ToggleContextDrawer),
                        _ => None,
                    }
                } else if matches!(key.as_ref(), Key::Named(Named::Escape)) {
                    Some(Message::CloseModals)
                } else {
                    None
                }
            } else {
                None
            }
        });

        if self.show_quick_switcher || self.show_fulltext_search {
            let modal_sub = keyboard::listen().filter_map(|event| {
                if let keyboard::Event::KeyPressed { key, modifiers, .. } = event {
                    if !modifiers.control() && !modifiers.alt() {
                        match key.as_ref() {
                            Key::Named(Named::ArrowDown) => Some(Message::ModalNavigateDown),
                            Key::Named(Named::ArrowUp) => Some(Message::ModalNavigateUp),
                            Key::Named(Named::Enter) => Some(Message::ModalSelectCurrent),
                            _ => None,
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            });
            cosmic::iced::Subscription::batch(vec![global_sub, modal_sub])
        } else {
            global_sub
        }
    }

    fn context_drawer(&self) -> Option<ContextDrawer<'_, Self::Message>> {
        None
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let spacing = cosmic::theme::spacing();

        // 1. Left Sidebar: Notes & Tags Explorer (260px)
        let mut all_tags: Vec<String> = self.notes.iter()
            .flat_map(|n| n.tags.iter().cloned())
            .collect();
        all_tags.sort();
        all_tags.dedup();

        let mut sidebar_col = Column::new().spacing(spacing.space_s);

        // All Notes item (clean, no artificial "Library" header)
        let all_notes_icon = widget::icon::from_name("folder-symbolic").size(16);
        let all_notes_row = Row::new()
            .spacing(spacing.space_xs)
            .align_y(cosmic::iced::Alignment::Center)
            .push(all_notes_icon)
            .push(text::body("All Notes").width(Length::Fill))
            .push(text::caption(self.notes.len().to_string()));

        let mut all_notes_btn = button::custom(all_notes_row)
            .width(Length::Fill)
            .padding([spacing.space_xs, spacing.space_s])
            .on_press(Message::FilterByTag(None));

        if self.selected_tag.is_none() && self.search_query.trim().is_empty() {
            all_notes_btn = all_notes_btn.class(cosmic::theme::Button::Suggested);
        } else {
            all_notes_btn = all_notes_btn.class(cosmic::theme::Button::Text);
        }
        sidebar_col = sidebar_col.push(all_notes_btn);

        // Tags Section (if any exist)
        if !all_tags.is_empty() {
            let mut tags_group = Column::new().spacing(spacing.space_xxs).push(text::caption("TAGS"));

            for tag in all_tags {
                let is_tag_selected = self.selected_tag.as_deref() == Some(&tag);
                let tag_row = Row::new()
                    .spacing(spacing.space_xs)
                    .align_y(cosmic::iced::Alignment::Center)
                    .push(widget::icon::from_name("tag-symbolic").size(14))
                    .push(text::body(format!("#{tag}")).width(Length::Fill));

                let mut tag_btn = button::custom(tag_row)
                    .width(Length::Fill)
                    .padding([spacing.space_xs, spacing.space_s])
                    .on_press(Message::FilterByTag(if is_tag_selected {
                        None
                    } else {
                        Some(tag.clone())
                    }));

                if is_tag_selected {
                    tag_btn = tag_btn.class(cosmic::theme::Button::Suggested);
                } else {
                    tag_btn = tag_btn.class(cosmic::theme::Button::Text);
                }
                tags_group = tags_group.push(tag_btn);
            }

            sidebar_col = sidebar_col
                .push(widget::divider::horizontal::default())
                .push(tags_group);
        }

        sidebar_col = sidebar_col.push(widget::divider::horizontal::default());

        // Notes List Section
        let notes_header = if !self.search_query.trim().is_empty() {
            text::caption("SEARCH RESULTS")
        } else if let Some(ref tag) = self.selected_tag {
            text::caption(format!("TAG: #{}", tag.to_uppercase()))
        } else {
            text::caption("NOTES")
        };
        sidebar_col = sidebar_col.push(notes_header);

        let render_note_item = |note: &Note, is_selected: bool| -> Element<'static, Message> {
            let title = if note.title.is_empty() {
                "Untitled".to_string()
            } else {
                note.title.clone()
            };

            let doc_icon = widget::icon::from_name("text-x-generic-symbolic").size(16);

            let row_content = Row::new()
                .spacing(spacing.space_xs)
                .align_y(cosmic::iced::Alignment::Center)
                .push(doc_icon)
                .push(text::body(title).width(Length::Fill))
                .push(text::caption(note.formatted_date()));

            let mut item_btn = button::custom(row_content)
                .width(Length::Fill)
                .padding([spacing.space_xs, spacing.space_s])
                .on_press(Message::SelectNote(note.path.clone()));

            if is_selected {
                item_btn = item_btn.class(cosmic::theme::Button::Suggested);
            } else {
                item_btn = item_btn.class(cosmic::theme::Button::Text);
            }

            item_btn.into()
        };

        let mut note_items = Column::new().spacing(spacing.space_xxs);
        let mut items_rendered = 0;

        if !self.search_query.trim().is_empty() {
            let matches_index = self.vault_index.quick_search(&self.search_query, 50);
            for m in matches_index {
                let is_selected = self.selected_note_path.as_ref() == Some(&m.item.path);
                if let Some(note) = self.notes.iter().find(|n| n.path == m.item.path) {
                    note_items = note_items.push(render_note_item(note, is_selected));
                    items_rendered += 1;
                }
            }
        } else if let Some(ref tag) = self.selected_tag {
            for note in &self.notes {
                if note.tags.contains(tag) {
                    let is_selected = self.selected_note_path.as_ref() == Some(&note.path);
                    note_items = note_items.push(render_note_item(note, is_selected));
                    items_rendered += 1;
                }
            }
        } else {
            for note in &self.notes {
                let is_selected = self.selected_note_path.as_ref() == Some(&note.path);
                note_items = note_items.push(render_note_item(note, is_selected));
                items_rendered += 1;
            }
        }

        if items_rendered == 0 {
            if !self.search_query.trim().is_empty() {
                let empty_col = Column::new()
                    .spacing(spacing.space_xs)
                    .align_x(cosmic::iced::Alignment::Center)
                    .padding(spacing.space_m)
                    .push(widget::icon::from_name("system-search-symbolic").size(24))
                    .push(text::caption(format!("No notes match \"{}\"", self.search_query)))
                    .push(
                        button::text("Clear Filter")
                            .class(cosmic::theme::Button::Text)
                            .on_press(Message::ToggleSearch),
                    );
                note_items = note_items.push(empty_col);
            } else if let Some(ref tag) = self.selected_tag {
                let empty_col = Column::new()
                    .spacing(spacing.space_xs)
                    .align_x(cosmic::iced::Alignment::Center)
                    .padding(spacing.space_m)
                    .push(widget::icon::from_name("tag-symbolic").size(24))
                    .push(text::caption(format!("No notes tagged #{tag}")))
                    .push(
                        button::text("Show All Notes")
                            .class(cosmic::theme::Button::Text)
                            .on_press(Message::FilterByTag(None)),
                    );
                note_items = note_items.push(empty_col);
            } else {
                let empty_col = Column::new()
                    .spacing(spacing.space_xs)
                    .align_x(cosmic::iced::Alignment::Center)
                    .padding(spacing.space_m)
                    .push(widget::icon::from_name("document-new-symbolic").size(24))
                    .push(text::caption("No notes yet"))
                    .push(
                        button::text("Create Note")
                            .class(cosmic::theme::Button::Suggested)
                            .on_press(Message::CreateNewNote),
                    );
                note_items = note_items.push(empty_col);
            }
        }

        let sidebar = container(
            sidebar_col.push(scrollable(note_items).width(Length::Fill).height(Length::Fill))
        )
        .class(cosmic::theme::Container::Background)
        .width(Length::Fixed(260.0))
        .padding(spacing.space_s);

        // 2. Center Workspace: Pure distraction-free writing surface
        let center_content: Element<'_, Self::Message> = if let Some(ref _note) = self.active_note {
            let editor_widget = te::text_editor(&self.editor_content)
                .placeholder("Type your markdown note here...")
                .on_action(Message::EditorAction)
                .style(|theme, _| {
                    let cosmic = theme.cosmic();
                    te::Style {
                        background: cosmic::iced::Color::TRANSPARENT.into(),
                        border: cosmic::iced::Border::default(),
                        placeholder: cosmic.palette.neutral_7.into(),
                        value: cosmic.palette.neutral_9.into(),
                        selection: cosmic.accent.base.into(),
                    }
                })
                .height(Length::Fill);

            let style = widget::markdown::Style::from_palette(cosmic::iced::theme::Palette::DARK);
            let md_settings = widget::markdown::Settings::with_style(style);
            let md_view = widget::markdown::view(&self.parsed_markdown, md_settings)
                .map(|_| Message::LinkClicked);

            let preview_view = scrollable(
                Column::new()
                    .spacing(spacing.space_m)
                    .push(md_view)
            )
            .id(ScrollId::new("preview-scroll"))
            .height(Length::Fill)
            .width(Length::Fill);

            let body_surface: Element<'_, Self::Message> = match self.view_mode {
                ViewMode::Editor => editor_widget.into(),
                ViewMode::Preview => preview_view.into(),
                ViewMode::Split => {
                    let grid = cosmic::widget::pane_grid(&self.panes, |_pane, kind, _maximized| {
                        let content: Element<'_, Self::Message> = match *kind {
                            PaneKind::Editor => {
                                let ed = te::text_editor(&self.editor_content)
                                    .placeholder("Type your markdown note here...")
                                    .on_action(Message::EditorAction)
                                    .style(|theme, _| {
                                        let cosmic = theme.cosmic();
                                        te::Style {
                                            background: cosmic::iced::Color::TRANSPARENT.into(),
                                            border: cosmic::iced::Border::default(),
                                            placeholder: cosmic.palette.neutral_7.into(),
                                            value: cosmic.palette.neutral_9.into(),
                                            selection: cosmic.accent.base.into(),
                                        }
                                    })
                                    .height(Length::Fill);
                                container(ed)
                                    .width(Length::Fill)
                                    .height(Length::Fill)
                                    .into()
                            }
                            PaneKind::Preview => {
                                let style = widget::markdown::Style::from_palette(cosmic::iced::theme::Palette::DARK);
                                let md_settings = widget::markdown::Settings::with_style(style);
                                let md_view = widget::markdown::view(&self.parsed_markdown, md_settings)
                                    .map(|_| Message::LinkClicked);

                                let prev = scrollable(
                                    Column::new()
                                        .spacing(spacing.space_m)
                                        .push(md_view)
                                )
                                .id(ScrollId::new("preview-scroll"))
                                .height(Length::Fill)
                                .width(Length::Fill);

                                container(prev)
                                    .width(Length::Fill)
                                    .height(Length::Fill)
                                    .into()
                            }
                        };
                        pane_grid::Content::new(content)
                    })
                    .spacing(spacing.space_m)
                    .on_resize(10, Message::PaneResized)
                    .width(Length::Fill)
                    .height(Length::Fill);

                    grid.into()
                }
            };

            let workspace_col = Column::new()
                .spacing(spacing.space_s)
                .push(body_surface);

            container(workspace_col)
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(spacing.space_m)
                .into()
        } else {
            container(
                Column::new()
                    .spacing(spacing.space_m)
                    .push(text::title3("No note selected"))
                    .push(text::caption("Select a note from the sidebar or create a new one."))
                    .push(
                        button::text("New Note")
                            .class(cosmic::theme::Button::Suggested)
                            .on_press(Message::CreateNewNote)
                    )
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
        };

        // 3. Right Inspector Column: Symmetrical counterpart to left sidebar (260px)
        let inspector_panel = if let Some(ref note) = self.active_note {
            let mut inspector_col = Column::new().spacing(spacing.space_s);

            // Properties Section
            let telemetry_col = Column::new()
                .spacing(spacing.space_xxs)
                .push(text::caption("PROPERTIES"))
                .push(
                    Row::new()
                        .spacing(spacing.space_s)
                        .push(text::caption("Words:"))
                        .push(text::body(note.word_count().to_string()).width(Length::Fill)),
                )
                .push(
                    Row::new()
                        .spacing(spacing.space_s)
                        .push(text::caption("Characters:"))
                        .push(text::body(note.char_count().to_string()).width(Length::Fill)),
                )
                .push(
                    Row::new()
                        .spacing(spacing.space_s)
                        .push(text::caption("Reading Time:"))
                        .push(text::body(format!("~{} min", note.reading_time_mins())).width(Length::Fill)),
                )
                .push(
                    Row::new()
                        .spacing(spacing.space_s)
                        .push(text::caption("Modified:"))
                        .push(text::body(note.formatted_date()).width(Length::Fill)),
                )
                .push(
                    Row::new()
                        .spacing(spacing.space_s)
                        .push(text::caption("Path:"))
                        .push(text::caption(note.path.display().to_string()).width(Length::Fill)),
                );
            inspector_col = inspector_col.push(telemetry_col);

            // Tags Section
            if !note.tags.is_empty() {
                let mut tags_group = Column::new().spacing(spacing.space_xxs).push(text::caption("TAGS"));
                for tag in &note.tags {
                    let tag_btn = button::text(format!("#{tag}"))
                        .class(cosmic::theme::Button::Text)
                        .padding([spacing.space_xs, spacing.space_s])
                        .on_press(Message::FilterByTag(Some(tag.clone())));
                    tags_group = tags_group.push(tag_btn);
                }
                inspector_col = inspector_col
                    .push(widget::divider::horizontal::default())
                    .push(tags_group);
            }

            // Connections Section
            let backlinks = self.vault_index.backlinks(&note.path);
            let outgoing = self.vault_index.outgoing_links(&note.path);

            let mut graph_col = Column::new().spacing(spacing.space_xs).push(text::caption("CONNECTIONS"));

            graph_col = graph_col.push(text::caption(format!("Backlinks ({})", backlinks.len())));
            if backlinks.is_empty() {
                graph_col = graph_col.push(text::caption("No incoming backlinks"));
            } else {
                for bl in backlinks {
                    let bl_btn = button::text(format!("← {}", bl.source_title))
                        .class(cosmic::theme::Button::Text)
                        .padding([spacing.space_xs, spacing.space_s])
                        .on_press(Message::SelectNote(bl.source_path));
                    graph_col = graph_col.push(bl_btn);
                }
            }

            graph_col = graph_col.push(text::caption(format!("Outgoing Links ({})", outgoing.len())));
            if outgoing.is_empty() {
                graph_col = graph_col.push(text::caption("No outgoing wikilinks"));
            } else {
                for link in outgoing {
                    if let Some(target_path) = link.resolved_path {
                        let link_btn = button::text(format!("→ {}", link.target))
                            .class(cosmic::theme::Button::Text)
                            .padding([spacing.space_xs, spacing.space_s])
                            .on_press(Message::SelectNote(target_path));
                        graph_col = graph_col.push(link_btn);
                    } else {
                        let ghost = text::caption(format!("⤑ {} (uncreated)", link.target));
                        graph_col = graph_col.push(ghost);
                    }
                }
            }

            inspector_col = inspector_col
                .push(widget::divider::horizontal::default())
                .push(graph_col);

            container(
                scrollable(inspector_col).width(Length::Fill).height(Length::Fill)
            )
            .class(cosmic::theme::Container::Background)
            .width(Length::Fixed(260.0))
            .padding(spacing.space_s)
        } else {
            container(
                Column::new()
                    .spacing(spacing.space_s)
                    .push(text::caption("PROPERTIES"))
                    .push(text::caption("No note selected"))
            )
            .class(cosmic::theme::Container::Background)
            .width(Length::Fixed(260.0))
            .padding(spacing.space_s)
        };

        // Assemble Symmetrical 3-Pane Root Layout
        let mut root_row = Row::new();

        if self.show_sidebar {
            root_row = root_row
                .push(sidebar)
                .push(widget::divider::vertical::default());
        }

        root_row = root_row.push(center_content);

        if self.show_context {
            root_row = root_row
                .push(widget::divider::vertical::default())
                .push(inspector_panel);
        }

        root_row.into()
    }
}


