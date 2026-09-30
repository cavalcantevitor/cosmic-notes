use cosmic::app::Core;
use cosmic::iced::Length;
use cosmic::iced::Task;
use cosmic::widget::text_editor::{self as te, Action as EditorAction, Content as EditorContent};
use cosmic::widget::{self, button, container, scrollable, text, Column, Row};
use cosmic::Element;
use std::path::{Path, PathBuf};

use crate::core::{Note, Vault, VaultEvent};
use crate::search::VaultIndex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Editor,
    Split,
    Preview,
}

#[derive(Clone, Debug)]
pub enum Message {
    SelectNote(PathBuf),
    CreateNewNote,
    SetViewMode(ViewMode),
    EditorAction(EditorAction),
    LinkClicked,
    SearchInputChanged(String),
    ToggleSearch,
    CloseNoteTab(PathBuf),
    VaultFileEvent(VaultEvent),
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
    view_mode: ViewMode,
    search_query: String,
    search_active: bool,
}

impl AppModel {
    pub fn new(core: Core, vault_path: PathBuf) -> (Self, Task<cosmic::Action<Message>>) {
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

        let parsed_markdown = active
            .as_ref()
            .map(|n| widget::markdown::parse(&n.body).collect())
            .unwrap_or_default();

        let open_tabs = selected.iter().cloned().collect();

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
            view_mode: ViewMode::Split,
            search_query: String::new(),
            search_active: false,
        };

