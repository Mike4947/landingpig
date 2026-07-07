//! Boot sequence — ASCII banner, API key onboarding, and auth gate.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::state::{AppState, BootMode};
use crate::ui::banner;
use crate::ui::theme;

pub fn render_boot(frame: &mut Frame, state: &AppState) {
    let area = frame.area();
    let term_width = area.width;

    let banner_lines = banner::build_banner(term_width);
    let banner_text = banner_lines.join("\n");
    let banner_height = banner_lines.len() as u16;

    let block = Block::default()
        .borders(Borders::NONE)
        .style(Style::default().bg(theme::BG_DARK));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let banner = Paragraph::new(banner_text)
        .style(theme::accent_bold())
        .alignment(Alignment::Center);

    match state.boot_mode {
        BootMode::ApiKeySetup => {
            let setup_height = 9u16;
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(0),
                    Constraint::Length(banner_height),
                    Constraint::Length(1),
                    Constraint::Length(setup_height),
                    Constraint::Min(0),
                ])
                .split(inner);

            frame.render_widget(banner, chunks[1]);
            render_api_key_setup(frame, chunks[3], state);
        }
        BootMode::Authenticated => {
            let status_height = 1u16;
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(0),
                    Constraint::Length(banner_height),
                    Constraint::Length(1),
                    Constraint::Length(status_height),
                    Constraint::Min(0),
                ])
                .split(inner);

            frame.render_widget(banner, chunks[1]);

            let status = Paragraph::new("✔ Authentication successful. Press Enter to continue")
                .style(theme::success())
                .alignment(Alignment::Center);

            frame.render_widget(status, chunks[3]);
        }
    }
}

fn render_api_key_setup(frame: &mut Frame, area: Rect, state: &AppState) {
    let display_key = mask_api_key(&state.api_key_buffer);

    let body = format!(
        "No active API key found.\n\
         Please paste your key below to save it securely:\n\
         (Ctrl+V / middle-click to paste, Enter to save, Ctrl+C to quit)\n\n\
         > {display_key}"
    );

    let setup = Paragraph::new(body)
        .style(theme::body())
        .wrap(Wrap { trim: true });

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::accent())
        .title(" Anthropic API Key Setup ")
        .title_style(theme::accent_bold())
        .style(Style::default().bg(theme::BG_PANEL));

    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(setup, inner);

    if !state.boot_error.is_empty() {
        let err_area = Rect {
            y: area.y.saturating_add(area.height.saturating_sub(1)),
            height: 1,
            ..area
        };
        let err = Paragraph::new(state.boot_error.as_str())
            .style(Style::default().fg(Color::Red))
            .alignment(Alignment::Center);
        frame.render_widget(err, err_area);
    }
}

fn mask_api_key(key: &str) -> String {
    if key.len() <= 16 {
        return key.to_string();
    }
    format!("{}...", &key[..16])
}
