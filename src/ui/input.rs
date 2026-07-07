//! Input line, processing animation, and status bar.

use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::state::{AppPhase, AppState};
use crate::ui::theme;

const PROCESSING_FRAMES: &[&str] = &[
    "while(optimizing) { evaluate_ux_metrics(); }",
    "while(optimizing) { adjust_tailwind_layouts(); }",
    "while(optimizing) { refine_cta_hierarchy(); }",
    "* Simmering...",
    "* Simmering..",
    "* Simmering.",
];

const PROMPT: &str = "> ";

pub fn point_in_area(col: u16, row: u16, area: Rect) -> bool {
    col >= area.x
        && col < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

pub fn render_input_line(frame: &mut Frame, area: Rect, state: &AppState) {
    if state.phase == AppPhase::Processing {
        let frame_idx = state.processing_frame % PROCESSING_FRAMES.len();
        let status = if state.status_message.is_empty() {
            PROCESSING_FRAMES[frame_idx].to_string()
        } else {
            state.status_message.clone()
        };
        let prompt = format!("{status}  │  Esc — stop generation");
        let para = Paragraph::new(prompt)
            .style(theme::processing())
            .style(Style::default().bg(theme::BG_PANEL));
        frame.render_widget(para, area);
        return;
    }

    let cursor = state.input_cursor.min(state.input_buffer.len());
    let (before, after) = state.input_buffer.split_at(cursor);

    let cursor_char = if state.input_focused && state.cursor_visible {
        "▌"
    } else {
        ""
    };
    let content = format!("{before}{cursor_char}{after}");
    let display = format!("{PROMPT}{content}");

    let line = Line::from(Span::styled(display, theme::body()));
    frame.render_widget(
        Paragraph::new(line)
            .style(Style::default().bg(theme::BG_PANEL))
            .wrap(Wrap { trim: false }),
        area,
    );

    if !state.completion_suggestions.is_empty() {
        let suggestion_area = Rect {
            y: area.y.saturating_sub(1),
            height: 1,
            ..area
        };
        let hint = format!(
            "  tab: {}",
            state.completion_suggestions.first().unwrap_or(&String::new())
        );
        let hint_para = Paragraph::new(hint).style(theme::muted());
        frame.render_widget(hint_para, suggestion_area);
    }
}

pub fn render_status_bar(frame: &mut Frame, area: Rect, state: &AppState) {
    let thinking = if state.thinking_mode == crate::state::ThinkingMode::On {
        "Thinking: ON"
    } else {
        "Thinking: OFF"
    };

    let phase = match state.phase {
        AppPhase::Boot => "BOOT",
        AppPhase::Dashboard => "READY",
        AppPhase::Processing => "STOP: Esc",
    };

    let left = format!("↑/↓ History | Tab Complete | Shift+Tab Thinking | {thinking}");
    let right = format!("{phase} | {}", state.model);

    let bar = Paragraph::new(format!("{left}  {right}"))
        .style(theme::muted())
        .style(Style::default().bg(theme::BG_PANEL));

    frame.render_widget(bar, area);
}
