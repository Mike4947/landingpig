//! Application orchestration — config, API, filesystem, and command dispatch.

use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::api::{
    AnthropicClient, DESIGN_SYSTEM_PROMPT, Message, ModelInfo, REDESIGN_SYSTEM_PROMPT,
    SYSTEM_PROMPT,
};
use crate::config::Config;
use crate::fs::{FileManager, PickerEntryKind, list_picker_entries, picker_start_dir};
use crate::state::{AppPhase, ImportPickerState, ModelPickerState, SharedState, ThinkingMode, new_shared_state};

pub struct App {
    pub config: Config,
    pub state: SharedState,
    pub files: FileManager,
    client: Option<AnthropicClient>,
    conversation: Vec<Message>,
    generation_cancel: Arc<AtomicBool>,
}

impl App {
    pub fn new() -> Result<Self> {
        let config = Config::load()?;
        let workspace = config
            .workspace
            .clone()
            .or_else(|| env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));

        let authenticated = config.is_authenticated();
        let thinking_enabled = config.thinking_enabled;
        let state = new_shared_state(
            config.model.clone(),
            workspace.clone(),
            authenticated,
            thinking_enabled,
        );
        let files = FileManager::new(workspace);

        let client = config.api_key().map(|key| {
            AnthropicClient::new(
                key,
                config.model.clone(),
                config.max_tokens,
                config.thinking_enabled,
                config.thinking_budget_tokens,
            )
        });

