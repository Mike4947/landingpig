//! Workspace import picker — visual directory browser modal.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

use crate::fs::picker::PickerEntryKind;
use crate::state::ImportPickerState;
use crate::ui::theme;

pub struct PickerLayout {
    pub modal: Rect,
    pub list: Rect,
}

pub fn picker_layout(area: Rect) -> PickerLayout {
    let modal_width = area.width.saturating_sub(8).clamp(48, 90);
    let modal_height = area.height.saturating_sub(6).clamp(14, 28);

    let modal = Rect {
        x: area.x + (area.width.saturating_sub(modal_width)) / 2,
        y: area.y + (area.height.saturating_sub(modal_height)) / 2,
        width: modal_width,
        height: modal_height,
    };

    let list = Rect {
        x: modal.x + 2,
        y: modal.y + 4,
        width: modal.width.saturating_sub(4),
        height: modal.height.saturating_sub(7),
    };

    PickerLayout { modal, list }
}

pub fn render_import_picker(frame: &mut Frame, picker: &ImportPickerState) {
    let area = frame.area();
    let layout = picker_layout(area);

    Clear.render(layout.modal, frame.buffer_mut());

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::accent())
        .title(" Select Landing Page Workspace ")
        .title_style(theme::accent_bold())
        .style(Style::default().bg(theme::BG_PANEL));

    let inner = block.inner(layout.modal);
    frame.render_widget(block, layout.modal);

    let path_line = format!("📁 {}", picker.current_dir.display());
    let path_para = Paragraph::new(path_line)
        .style(theme::body())
        .alignment(Alignment::Left);
    frame.render_widget(
        path_para,
        Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: 1,
        },
    );

    let visible = layout.list.height as usize;
    let items: Vec<ListItem> = picker
        .entries
        .iter()
        .enumerate()
        .skip(picker.scroll)
        .take(visible)
        .map(|(idx, entry)| {
            let icon = match entry.kind {
                PickerEntryKind::ImportHere => "✔ ",
                PickerEntryKind::Parent => "↑ ",
                PickerEntryKind::Drive => "⛁ ",
                PickerEntryKind::Directory => "▸ ",
            };
            let label = format!("{icon}{}", entry.label);
            let style = if idx == picker.selected {
                Style::default()
                    .fg(theme::BG_DARK)
                    .bg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                theme::body()
            };
            ListItem::new(label).style(style)
        })
        .collect();

    let list = List::new(items).style(Style::default().bg(theme::BG_PANEL));
    frame.render_widget(list, layout.list);

    let help = "↑/↓ move  Enter open  ⛁ drives  Esc cancel";
    let help_para = Paragraph::new(help).style(theme::muted());
    frame.render_widget(
        help_para,
        Rect {
            x: inner.x,
            y: inner.y + inner.height.saturating_sub(2),
            width: inner.width,
            height: 1,
        },
    );

    if !picker.error.is_empty() {
        let err = Paragraph::new(picker.error.as_str()).style(Style::default().fg(Color::Red));
        frame.render_widget(
            err,
            Rect {
                x: inner.x,
                y: inner.y + inner.height.saturating_sub(1),
                width: inner.width,
                height: 1,
            },
        );
    }
}

pub fn picker_row_at_y(layout: &PickerLayout, picker: &ImportPickerState, y: u16) -> Option<usize> {
    if y < layout.list.y || y >= layout.list.y + layout.list.height {
        return None;
    }
    let row = (y - layout.list.y) as usize + picker.scroll;
    if row < picker.entries.len() {
        Some(row)
    } else {
        None
    }
}

pub fn ensure_picker_visible(picker: &mut ImportPickerState, visible_rows: usize) {
    if picker.selected < picker.scroll {
        picker.scroll = picker.selected;
    } else if picker.selected >= picker.scroll + visible_rows {
        picker.scroll = picker.selected.saturating_sub(visible_rows.saturating_sub(1));
    }
}

pub fn render_model_picker(frame: &mut Frame, picker: &crate::state::ModelPickerState) {
    let area = frame.area();
    let layout = picker_layout(area);

    Clear.render(layout.modal, frame.buffer_mut());

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::accent())
        .title(" Select Anthropic Model ")
        .title_style(theme::accent_bold())
        .style(Style::default().bg(theme::BG_PANEL));

    let inner = block.inner(layout.modal);
    frame.render_widget(block, layout.modal);

    let subtitle = format!("Active: {}", picker.active_model);
    frame.render_widget(
        Paragraph::new(subtitle).style(theme::muted()),
        Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: 1,
        },
    );

    let thinking_label = if picker.thinking_enabled {
        "⟡ Reasoning (Thinking): ON"
    } else {
        "⟡ Reasoning (Thinking): OFF"
    };

    let visible = layout.list.height as usize;
    let mut rows: Vec<(usize, String)> = vec![(0, thinking_label.to_string())];
    for (i, model) in picker.models.iter().enumerate() {
        let display = model.display_name.as_deref().unwrap_or(&model.id);
        let active = if model.id == picker.active_model {
            " ◉"
        } else {
            ""
        };
        rows.push((i + 1, format!("{} — {}{}", model.id, display, active)));
    }

    let items: Vec<ListItem> = rows
        .iter()
        .skip(picker.scroll)
        .take(visible)
        .map(|(idx, label)| {
            let style = if *idx == picker.selected {
                Style::default()
                    .fg(theme::BG_DARK)
                    .bg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                theme::body()
            };
            ListItem::new(label.as_str()).style(style)
        })
        .collect();

    frame.render_widget(
        List::new(items).style(Style::default().bg(theme::BG_PANEL)),
        layout.list,
    );

    let help = "↑/↓ move  Enter select  Space toggle thinking  Click row  Esc cancel";
    frame.render_widget(
        Paragraph::new(help).style(theme::muted()),
        Rect {
            x: inner.x,
            y: inner.y + inner.height.saturating_sub(2),
            width: inner.width,
            height: 1,
        },
    );

    if !picker.error.is_empty() {
        frame.render_widget(
            Paragraph::new(picker.error.as_str()).style(Style::default().fg(Color::Red)),
            Rect {
                x: inner.x,
                y: inner.y + inner.height.saturating_sub(1),
                width: inner.width,
                height: 1,
            },
        );
    }
}

pub fn model_picker_row_at_y(
    layout: &PickerLayout,
    picker: &crate::state::ModelPickerState,
    y: u16,
) -> Option<usize> {
    if y < layout.list.y || y >= layout.list.y + layout.list.height {
        return None;
    }
    let row = (y - layout.list.y) as usize + picker.scroll;
    if row < picker.row_count() {
        Some(row)
    } else {
        None
    }
}

pub fn ensure_model_picker_visible(picker: &mut crate::state::ModelPickerState, visible_rows: usize) {
    if picker.selected < picker.scroll {
        picker.scroll = picker.selected;
    } else if picker.selected >= picker.scroll + visible_rows {
        picker.scroll = picker.selected.saturating_sub(visible_rows.saturating_sub(1));
    }
}

