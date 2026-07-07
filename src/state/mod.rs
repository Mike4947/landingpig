//! Thread-safe application state.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Local};
use ratatui::layout::Rect;
use tokio::sync::RwLock;

use crate::api::ModelInfo;
use crate::fs::picker::PickerEntry;
use crate::fs::context::FileContext;

#[derive(Debug, Clone)]
pub struct ActivityEntry {
    pub message: String,
    pub timestamp: DateTime<Local>,
}

impl ActivityEntry {
    pub fn relative_label(&self) -> String {
        let now = Local::now();
        let duration = now.signed_duration_since(self.timestamp);
        let secs = duration.num_seconds().max(0);

        let label = if secs < 60 {
            format!("{secs}s ago")
        } else if secs < 3600 {
            format!("{}m ago", secs / 60)
        } else if secs < 86400 {
            format!("{}h ago", secs / 3600)
        } else {
            format!("{}d ago", secs / 86400)
        };

        format!("{label:<8} | {}", self.message)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppPhase {
    Boot,
    Dashboard,
    Processing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootMode {
    ApiKeySetup,
    Authenticated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingMode {
    Off,
    On,
}

#[derive(Debug, Clone)]
pub struct TokenStats {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub live_input: u64,
    pub live_output: u64,
    pub context_window: u64,
}

impl Default for TokenStats {
    fn default() -> Self {
        Self {
            input_tokens: 0,
            output_tokens: 0,
            live_input: 0,
            live_output: 0,
            context_window: 200_000,
        }
    }
}

impl TokenStats {
    pub fn set_live_usage(&mut self, input: u64, output: u64) {
        self.live_input = input;
        self.live_output = output;
    }

    pub fn clear_live_usage(&mut self) {
        self.live_input = 0;
        self.live_output = 0;
    }

    pub fn commit_usage(&mut self, input: u64, output: u64) {
        self.input_tokens += input;
        self.output_tokens += output;
        self.clear_live_usage();
    }

    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens + self.live_input + self.live_output
    }

    pub fn usage_fraction(&self) -> f64 {
        if self.context_window == 0 {
            return 0.0;
        }
        (self.total_tokens() as f64 / self.context_window as f64).clamp(0.0, 1.0)
    }

    pub fn usage_summary(&self) -> String {
        let in_tok = self.input_tokens + self.live_input;
        let out_tok = self.output_tokens + self.live_output;
        let total = in_tok + out_tok;
        format!("{in_tok} in · {out_tok} out · {total} / {}", self.context_window)
    }
}

#[derive(Debug)]
pub struct AppState {
    pub phase: AppPhase,
    pub boot_mode: BootMode,
    pub api_key_buffer: String,
    pub api_key_cursor: usize,
    pub boot_error: String,
    pub thinking_mode: ThinkingMode,
    pub model: String,
    pub available_models: Vec<ModelInfo>,
    pub workspace: PathBuf,
    pub token_stats: TokenStats,
    pub activity_log: Vec<ActivityEntry>,
    pub chat_history: Vec<String>,
    pub input_buffer: String,
    pub input_cursor: usize,
    pub input_focused: bool,
    pub cursor_visible: bool,
    pub blink_tick: u8,
    pub command_history: Vec<String>,
    pub history_index: Option<usize>,
    pub file_context: HashMap<PathBuf, FileContext>,
    pub processing_frame: usize,
    pub status_message: String,
    pub completion_suggestions: Vec<String>,
    pub import_picker: Option<ImportPickerState>,
    pub model_picker: Option<ModelPickerState>,
    pub viewport: Rect,
}

#[derive(Debug, Clone)]
pub struct ModelPickerState {
    pub models: Vec<ModelInfo>,
    pub selected: usize,
    pub scroll: usize,
    pub thinking_enabled: bool,
    pub active_model: String,
    pub error: String,
}

impl ModelPickerState {
    pub fn row_count(&self) -> usize {
        1 + self.models.len()
    }
}

#[derive(Debug, Clone)]
pub struct ImportPickerState {
    pub current_dir: PathBuf,
    pub entries: Vec<PickerEntry>,
    pub selected: usize,
    pub scroll: usize,
    pub error: String,
}

impl AppState {
    pub fn new(model: String, workspace: PathBuf, authenticated: bool) -> Self {
        Self {
            phase: AppPhase::Boot,
            boot_mode: if authenticated {
                BootMode::Authenticated
            } else {
                BootMode::ApiKeySetup
            },
            api_key_buffer: String::new(),
            api_key_cursor: 0,
            boot_error: String::new(),
            thinking_mode: ThinkingMode::Off,
            model,
            available_models: Vec::new(),
            workspace,
            token_stats: TokenStats::default(),
            activity_log: vec![
                ActivityEntry {
                    message: "landingpig CLI initialized".to_string(),
                    timestamp: Local::now(),
                },
                ActivityEntry {
                    message: "Awaiting workspace import".to_string(),
                    timestamp: Local::now(),
                },
            ],
            chat_history: Vec::new(),
            input_buffer: String::new(),
            input_cursor: 0,
            input_focused: true,
            cursor_visible: true,
            blink_tick: 0,
            command_history: Vec::new(),
            history_index: None,
            file_context: HashMap::new(),
            processing_frame: 0,
            status_message: String::new(),
            completion_suggestions: Vec::new(),
            import_picker: None,
            model_picker: None,
            viewport: Rect::default(),
        }
    }

    pub fn push_activity(&mut self, message: impl Into<String>) {
        self.activity_log.insert(
            0,
            ActivityEntry {
                message: message.into(),
                timestamp: Local::now(),
            },
        );
        if self.activity_log.len() > 50 {
            self.activity_log.truncate(50);
        }
    }

    pub fn push_chat(&mut self, line: impl Into<String>) {
        self.chat_history.push(line.into());
        if self.chat_history.len() > 200 {
            let drain = self.chat_history.len() - 200;
            self.chat_history.drain(0..drain);
        }
    }
}

pub type SharedState = Arc<RwLock<AppState>>;

pub fn new_shared_state(
    model: String,
    workspace: PathBuf,
    authenticated: bool,
    thinking_enabled: bool,
) -> SharedState {
    let mut state = AppState::new(model, workspace, authenticated);
    state.thinking_mode = if thinking_enabled {
        ThinkingMode::On
    } else {
        ThinkingMode::Off
    };
    Arc::new(RwLock::new(state))
}