        Ok(Self {
            config,
            state,
            files,
            client,
            conversation: Vec::new(),
            generation_cancel: Arc::new(AtomicBool::new(false)),
        })
    }

    pub(crate) fn generation_cancel(&self) -> Arc<AtomicBool> {
        self.generation_cancel.clone()
    }

    fn reset_generation_cancel(&self) {
        self.generation_cancel.store(false, Ordering::SeqCst);
    }

    fn poll_generation_stop(&self) {
        while event::poll(Duration::ZERO).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if is_stop_generation_key(&key) {
                    self.generation_cancel.store(true, Ordering::SeqCst);
                    return;
                }
            }
        }
    }

    async fn finish_generation(&self, cancelled: bool, accumulated: &str) {
        let mut state = self.state.write().await;
        state.phase = AppPhase::Dashboard;
        state.status_message.clear();
        if cancelled {
            if accumulated.is_empty() {
                state.push_chat("[system] Generation stopped.".to_string());
            } else {
                state.push_chat(format!(
                    "[system] Generation stopped.\n[assistant]\n{accumulated}"
                ));
            }
            state.push_activity("Generation stopped by user");
        }
    }

    fn sync_client(&mut self) {
        self.client = self.config.api_key().map(|key| {
            AnthropicClient::new(
                key,
                self.config.model.clone(),
                self.config.max_tokens,
                self.config.thinking_enabled,
                self.config.thinking_budget_tokens,
            )
        });
    }

    pub(crate) async fn apply_thinking(&mut self, enabled: bool) {
        self.config.thinking_enabled = enabled;
        let _ = self.config.save();
        self.sync_client();
        let mut state = self.state.write().await;
        state.thinking_mode = if enabled {
            ThinkingMode::On
        } else {
            ThinkingMode::Off
        };
    }

    pub async fn save_api_key(&mut self, key: &str) -> Result<()> {
        let trimmed = key.trim();
        if trimmed.len() < 10 {
            bail!("API key looks too short — paste your full Anthropic key");
        }
        if !trimmed.starts_with("sk-ant-") {
            bail!("API key should start with sk-ant- — check your paste");
        }

        self.config.anthropic_api_key = Some(trimmed.to_string());
        self.config.save()?;
        self.sync_client();

        if let Err(e) = self.refresh_models().await {
            let mut state = self.state.write().await;
            state.push_activity(format!("Model fetch failed: {e}"));
        }

        Ok(())
    }

    pub async fn refresh_models(&mut self) -> Result<Vec<ModelInfo>> {
        let client = self
            .client
            .as_ref()
            .context("not authenticated — set ANTHROPIC_API_KEY")?;

        let models = client.list_models().await?;
        {
            let mut state = self.state.write().await;
            state.available_models = models.clone();
            state.push_activity(format!("Loaded {} models from Anthropic", models.len()));
        }
        Ok(models)
    }

    async fn set_model(&mut self, model_id: &str) -> Result<()> {
        if self.state.read().await.available_models.is_empty() {
            self.refresh_models().await?;
        }

        let known = {
            let state = self.state.read().await;
            state
                .available_models
                .iter()
                .any(|m| m.id == model_id)
        };

        if !known {
            self.refresh_models().await?;
            let state = self.state.read().await;
            if !state.available_models.iter().any(|m| m.id == model_id) {
                bail!("Unknown model '{model_id}'. Use /model to open the model picker.");
            }
        }

        self.config.model = model_id.to_string();
        self.config.save()?;
        self.sync_client();

        {
            let mut state = self.state.write().await;
            state.model = model_id.to_string();
            state.push_activity(format!("Switched model to {model_id}"));
        }

        Ok(())
    }

    pub async fn execute_command(&mut self, command: &str) -> Result<()> {
        {
            let mut state = self.state.write().await;
            state.push_chat(format!("> {command}"));
        }

        let parts: Vec<&str> = command.splitn(2, ' ').collect();
        let cmd = parts[0];

        match cmd {
            "/import" => self.cmd_import(parts.get(1).copied()).await?,
            "/read" => self.cmd_read(parts.get(1).copied()).await?,
            "/write" => self.cmd_write(command).await?,
            "/redesign" => self.cmd_redesign(parts.get(1).copied()).await?,
            "/design" => self.cmd_design(parts.get(1).copied()).await?,
            "/model" => self.cmd_model(parts.get(1).copied()).await?,
            "/help" => self.cmd_help().await?,
            _ => {
                if cmd.starts_with('/') {
                    self.push_chat_system(&format!("Unknown command: {cmd}. Type /help for commands."))
                        .await;
                } else {
                    self.cmd_chat(command).await?;
                }
            }
        }

        Ok(())
    }

    pub async fn open_model_picker(&mut self) -> Result<()> {
        let models = self.refresh_models().await?;
        if models.is_empty() {
            bail!("No models returned by Anthropic API.");
        }

        let (active_model, thinking_enabled) = {
            let state = self.state.read().await;
            (state.model.clone(), self.config.thinking_enabled)
        };

        let mut selected = 1usize;
        for (i, model) in models.iter().enumerate() {
            if model.id == active_model {
                selected = i + 1;
                break;
            }
        }

        {
            let mut state = self.state.write().await;
            state.model_picker = Some(ModelPickerState {
                models,
                selected,
                scroll: 0,
                thinking_enabled,
                active_model,
                error: String::new(),
            });
            state.push_activity("Opened model picker");
        }
        Ok(())
    }

    pub async fn close_model_picker(&mut self) {
        let mut state = self.state.write().await;
        state.model_picker = None;
    }

    pub async fn model_picker_activate(&mut self) -> Result<()> {
        let (selected, thinking_enabled, model_id) = {
            let state = self.state.read().await;
            let picker = state
                .model_picker
                .as_ref()
                .context("model picker is not open")?;

            if picker.selected == 0 {
                (0, picker.thinking_enabled, String::new())
            } else {
                let model = picker
                    .models
                    .get(picker.selected - 1)
                    .context("invalid model selection")?;
                (
                    picker.selected,
                    picker.thinking_enabled,
                    model.id.clone(),
                )
            }
        };

        if selected == 0 {
            let mut state = self.state.write().await;
            if let Some(picker) = state.model_picker.as_mut() {
                picker.thinking_enabled = !picker.thinking_enabled;
            }
            return Ok(());
        }

        self.apply_thinking(thinking_enabled).await;
        self.set_model(&model_id).await?;
        self.close_model_picker().await;

        let thinking = if thinking_enabled { "ON" } else { "OFF" };
        self.push_chat_system(&format!(
            "Model set to {model_id} (Reasoning: {thinking})"
        ))
        .await;

        Ok(())
    }

    async fn cmd_import(&mut self, path: Option<&str>) -> Result<()> {
        let path = path.map(str::trim).filter(|p| !p.is_empty());

        if path.is_none() {
            self.open_import_picker().await?;
            return Ok(());
        }

        self.import_workspace_path(PathBuf::from(path.unwrap())).await
    }

    pub async fn open_import_picker(&mut self) -> Result<()> {
        let start = {
            let workspace = self.state.read().await.workspace.clone();
            picker_start_dir(&workspace)
        };

        let entries = list_picker_entries(&start)?;
        {
            let mut state = self.state.write().await;
            state.import_picker = Some(ImportPickerState {
                current_dir: start,
                entries,
                selected: 0,
                scroll: 0,
                error: String::new(),
            });
            state.push_activity("Opened workspace picker");
        }
        Ok(())
    }

    pub async fn close_import_picker(&mut self) {
        let mut state = self.state.write().await;
        state.import_picker = None;
    }

    pub async fn picker_activate_selection(&mut self) -> Result<()> {
        let (kind, path) = {
            let state = self.state.read().await;
            let picker = state
                .import_picker
                .as_ref()
                .context("import picker is not open")?;
            let entry = picker
                .entries
                .get(picker.selected)
                .context("invalid picker selection")?;
            (entry.kind, entry.path.clone())
        };

        match kind {
            PickerEntryKind::ImportHere => {
                let path = path;
                self.close_import_picker().await;
                self.import_workspace_path(path).await?;
            }
            PickerEntryKind::Parent | PickerEntryKind::Directory | PickerEntryKind::Drive => {
                self.picker_navigate_to(&path).await?;
            }
        }

        Ok(())
    }

    async fn picker_navigate_to(&mut self, dir: &PathBuf) -> Result<()> {
        let entries = list_picker_entries(dir)?;
        let mut state = self.state.write().await;
        let picker = state
            .import_picker
            .as_mut()
            .context("import picker is not open")?;
        picker.current_dir = dir.canonicalize().unwrap_or_else(|_| dir.clone());
        picker.entries = entries;
        picker.selected = 0;
        picker.scroll = 0;
        picker.error.clear();
        Ok(())
    }

    async fn import_workspace_path(&mut self, path: PathBuf) -> Result<()> {
        let resolved = path
            .canonicalize()
            .with_context(|| format!("cannot access {}", path.display()))?;

        self.config.workspace = Some(resolved.clone());
        self.config.save()?;
        self.files = FileManager::new(resolved.clone());

        let graph = self.files.import(resolved.as_path())?;

        if graph.files.is_empty() {
            bail!(
                "no importable files found at '{}' — pick a folder with HTML, CSS, TSX, or JS files",
                resolved.display()
            );
        }

        {
            let mut state = self.state.write().await;
            state.workspace = resolved.clone();
            for file in &graph.files {
                state
                    .file_context
                    .insert(file.path.clone(), file.clone());
            }
            let summary = graph.summary();
            state.push_activity(format!("Imported {summary}"));
            state.push_chat(format!(
                "[system] Workspace set to {}\nImported {summary}",
                resolved.display()
            ));
        }

        Ok(())
    }

    async fn cmd_read(&mut self, path: Option<&str>) -> Result<()> {
        let path = path.context("usage: /read <file>")?;
        let content = self.files.read(PathBuf::from(path).as_path())?;
        let preview: String = content.chars().take(500).collect();
        let suffix = if content.len() > 500 { "\n..." } else { "" };
        self.push_chat_system(&format!("```\n{preview}{suffix}\n```"))
            .await;
        Ok(())
    }

    async fn cmd_write(&mut self, command: &str) -> Result<()> {
        // /write <path> — content comes from last assistant message (simplified)
        let rest = command.strip_prefix("/write ").unwrap_or("");
        let path = rest
            .split_whitespace()
            .next()
            .context("usage: /write <file> (after /redesign output)")?;

        let content = self
            .conversation
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();

        if content.is_empty() {
            bail!("no agent output to write — run /redesign or /design first");
        }

        self.files
            .write(PathBuf::from(path).as_path(), &content)?;

        {
            let mut state = self.state.write().await;
            state.push_activity(format!("Updated {path}"));
            state.push_chat(format!("[system] Wrote {path}"));
        }

        Ok(())
    }

    async fn cmd_redesign(&mut self, prompt: Option<&str>) -> Result<()> {
        self.config
            .api_key()
            .context("not authenticated — set ANTHROPIC_API_KEY")?;

        let user_prompt = prompt.unwrap_or(
            "Analyze the imported components and propose a conversion-optimized redesign.",
        );

        let context_block = self.build_context_block().await;
        if context_block.is_empty() {
            self.push_chat_system(
                "Tip: run /import first so the AI can read your existing landing page files.",
            )
            .await;
        }

        let full_prompt = if context_block.is_empty() {
            user_prompt.to_string()
        } else {
            format!("{context_block}\n\n---\n\n{user_prompt}")
        };

        self.run_streaming_job(
            REDESIGN_SYSTEM_PROMPT,
            full_prompt,
            "Re-engineering landing page components",
            "Redesign generation complete",
        )
        .await
    }

    async fn cmd_design(&mut self, prompt: Option<&str>) -> Result<()> {
        self.config
            .api_key()
            .context("not authenticated — set ANTHROPIC_API_KEY")?;

        let Some(user_brief) = prompt.map(str::trim).filter(|s| !s.is_empty()) else {
            bail!(
                "/design requires your instructions.\n\
                 Usage: /design <brief>\n\
                 Example: /design SaaS landing for a devtools CLI — dark theme, hero, pricing, FAQ"
            );
        };

        if user_brief.len() < 20 {
            bail!(
                "Design brief too short — describe product, audience, visual style, and required sections."
            );
        }

        let context_block = self.build_context_block().await;
        let full_prompt = if context_block.is_empty() {
            self.push_chat_system(
                "Tip: run /import so the AI can match your codebase stack and conventions.",
            )
            .await;
            format!(
                "# Design Brief (mandatory)\n{user_brief}\n\n\
                 # Codebase\n\
                 No files imported yet — infer stack from the workspace and create a new landing page that fits this repo."
            )
        } else {
            format!(
                "{context_block}\n\n---\n\n\
                 # Design Brief (mandatory)\n{user_brief}\n\n\
                 Design a new landing page from scratch using the workspace stack and conventions above."
            )
        };

        self.run_streaming_job(
            DESIGN_SYSTEM_PROMPT,
            full_prompt,
            "Designing landing page from scratch",
            "Design generation complete",
        )
        .await
    }

    async fn run_streaming_job(
        &mut self,
        system_prompt: &'static str,
        user_content: String,
        start_activity: &str,
        done_activity: &str,
    ) -> Result<()> {
        self.conversation.push(Message {
            role: "user".to_string(),
            content: user_content,
        });

        self.reset_generation_cancel();
        {
            let mut state = self.state.write().await;
            state.token_stats.clear_live_usage();
            state.phase = AppPhase::Processing;
            state.push_activity(start_activity);
        }

        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let (status_tx, mut status_rx) = mpsc::unbounded_channel::<String>();
        let (usage_tx, mut usage_rx) = mpsc::unbounded_channel();
        let messages = self.conversation.clone();
        let system = system_prompt.to_string();
        let client_clone = AnthropicClient::new(
            self.config.api_key().unwrap(),
            self.config.model.clone(),
            self.config.max_tokens,
            self.config.thinking_enabled,
            self.config.thinking_budget_tokens,
        );

        let stream_handle = tokio::spawn(async move {
            client_clone
                .stream_message(messages, &system, tx, Some(status_tx), Some(usage_tx))
                .await
        });

        let mut accumulated = String::new();
        let mut last_usage = None;
        loop {
            if self.generation_cancel.load(Ordering::Relaxed) {
                break;
            }

            tokio::select! {
                chunk = rx.recv() => {
                    match chunk {
                        Some(text) => {
                            accumulated.push_str(&text);
                            while let Ok(status) = status_rx.try_recv() {
                                let mut state = self.state.write().await;
                                state.status_message = status;
                            }
                            while let Ok(u) = usage_rx.try_recv() {
                                last_usage = Some(u.clone());
                                let mut state = self.state.write().await;
                                state.token_stats.set_live_usage(u.input_tokens, u.output_tokens);
                            }
                            let preview = accumulated.chars().take(80).collect::<String>();
                            let mut state = self.state.write().await;
                            state.status_message = preview;
                        }
                        None => break,
                    }
                }
                _ = sleep(Duration::from_millis(40)) => {
                    while let Ok(u) = usage_rx.try_recv() {
                        last_usage = Some(u.clone());
                        let mut state = self.state.write().await;
                        state.token_stats.set_live_usage(u.input_tokens, u.output_tokens);
                    }
                    self.poll_generation_stop();
                }
            }
        }

        let cancelled = self.generation_cancel.load(Ordering::Relaxed);
        stream_handle.abort();

        if cancelled {
            if let Some(u) = last_usage {
                let mut state = self.state.write().await;
                state.token_stats.commit_usage(u.input_tokens, u.output_tokens);
            } else {
                let mut state = self.state.write().await;
                state.token_stats.clear_live_usage();
            }
            self.finish_generation(true, &accumulated).await;
            return Ok(());
        }

        let usage = match stream_handle.await {
            Ok(Ok(u)) => u.or(last_usage),
            Ok(Err(e)) => {
                if let Some(u) = last_usage {
                    let mut state = self.state.write().await;
                    state.token_stats.commit_usage(u.input_tokens, u.output_tokens);
                }
                self.finish_generation(false, &accumulated).await;
                return Err(e);
            }
            Err(_) => last_usage,
        };

        self.conversation.push(Message {
            role: "assistant".to_string(),
            content: accumulated.clone(),
        });

        {
            let mut state = self.state.write().await;
            state.phase = AppPhase::Dashboard;
            state.status_message.clear();
            state.push_chat(format!("[assistant]\n{accumulated}"));
            state.push_activity(done_activity);
            if let Some(u) = usage {
                state.token_stats.commit_usage(u.input_tokens, u.output_tokens);
            } else {
                state.token_stats.clear_live_usage();
            }
        }

        Ok(())
    }

    async fn cmd_chat(&mut self, message: &str) -> Result<()> {
        self.config
            .api_key()
            .context("not authenticated — set ANTHROPIC_API_KEY")?;

        let context_block = self.build_context_block().await;
        let full_message = if context_block.is_empty() {
            message.to_string()
        } else {
            format!("{context_block}\n\n---\n\n{message}")
        };

        self.conversation.push(Message {
            role: "user".to_string(),
            content: full_message,
        });

        self.reset_generation_cancel();
        {
            let mut state = self.state.write().await;
            state.token_stats.clear_live_usage();
            state.phase = AppPhase::Processing;
        }

        let messages = self.conversation.clone();
        let client = AnthropicClient::new(
            self.config.api_key().unwrap(),
            self.config.model.clone(),
            self.config.max_tokens,
            self.config.thinking_enabled,
            self.config.thinking_budget_tokens,
        );
        let cancel = self.generation_cancel.clone();
        let task =
            tokio::spawn(async move { client.send_message(messages, SYSTEM_PROMPT).await });

        loop {
            if cancel.load(Ordering::Relaxed) {
                task.abort();
                break;
            }
            if task.is_finished() {
                break;
            }
            self.poll_generation_stop();
            sleep(Duration::from_millis(40)).await;
        }

        if cancel.load(Ordering::Relaxed) {
            self.finish_generation(true, "").await;
            return Ok(());
        }

        let (text, usage) = task.await.context("generation task panicked")??;

        self.conversation.push(Message {
            role: "assistant".to_string(),
            content: text.clone(),
        });

        {
            let mut state = self.state.write().await;
            state.phase = AppPhase::Dashboard;
            state.status_message.clear();
            state.push_chat(format!("[assistant]\n{text}"));
            if let Some(u) = usage {
                state.token_stats.commit_usage(u.input_tokens, u.output_tokens);
            } else {
                state.token_stats.clear_live_usage();
            }
        }

        Ok(())
    }

    async fn cmd_model(&mut self, name: Option<&str>) -> Result<()> {
        let Some(name) = name else {
            return self.open_model_picker().await;
        };

        if let Ok(index) = name.parse::<usize>() {
            if index == 0 {
                bail!("Model index starts at 1 — use /model to open the picker");
            }
            if self.state.read().await.available_models.is_empty() {
                self.refresh_models().await?;
            }
            let model_id = {
                let state = self.state.read().await;
                state
                    .available_models
                    .get(index - 1)
                    .map(|m| m.id.clone())
                    .with_context(|| format!("No model at index {index} — use /model to open the picker"))?
            };
            self.set_model(&model_id).await?;
            self.push_chat_system(&format!("Model set to {model_id}")).await;
            return Ok(());
        }

        self.set_model(name).await?;
        self.push_chat_system(&format!("Model set to {name}")).await;
        Ok(())
    }

    async fn cmd_help(&self) -> Result<()> {
        self.push_chat_system(
            "Commands:\n  \
             /import          — open workspace picker\n  \
             /import <path>   — import path directly\n  \
             /design <brief>  — design a new landing page (brief required)\n  \
             /redesign [prompt] — redesign existing landing page\n  \
             /model          — open model picker\n  \
             /model <id>     — switch model directly\n  \
             /read <file>    — read workspace file\n  \
             /write <file>   — write last output\n  \
             /help           — show this help\n  \
             Esc (generating) — stop generation\n  \
             Ctrl+Q / Esc    — quit",
        )
        .await;
        Ok(())
    }

    async fn build_context_block(&self) -> String {
        let state = self.state.read().await;
        if state.file_context.is_empty() {
            return String::new();
        }

        let mut parts = vec!["# Workspace Context".to_string()];
        for (path, ctx) in &state.file_context {
            parts.push(format!(
                "## {}\n```{}\n{}\n```",
                path.display(),
                ctx.language,
                &ctx.content.chars().take(4000).collect::<String>()
            ));
        }
        parts.join("\n\n")
    }

    pub(crate) async fn push_chat_system(&self, message: &str) {
        let mut state = self.state.write().await;
        state.push_chat(format!("[system] {message}"));
    }
}

pub(crate) fn is_stop_generation_key(key: &KeyEvent) -> bool {
    key.code == KeyCode::Esc
        || (key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL))
}