        (app, Task::none())
    }
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

    fn init(core: Core, flags: Self::Flags) -> (Self, Task<cosmic::Action<Self::Message>>) {
        Self::new(core, flags)
    }

    fn header_start(&self) -> Vec<Element<'_, Self::Message>> {
        let menu_btn = |label: &'static str, msg: Message, is_active: bool| {
            let b = button::text(label)
                .class(if is_active {
                    cosmic::theme::Button::Suggested
                } else {
                    cosmic::theme::Button::Text
                })
                .on_press(msg);
            b.into()
        };

        vec![
            menu_btn("New Note", Message::CreateNewNote, false),
            menu_btn("Edit", Message::SetViewMode(ViewMode::Editor), self.view_mode == ViewMode::Editor),
            menu_btn("Split", Message::SetViewMode(ViewMode::Split), self.view_mode == ViewMode::Split),
            menu_btn("Preview", Message::SetViewMode(ViewMode::Preview), self.view_mode == ViewMode::Preview),
        ]
    }

    fn header_end(&self) -> Vec<Element<'_, Self::Message>> {
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

        vec![search_btn.into()]
    }

    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::SelectNote(path) => {
                if !self.open_tabs.contains(&path) {
                    self.open_tabs.push(path.clone());
                }
                if let Ok(note) = self.vault.read_note(&path) {
                    self.editor_content = EditorContent::with_text(&note.raw_content);
                    self.parsed_markdown = widget::markdown::parse(&note.body).collect();
                    self.active_note = Some(note);
                    self.selected_note_path = Some(path);
                }
            }
            Message::CreateNewNote => {
                let note_count = self.notes.len() + 1;
                let filename = format!("Untitled_{note_count}.md");
                let initial_content = format!("# Untitled {}\n\nStart typing your note here...", note_count);
                if let Ok(note) = self.vault.write_note(Path::new(&filename), &initial_content) {
                    let _ = self.vault_index.update_note(&note);
                    self.editor_content = EditorContent::with_text(&note.raw_content);
                    self.parsed_markdown = widget::markdown::parse(&note.body).collect();
                    self.open_tabs.push(note.path.clone());
                    self.selected_note_path = Some(note.path.clone());
                    self.active_note = Some(note.clone());
                    self.notes.push(note);
                }
            }
            Message::EditorAction(action) => {
                let is_edit = action.is_edit();
                self.editor_content.perform(action);

                if is_edit {
                    let current_text = self.editor_content.text();
                    // Live update parsed preview
                    self.parsed_markdown = widget::markdown::parse(&current_text).collect();

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
            Message::CloseNoteTab(path) => {
                self.open_tabs.retain(|p| p != &path);
                if self.selected_note_path.as_ref() == Some(&path) {
                    let next = self.open_tabs.last().cloned();
                    if let Some(next_path) = next {
                        if let Ok(note) = self.vault.read_note(&next_path) {
                            self.editor_content = EditorContent::with_text(&note.raw_content);
                            self.parsed_markdown = widget::markdown::parse(&note.body).collect();
                            self.active_note = Some(note);
                            self.selected_note_path = Some(next_path);
                        }
                    } else {
                        self.active_note = None;
                        self.selected_note_path = None;
                        self.editor_content = EditorContent::new();
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
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let spacing = cosmic::theme::spacing();

        // 1. Left sidebar pane: COSMIC Files styling
        let mut sidebar_col = Column::new().spacing(spacing.space_xs);

        if self.search_active {
            let search_bar = widget::text_input("Search notes...", &self.search_query)
                .on_input(Message::SearchInputChanged)
                .width(Length::Fill);
            sidebar_col = sidebar_col
                .push(search_bar)
                .push(widget::divider::horizontal::default());
        }

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
                .padding([spacing.space_xxs, spacing.space_xs])
                .on_press(Message::SelectNote(note.path.clone()));

            if is_selected {
                item_btn = item_btn.class(cosmic::theme::Button::Suggested);
            } else {
                item_btn = item_btn.class(cosmic::theme::Button::Text);
            }

            item_btn.into()
        };

        let mut note_items = Column::new().spacing(spacing.space_xxs);

        if self.search_query.trim().is_empty() {
            for note in &self.notes {
                let is_selected = self.selected_note_path.as_ref() == Some(&note.path);
                note_items = note_items.push(render_note_item(note, is_selected));
            }
        } else {
            let matches_index = self.vault_index.quick_search(&self.search_query, 50);
            for m in matches_index {
                let is_selected = self.selected_note_path.as_ref() == Some(&m.item.path);
                if let Some(note) = self.notes.iter().find(|n| n.path == m.item.path) {
                    note_items = note_items.push(render_note_item(note, is_selected));
                }
            }
        }

        let sidebar = container(
            sidebar_col.push(scrollable(note_items).width(Length::Fill).height(Length::Fill))
        )
        .class(cosmic::theme::Container::Background)
        .width(Length::Fixed(240.0))
        .padding(spacing.space_s);

        // 2. Main Workspace Area
        let center_content: Element<'_, Self::Message> = if let Some(ref note) = self.active_note {
            // Editor & Preview Surface (Completely borderless editor)
            let style = widget::markdown::Style::from_palette(cosmic::iced::theme::Palette::DARK);
            let md_settings = widget::markdown::Settings::with_style(style);
            let md_view = widget::markdown::view(&self.parsed_markdown, md_settings)
                .map(|_| Message::LinkClicked);

            let preview_view = scrollable(
                Column::new()
                    .spacing(spacing.space_m)
                    .push(md_view)
            )
            .height(Length::Fill)
            .width(Length::Fill);

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

            let body_surface: Element<'_, Self::Message> = match self.view_mode {
                ViewMode::Editor => editor_widget.into(),
                ViewMode::Preview => preview_view.into(),
                ViewMode::Split => {
                    Row::new()
                        .spacing(spacing.space_m)
                        .push(container(editor_widget).width(Length::FillPortion(1)))
                        .push(widget::divider::vertical::default())
                        .push(container(preview_view).width(Length::FillPortion(1)))
                        .into()
                }
            };

            // Bottom Links & Backlinks
            let backlinks = self.vault_index.backlinks(&note.path);
            let outgoing = self.vault_index.outgoing_links(&note.path);
            let has_links = !backlinks.is_empty() || !outgoing.is_empty();

            let mut link_panel = Row::new().spacing(spacing.space_xs).align_y(cosmic::iced::Alignment::Center);

            if !backlinks.is_empty() {
                link_panel = link_panel.push(text::caption("Backlinks:"));
                for bl in backlinks {
                    let bl_btn = button::text(format!("← {}", bl.source_title))
                        .class(cosmic::theme::Button::Text)
                        .on_press(Message::SelectNote(bl.source_path));
                    link_panel = link_panel.push(bl_btn);
                }
            }

            if !outgoing.is_empty() {
                link_panel = link_panel.push(text::caption("Links:"));
                for link in outgoing {
                    if let Some(target_path) = link.resolved_path {
                        let link_btn = button::text(format!("→ {}", link.target))
                            .class(cosmic::theme::Button::Text)
                            .on_press(Message::SelectNote(target_path));
                        link_panel = link_panel.push(link_btn);
                    } else {
                        let ghost = text::caption(format!("⤑ {} (uncreated)", link.target));
                        link_panel = link_panel.push(ghost);
                    }
                }
            }

            let mut workspace_col = Column::new()
                .spacing(spacing.space_s)
                .push(body_surface);

            if has_links {
                workspace_col = workspace_col
                    .push(widget::divider::horizontal::default())
                    .push(link_panel);
            }

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

        Row::new()
            .push(sidebar)
            .push(widget::divider::vertical::default())
            .push(center_content)
            .into()
    }
}


