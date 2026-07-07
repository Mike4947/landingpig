//! Terminal color theme — blue accent on dark background.

use ratatui::style::{Color, Modifier, Style};

/// Primary accent: electric blue (#61AFEF).
pub const ACCENT: Color = Color::Rgb(97, 175, 239);
pub const ACCENT_DIM: Color = Color::Rgb(59, 130, 246);
pub const BG_DARK: Color = Color::Rgb(15, 23, 42);
pub const BG_PANEL: Color = Color::Rgb(30, 41, 59);
pub const FG_PRIMARY: Color = Color::Rgb(226, 232, 240);
pub const FG_MUTED: Color = Color::Rgb(148, 163, 184);
pub const BORDER: Color = Color::Rgb(71, 85, 105);
pub const SUCCESS: Color = Color::Rgb(52, 211, 153);

pub fn accent() -> Style {
    Style::default().fg(ACCENT)
}

pub fn accent_bold() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

pub fn title() -> Style {
    Style::default()
        .fg(ACCENT)
        .add_modifier(Modifier::BOLD)
}

pub fn muted() -> Style {
    Style::default().fg(FG_MUTED)
}

pub fn body() -> Style {
    Style::default().fg(FG_PRIMARY)
}

pub fn success() -> Style {
    Style::default().fg(SUCCESS)
}

pub fn border() -> Style {
    Style::default().fg(BORDER)
}

#[allow(dead_code)]
pub fn panel_bg() -> Style {
    Style::default().bg(BG_PANEL)
}

pub fn processing() -> Style {
    Style::default()
        .fg(ACCENT_DIM)
        .add_modifier(Modifier::ITALIC)
}
