//! Terminal UI engine.

pub mod banner;
pub mod boot;
pub mod dashboard;
pub mod input;
pub mod picker;
pub mod theme;

use std::io::{self, IsTerminal};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use crossterm::cursor::{Hide, Show};
use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::prelude::*;
use tokio::runtime::Runtime;

use crate::api::format_anyhow;
use crate::app::{is_stop_generation_key, App};
use crate::state::{AppPhase, BootMode, ThinkingMode};

impl App {
    /// Blocking terminal event loop. Runs until the user explicitly quits.
    pub fn run_ui(&mut self, rt: &Runtime) -> Result<()> {
        ensure_tty()?;

        enable_raw_mode().context("failed to enable raw terminal mode")?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, Hide, EnableBracketedPaste, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = self.run_event_loop(rt, &mut terminal);

        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            DisableMouseCapture,
            DisableBracketedPaste,
            LeaveAlternateScreen,
            Show
        )?;
        terminal.show_cursor()?;

        result
    }

    fn run_event_loop(
        &mut self,
        rt: &Runtime,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<()> {
        let tick_rate = Duration::from_millis(120);
        let mut last_tick = std::time::Instant::now();

        // Always enter the rendering pipeline — missing API key routes to onboarding,
        // not an early return.
        loop {
            let viewport = terminal.size()?;
            rt.block_on(async {
                let mut state = self.state.write().await;
                state.viewport = Rect::new(0, 0, viewport.width, viewport.height);
            });

            rt.block_on(async {
                let state = self.state.read().await;
                terminal.draw(|frame| self.render(frame, &state))
            })?;

            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or(Duration::ZERO);

            // Block on crossterm input synchronously — never exit until quit or boot complete + quit.
            while event::poll(timeout)? {
                match event::read()? {
                    Event::Key(key) => {
                        if rt.block_on(self.handle_key(key))? {
                            return Ok(());
                        }
                    }
                    Event::Paste(text) => {
                        if rt.block_on(self.handle_paste(text))? {
                            return Ok(());
                        }
                    }
                    Event::Mouse(mouse) => {
                        if rt.block_on(self.handle_mouse(mouse))? {
                            return Ok(());
                        }
                    }
                    _ => {}
                }

                // Drain any additional queued events without waiting.
                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }

            if last_tick.elapsed() >= tick_rate {
                rt.block_on(async {
                    let mut state = self.state.write().await;
                    if state.phase == AppPhase::Processing {
                        state.processing_frame = state.processing_frame.wrapping_add(1);
                    } else if matches!(state.phase, AppPhase::Dashboard | AppPhase::Boot) {
                        state.blink_tick = state.blink_tick.wrapping_add(1);
                        if state.blink_tick % 5 == 0 {
                            state.cursor_visible = !state.cursor_visible;
                        }
                    }
                });
                last_tick = std::time::Instant::now();
            }
        }
    }

    async fn handle_paste(&mut self, text: String) -> Result<bool> {
        let phase = self.state.read().await.phase;
        if phase != AppPhase::Boot {
            return Ok(false);
        }

        let boot_mode = self.state.read().await.boot_mode;
        if boot_mode != BootMode::ApiKeySetup {
            return Ok(false);
        }

        let cleaned = sanitize_api_key_input(&text);
        if cleaned.is_empty() {
            return Ok(false);
        }

        let mut state = self.state.write().await;
        state.api_key_buffer = cleaned;
        state.api_key_cursor = state.api_key_buffer.len();
        state.boot_error.clear();
        Ok(false)
    }

    fn render(&self, frame: &mut Frame, state: &crate::state::AppState) {
        match state.phase {
            AppPhase::Boot => boot::render_boot(frame, state),
            AppPhase::Dashboard | AppPhase::Processing => {
                let area = frame.area();

                let root = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Percentage(55),
                        Constraint::Length(1),
                        Constraint::Min(4),
                        Constraint::Min(3),
                        Constraint::Length(1),
                    ])
                    .split(area);

                dashboard::render_dashboard(frame, root[0], state);
                dashboard::render_chat_separator(frame, root[1]);
                dashboard::render_chat_history(frame, root[2], state);
                input::render_input_line(frame, root[3], state);
                input::render_status_bar(frame, root[4], state);

                if let Some(picker) = &state.import_picker {
                    picker::render_import_picker(frame, picker);
                }
                if let Some(picker) = &state.model_picker {
                    picker::render_model_picker(frame, picker);
                }
            }
        }
    }

    async fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        if self.state.read().await.model_picker.is_some() {
            return self.handle_model_picker_key(key).await;
        }
        if self.state.read().await.import_picker.is_some() {
            return self.handle_import_picker_key(key).await;
        }

        let phase = self.state.read().await.phase;

        match phase {
            AppPhase::Boot => self.handle_boot_key(key).await,
            AppPhase::Dashboard | AppPhase::Processing => self.handle_dashboard_key(key).await,
        }
    }

    async fn handle_import_picker_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc => {
                self.close_import_picker().await;
            }
            KeyCode::Enter => {
                if let Err(e) = self.picker_activate_selection().await {
                    let mut state = self.state.write().await;
                    if let Some(picker) = state.import_picker.as_mut() {
                        picker.error = e.to_string();
                    }
                }
            }
            KeyCode::Up => {
                let mut state = self.state.write().await;
                if let Some(picker) = state.import_picker.as_mut() {
                    picker.selected = picker.selected.saturating_sub(1);
                    picker.error.clear();
                    picker::ensure_picker_visible(picker, 12);
                }
            }
            KeyCode::Down => {
                let mut state = self.state.write().await;
                if let Some(picker) = state.import_picker.as_mut() {
                    if picker.selected + 1 < picker.entries.len() {
                        picker.selected += 1;
                    }
                    picker.error.clear();
                    picker::ensure_picker_visible(picker, 12);
                }
            }
            _ => {}
        }
        Ok(false)
    }

    async fn handle_model_picker_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc => {
                self.close_model_picker().await;
            }
            KeyCode::Char(' ') => {
                let mut state = self.state.write().await;
                if let Some(picker) = state.model_picker.as_mut() {
                    picker.thinking_enabled = !picker.thinking_enabled;
                    picker.error.clear();
                }
            }
            KeyCode::Enter => {
                if let Err(e) = self.model_picker_activate().await {
                    let mut state = self.state.write().await;
                    if let Some(picker) = state.model_picker.as_mut() {
                        picker.error = e.to_string();
                    }
                }
            }
            KeyCode::Up => {
                let mut state = self.state.write().await;
                if let Some(picker) = state.model_picker.as_mut() {
                    picker.selected = picker.selected.saturating_sub(1);
                    picker.error.clear();
                    picker::ensure_model_picker_visible(picker, 12);
                }
            }
            KeyCode::Down => {
                let mut state = self.state.write().await;
                if let Some(picker) = state.model_picker.as_mut() {
                    let max = picker.row_count().saturating_sub(1);
                    if picker.selected < max {
                        picker.selected += 1;
                    }
                    picker.error.clear();
                    picker::ensure_model_picker_visible(picker, 12);
                }
            }
            _ => {}
        }
        Ok(false)
    }

    async fn handle_mouse(&mut self, mouse: MouseEvent) -> Result<bool> {
        if self.state.read().await.model_picker.is_some() {
            return self.handle_model_picker_mouse(mouse).await;
        }
        if self.state.read().await.import_picker.is_some() {
            return self.handle_import_picker_mouse(mouse).await;
        }

        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return Ok(false);
        }

        let phase = self.state.read().await.phase;
        if matches!(phase, AppPhase::Dashboard | AppPhase::Processing) {
            let viewport = self.state.read().await.viewport;
            let input_area = input_area_rect(viewport);
            if input::point_in_area(mouse.column, mouse.row, input_area) {
                let mut state = self.state.write().await;
                state.input_focused = true;
                state.cursor_visible = true;
                state.blink_tick = 0;
                let prompt_len = 2u16; // "> "
                let col = mouse.column.saturating_sub(input_area.x + prompt_len);
                state.input_cursor = (col as usize).min(state.input_buffer.len());
            }
        }

        Ok(false)
    }

    async fn handle_model_picker_mouse(&mut self, mouse: MouseEvent) -> Result<bool> {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return Ok(false);
        }

        let (layout, picker) = {
            let state = self.state.read().await;
            let picker = state.model_picker.clone().context("model picker closed")?;
            let layout = picker::picker_layout(state.viewport);
            (layout, picker)
        };

        if let Some(row) = picker::model_picker_row_at_y(&layout, &picker, mouse.row) {
            {
                let mut state = self.state.write().await;
                if let Some(picker) = state.model_picker.as_mut() {
                    picker.selected = row;
                    picker.error.clear();
                }
            }
            if row == 0 {
                let mut state = self.state.write().await;
                if let Some(picker) = state.model_picker.as_mut() {
                    picker.thinking_enabled = !picker.thinking_enabled;
                }
            } else if let Err(e) = self.model_picker_activate().await {
                let mut state = self.state.write().await;
                if let Some(picker) = state.model_picker.as_mut() {
                    picker.error = e.to_string();
                }
            }
        }

        Ok(false)
    }

    async fn handle_import_picker_mouse(&mut self, mouse: MouseEvent) -> Result<bool> {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return Ok(false);
        }

        let (layout, picker) = {
            let state = self.state.read().await;
            let picker = state.import_picker.clone().context("import picker closed")?;
            let layout = picker::picker_layout(state.viewport);
            (layout, picker)
        };

        if let Some(row) = picker::picker_row_at_y(&layout, &picker, mouse.row) {
            {
                let mut state = self.state.write().await;
                if let Some(picker) = state.import_picker.as_mut() {
                    picker.selected = row;
                    picker.error.clear();
                }
            }
            if let Err(e) = self.picker_activate_selection().await {
                let mut state = self.state.write().await;
                if let Some(picker) = state.import_picker.as_mut() {
                    picker.error = e.to_string();
                }
            }
        }

        Ok(false)
    }

    async fn handle_boot_key(&mut self, key: KeyEvent) -> Result<bool> {
        let boot_mode = self.state.read().await.boot_mode;

        match boot_mode {
            BootMode::ApiKeySetup => self.handle_api_key_setup_key(key).await,
            BootMode::Authenticated => self.handle_boot_authenticated_key(key).await,
        }
    }

    async fn handle_boot_authenticated_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Enter => {
                {
                    let mut state = self.state.write().await;
                    state.phase = AppPhase::Dashboard;
                }
                if let Err(e) = self.refresh_models().await {
                    self.push_chat_system(&format!("Could not load models: {e}"))
                        .await;
                }
            }
            KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
            _ => {}
        }
        Ok(false)
    }

    async fn handle_api_key_setup_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            // Esc is not a quit shortcut here — bracketed-paste sequences begin with
            // escape bytes and would otherwise kick the user back to the shell mid-paste.
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Ok(true),
            KeyCode::Backspace => {
                let mut state = self.state.write().await;
                let cursor = state.api_key_cursor;
                if cursor > 0 {
                    state.api_key_cursor = cursor - 1;
                    state.api_key_buffer.remove(cursor - 1);
                }
                state.boot_error.clear();
            }
            KeyCode::Enter | KeyCode::Char('\n') | KeyCode::Char('\r') => {
                self.submit_api_key().await?;
            }
            KeyCode::Char(c) if !c.is_control() && !c.is_whitespace() => {
                let mut state = self.state.write().await;
                let cursor = state.api_key_cursor;
                state.api_key_buffer.insert(cursor, c);
                state.api_key_cursor = cursor + 1;
                state.boot_error.clear();
            }
            _ => {}
        }
        Ok(false)
    }

    async fn submit_api_key(&mut self) -> Result<()> {
        let key_value = {
            let state = self.state.read().await;
            sanitize_api_key_input(&state.api_key_buffer)
        };

        if key_value.is_empty() {
            let mut state = self.state.write().await;
            state.boot_error = "API key cannot be empty.".to_string();
        } else if let Err(e) = self.save_api_key(&key_value).await {
            let mut state = self.state.write().await;
            state.boot_error = e.to_string();
        } else {
            let mut state = self.state.write().await;
            state.boot_mode = BootMode::Authenticated;
            state.api_key_buffer.clear();
            state.api_key_cursor = 0;
            state.boot_error.clear();
            state.push_activity("API key saved to ~/.config/landingpig/config.json");
        }

        Ok(())
    }

    async fn handle_dashboard_key(&mut self, key: KeyEvent) -> Result<bool> {
        let is_processing = self.state.read().await.phase == AppPhase::Processing;
        if is_processing {
            if is_stop_generation_key(&key) {
                self.generation_cancel().store(true, std::sync::atomic::Ordering::SeqCst);
            }
            return Ok(false);
        }

        match key.code {
            KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => return Ok(true),
            KeyCode::Esc => return Ok(true),
            KeyCode::BackTab => {
                self.toggle_thinking().await;
            }
            KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.toggle_thinking().await;
            }
            KeyCode::Tab => {
                let (suggestion, input) = {
                    let state = self.state.read().await;
                    (
                        state.completion_suggestions.first().cloned(),
                        state.input_buffer.clone(),
                    )
                };

                if let Some(suggestion) = suggestion {
                    let mut state = self.state.write().await;
                    if let Some((cmd, _)) = input.rsplit_once(' ') {
                        state.input_buffer = format!("{cmd} {suggestion}");
                    } else {
                        state.input_buffer = format!("{input} {suggestion}");
                    }
                    state.input_cursor = state.input_buffer.len();
                    drop(state);
                    self.update_completions().await;
                }
            }
            KeyCode::Up => {
                let mut state = self.state.write().await;
                if state.command_history.is_empty() {
                    return Ok(false);
                }
                let new_index = match state.history_index {
                    None => state.command_history.len() - 1,
                    Some(i) => i.saturating_sub(1),
                };
                state.history_index = Some(new_index);
                state.input_buffer = state.command_history[new_index].clone();
                state.input_cursor = state.input_buffer.len();
            }
            KeyCode::Down => {
                let mut state = self.state.write().await;
                if let Some(i) = state.history_index {
                    if i + 1 < state.command_history.len() {
                        let new_index = i + 1;
                        state.history_index = Some(new_index);
                        state.input_buffer = state.command_history[new_index].clone();
                    } else {
                        state.history_index = None;
                        state.input_buffer.clear();
                    }
                    state.input_cursor = state.input_buffer.len();
                }
            }
            KeyCode::Left => {
                let mut state = self.state.write().await;
                state.input_focused = true;
                state.input_cursor = state.input_cursor.saturating_sub(1);
            }
            KeyCode::Right => {
                let mut state = self.state.write().await;
                state.input_focused = true;
                let len = state.input_buffer.len();
                if state.input_cursor < len {
                    state.input_cursor += 1;
                }
            }
            KeyCode::Backspace => {
                let mut state = self.state.write().await;
                state.input_focused = true;
                let cursor = state.input_cursor;
                if cursor > 0 {
                    state.input_cursor = cursor - 1;
                    state.input_buffer.remove(cursor - 1);
                }
                drop(state);
                self.update_completions().await;
            }
            KeyCode::Enter => {
                let command = {
                    let mut state = self.state.write().await;
                    let cmd = state.input_buffer.trim().to_string();
                    if !cmd.is_empty() {
                        state.command_history.push(cmd.clone());
                        state.history_index = None;
                    }
                    state.input_buffer.clear();
                    state.input_cursor = 0;
                    state.completion_suggestions.clear();
                    cmd
                };
                if !command.is_empty() {
                    if let Err(e) = self.execute_command(&command).await {
                        {
                            let mut state = self.state.write().await;
                            if state.phase == AppPhase::Processing {
                                state.phase = AppPhase::Dashboard;
                                state.status_message.clear();
                            }
                        }
                        self.push_chat_system(&format_anyhow(&e)).await;
                    }
                }
            }
            KeyCode::Char(c) => {
                let mut state = self.state.write().await;
                state.input_focused = true;
                let cursor = state.input_cursor;
                state.input_buffer.insert(cursor, c);
                state.input_cursor = cursor + 1;
                drop(state);
                self.update_completions().await;
            }
            _ => {}
        }

        Ok(false)
    }

    async fn toggle_thinking(&mut self) {
        let enabled = {
            let state = self.state.read().await;
            state.thinking_mode == ThinkingMode::Off
        };
        self.apply_thinking(enabled).await;
        let label = if enabled { "ON" } else { "OFF" };
        let mut state = self.state.write().await;
        state.push_activity(format!("Thinking mode {label}"));
    }

    async fn update_completions(&self) {
        let input = self.state.read().await.input_buffer.clone();
        let mut suggestions = Vec::new();

        if let Some(rest) = input.strip_prefix("/model ") {
            let models = self.state.read().await.available_models.clone();
            suggestions = models
                .iter()
                .map(|m| m.id.clone())
                .filter(|id| id.starts_with(rest))
                .collect();
        } else if input == "/import" || input.starts_with("/import ") {
            let prefix = input.strip_prefix("/import").unwrap_or("").trim();
            suggestions = self.files.complete_paths(prefix);
        } else if input == "/read" || input.starts_with("/read ") {
            let prefix = input.strip_prefix("/read").unwrap_or("").trim();
            suggestions = self.files.complete_paths(prefix);
        }

        let mut state = self.state.write().await;
        state.completion_suggestions = suggestions;
    }
}

fn input_area_rect(viewport: Rect) -> Rect {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(55),
            Constraint::Length(1),
            Constraint::Min(4),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(viewport);
    root[3]
}

fn ensure_tty() -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!(
            "landingpig requires an interactive terminal (TTY).\n\
             Run directly from a terminal: cargo run"
        );
    }
    Ok(())
}

/// Strip whitespace and control characters accidentally included when pasting.
fn sanitize_api_key_input(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect()
}
