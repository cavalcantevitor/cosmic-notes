use cosmic::app::Core;
use cosmic::iced::Length;
use cosmic::iced::Task;
use cosmic::widget::text_editor::{self as te, Action as EditorAction, Content as EditorContent};
use cosmic::widget::{self, button, container, scrollable, text, Column, Row};
use cosmic::Element;
use std::path::{Path, PathBuf};

use crate::core::{Note, Vault};

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
}

pub struct AppModel {
    core: Core,
    vault: Vault,
    notes: Vec<Note>,
    selected_note_path: Option<PathBuf>,
    active_note: Option<Note>,
    editor_content: EditorContent,
    parsed_markdown: Vec<widget::markdown::Item>,
    view_mode: ViewMode,
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

Click **New Note** above or start editing right here!
"#;
            if let Ok(welcome_note) = vault.write_note(Path::new("Welcome.md"), welcome_content) {
                scanned.push(welcome_note);
            }
        }

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

        let app = Self {
            core,
            vault,
            notes: scanned,
            selected_note_path: selected,
            active_note: active,
            editor_content,
            parsed_markdown,
            view_mode: ViewMode::Split,
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
        let count_text = format!("{} notes", self.notes.len());
        vec![
            text::title3("COSMIC Notes").into(),
            text::body(count_text).into(),
        ]
    }

    fn header_end(&self) -> Vec<Element<'_, Self::Message>> {
        vec![
            button::text("New Note")
                .on_press(Message::CreateNewNote)
                .into(),
            button::text("Edit")
                .on_press(Message::SetViewMode(ViewMode::Editor))
                .into(),
            button::text("Split")
                .on_press(Message::SetViewMode(ViewMode::Split))
                .into(),
            button::text("Preview")
                .on_press(Message::SetViewMode(ViewMode::Preview))
                .into(),
        ]
    }

    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::SelectNote(path) => {
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
                    self.editor_content = EditorContent::with_text(&note.raw_content);
                    self.parsed_markdown = widget::markdown::parse(&note.body).collect();
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
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let spacing = cosmic::theme::spacing();

        // Left sidebar: Note list
        let mut note_items = Column::new().spacing(spacing.space_xxs);
        for note in &self.notes {
            let is_selected = self.selected_note_path.as_ref() == Some(&note.path);
            let title = if note.title.is_empty() {
                "Untitled"
            } else {
                &note.title
            };

            let mut note_btn = button::text(title.to_string())
                .width(Length::Fill)
                .on_press(Message::SelectNote(note.path.clone()));

            if is_selected {
                note_btn = note_btn.class(cosmic::theme::Button::Suggested);
            } else {
                note_btn = note_btn.class(cosmic::theme::Button::Text);
            }

            note_items = note_items.push(note_btn);
        }

        let sidebar = container(
            scrollable(note_items).width(Length::Fill).height(Length::Fill)
        )
        .width(Length::Fixed(240.0))
        .padding(spacing.space_s);

        // Center content area: Active note view
        let center_content: Element<'_, Self::Message> = if let Some(ref note) = self.active_note {
            let title_header = text::title2(&note.title);

            let tags_row = if !note.tags.is_empty() {
                let mut r = Row::new().spacing(spacing.space_xs);
                for tag in &note.tags {
                    r = r.push(text::caption(format!("#{tag}")));
                }
                Some(r)
            } else {
                None
            };

            let style = widget::markdown::Style::from_palette(cosmic::iced::theme::Palette::DARK);
            let md_settings = widget::markdown::Settings::with_style(style);
            let md_view = widget::markdown::view(&self.parsed_markdown, md_settings)
                .map(|_| Message::LinkClicked);

            let preview_view = container(
                scrollable(
                    Column::new()
                        .spacing(spacing.space_m)
                        .push(md_view)
                )
                .height(Length::Fill)
                .width(Length::Fill)
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(spacing.space_m);

            let editor_widget = container(
                te::text_editor(&self.editor_content)
                    .placeholder("Type your markdown note here...")
                    .on_action(Message::EditorAction)
                    .height(Length::Fill)
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(spacing.space_m);

            let body_surface: Element<'_, Self::Message> = match self.view_mode {
                ViewMode::Editor => editor_widget.into(),
                ViewMode::Preview => preview_view.into(),
                ViewMode::Split => {
                    Row::new()
                        .spacing(spacing.space_m)
                        .push(container(editor_widget).width(Length::FillPortion(1)))
                        .push(container(preview_view).width(Length::FillPortion(1)))
                        .into()
                }
            };

            let mut note_col = Column::new().spacing(spacing.space_s).push(title_header);
            if let Some(t_row) = tags_row {
                note_col = note_col.push(t_row);
            }
            note_col = note_col.push(body_surface);

            container(note_col)
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(spacing.space_m)
                .into()
        } else {
            container(
                Column::new()
                    .spacing(spacing.space_m)
                    .push(text::title3("No note selected"))
                    .push(
                        button::text("Create Note")
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
