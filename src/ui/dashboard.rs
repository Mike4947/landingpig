//! Main dashboard split-view layout.

use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

use crate::state::{AppState, TokenStats};
use crate::ui::theme;

const CHEATSHEET: &[&str] = &[
    "/import          — pick workspace visually",
    "/design <brief>  — new landing page (required)",
    "/redesign        — improve existing landing page",
    "/redesign <note> — redesign with extra instructions",
    "/model          — open model picker + reasoning",
    "/model <id>     — switch model directly",
    "/read <file>    — inspect a workspace file",
    "/write <file>   — save agent output to disk",
    "/help           — list all commands",
    "Shift+Tab       — toggle thinking on/off",
    "Esc             — stop generation",
    "Tab             — autocomplete paths/models",
    "↑/↓             — command history navigation",
];

pub fn render_dashboard(frame: &mut Frame, area: Rect, state: &AppState) {

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Plain)
        .title(" landingpig ")
        .title_style(theme::title())
        .border_style(theme::border())
        .style(Style::default().bg(theme::BG_DARK));

    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    // Dashed interior container
    let dashed = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Plain)
        .border_style(theme::accent())
        .title(" --- dashboard --- ")
        .title_style(theme::muted());

    let dash_inner = dashed.inner(inner);
    frame.render_widget(dashed, inner);

    // Split: left column | right (top + bottom)
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .margin(1)
        .split(dash_inner);

    render_left_panel(frame, columns[0], state);

    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(columns[1]);

    render_activity_panel(frame, right_rows[0], state);
    render_cheatsheet_panel(frame, right_rows[1]);
}

fn render_left_panel(frame: &mut Frame, area: Rect, state: &AppState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .title(" Status & Environment ")
        .title_style(theme::accent());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let welcome = Paragraph::new("Welcome back, Developer!")
        .style(theme::accent_bold());

    let models_label = if state.available_models.is_empty() {
        "loading...".to_string()
    } else {
        format!("{} available", state.available_models.len())
    };

    let metadata = vec![
        format!("Model:      {}", state.model),
        format!("Models:     {models_label}"),
        format!("Usage:      {}", state.token_stats.usage_summary()),
        format!("Workspace:  {}", state.workspace.display()),
        format!("Files:      {}", state.file_context.len()),
        format!(
            "Thinking:   {}",
            if state.thinking_mode == crate::state::ThinkingMode::On {
                "ON (reasoning enabled)"
            } else {
                "OFF"
            }
        ),
    ];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(5)])
        .split(inner);

    frame.render_widget(welcome, chunks[0]);

    let bar_width = chunks[1].width.saturating_sub(14).clamp(12, 28) as usize;
    let mut meta_lines: Vec<Line> = metadata.into_iter().map(Line::from).collect();
    meta_lines.push(usage_progress_bar(&state.token_stats, bar_width));

    let meta = Paragraph::new(meta_lines)
        .style(theme::body())
        .wrap(Wrap { trim: true });

    frame.render_widget(meta, chunks[1]);
}

fn usage_progress_bar(stats: &TokenStats, bar_width: usize) -> Line<'static> {
    let fraction = stats.usage_fraction();
    let width = bar_width.clamp(12, 32);
    let filled = ((fraction * width as f64).round() as usize).min(width);
    let empty = width.saturating_sub(filled);

    let in_tok = stats.input_tokens + stats.live_input;
    let out_tok = stats.output_tokens + stats.live_output;
    let total = in_tok + out_tok;

    let (in_filled, out_filled) = if filled == 0 {
        (0, 0)
    } else if total == 0 {
        (filled, 0)
    } else {
        let in_share = in_tok as f64 / total as f64;
        let in_f = ((filled as f64 * in_share).round() as usize).min(filled);
        (in_f, filled.saturating_sub(in_f))
    };

    let input_style = if fraction >= 0.9 {
        Style::default().fg(Color::Rgb(248, 113, 113))
    } else {
        Style::default().fg(theme::ACCENT)
    };
    let output_style = if fraction >= 0.9 {
        Style::default().fg(Color::Rgb(251, 146, 60))
    } else if fraction >= 0.75 {
        Style::default().fg(Color::Rgb(251, 191, 36))
    } else {
        Style::default().fg(theme::SUCCESS)
    };
    let track = Style::default().fg(theme::BORDER);

    let mut spans = vec![Span::styled("            ", theme::body())];
    if in_filled > 0 {
        spans.push(Span::styled("█".repeat(in_filled), input_style));
    }
    if out_filled > 0 {
        spans.push(Span::styled("█".repeat(out_filled), output_style));
    }
    if empty > 0 {
        spans.push(Span::styled("░".repeat(empty), track));
    }

    Line::from(spans)
}

fn render_activity_panel(frame: &mut Frame, area: Rect, state: &AppState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .title(" Recent Activity ")
        .title_style(theme::accent());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let items: Vec<ListItem> = state
        .activity_log
        .iter()
        .map(|entry| ListItem::new(entry.relative_label()).style(theme::body()))
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

fn render_cheatsheet_panel(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .title(" Quick Start ")
        .title_style(theme::accent());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let text = CHEATSHEET.join("\n");
    let para = Paragraph::new(text)
        .style(theme::muted())
        .wrap(Wrap { trim: true });

    frame.render_widget(para, inner);
}

pub fn render_chat_separator(frame: &mut Frame, area: Rect) {
    let line = "─".repeat(area.width as usize);
    let para = Paragraph::new(line).style(theme::accent());
    frame.render_widget(para, area);
}

pub fn render_chat_history(frame: &mut Frame, area: Rect, state: &AppState) {
    let text = if state.chat_history.is_empty() {
        "No messages yet. Type /import <path> to load your landing page components.".to_string()
    } else {
        state.chat_history.join("\n")
    };

    let scroll_offset = state
        .chat_history
        .len()
        .saturating_sub(area.height as usize) as u16;

    let para = Paragraph::new(text)
        .style(theme::body())
        .wrap(Wrap { trim: true })
        .scroll((scroll_offset, 0));

    frame.render_widget(para, area);
}
