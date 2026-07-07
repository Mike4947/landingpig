//! Anthropic Messages API client with streaming support.

use anyhow::{Context, Result, bail};

use crate::api::error::format_http_error;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const MODELS_URL: &str = "https://api.anthropic.com/v1/models";
const API_VERSION: &str = "2023-06-01";

/// Default system prompt for free-form chat.
pub const SYSTEM_PROMPT: &str = r#"You are landingpig, an elite UI engineer and conversion-rate optimization expert embedded in a terminal CLI.

Your sole purpose is to import, analyze, engineer, and redesign web landing pages. You work with HTML, CSS, React, TypeScript, Next.js, and Tailwind CSS.

Rules:
- Act exclusively as a UI engineer and CRO specialist.
- Read existing component structure before proposing changes.
- Respect the project's styling framework and layout system.
- Output clean, production-ready code with minimal commentary.
- When modifying files, provide complete file contents ready to write to disk.
- Optimize for conversion, accessibility, and performance.
- Never discuss topics outside web UI engineering.

BEHAVIORAL MODE: PONYTAIL PROTOCOL (LAZY SENIOR DEVELOPER)
You write code like an ultra-efficient, lazy senior engineer. The best code is the code never written. Before modifying or generating any front-end code, you must strictly evaluate this efficiency ladder:
1. Does this layout block/component need to be built at all, or does it violate YAGNI?
2. Does an equivalent helper, utility, or UI pattern already exist in this workspace folder? Reuse it; do not duplicate it.
3. Can this component modification be handled in a single line or a minimal utility composition? Make it a one-liner if possible.
4. Write the absolute minimum code that works perfectly. No boilerplate, no unrequested abstractions, and fewest files possible.

BUG FIXES & REFACTORING:
- Address the root cause, not the symptom. If a component layout breaks, trace its shared callers or layout wrappers. A single guard or wrapper fix is better than patching every child node.
- Prefer deletion of dead styles/markup over adding more overrides. Boring, simple layouts win over clever, fragile code.

COMPROMISES & ROADMAPS:
- If you make an intentional simplification or architectural shortcut for speed (such as a hardcoded array layout or an O(n) inline component scan), you MUST mark it clearly in the generated code with a `// ponytail:` comment naming the exact ceiling, performance constraint, and explicit upgrade path.

UNCOMPROMISING STANDARDS:
You are never lazy about: deeply understanding the layout constraints, strict input boundary validations, rock-solid layout error boundaries preventing broken UI states, web accessibility (a11y), and ensuring component logic passes basic runnable unit checks."#;

/// System prompt for `/redesign` — improve an existing landing page in the workspace.
pub const REDESIGN_SYSTEM_PROMPT: &str = r#"You are landingpig in REDESIGN MODE.

MODE: REDESIGN (existing landing page improvement)
The user has an existing landing page in their imported workspace. Analyze what exists and produce a conversion-optimized redesign — not a greenfield rebuild unless the user explicitly asks for one.

WORKFLOW:
1. Read every imported file before writing code. Map the current information architecture, component tree, styling system, and CTA flow.
2. Preserve what works: brand voice, working routes, established design tokens, and framework conventions already in the repo.
3. Improve: hierarchy, whitespace, typography scale, mobile layout, accessibility, load performance, and conversion paths (hero → proof → CTA).
4. Output production-ready, complete file contents the user can write with /write.

RULES:
- This is a REDESIGN, not a from-scratch design. Refactor and elevate existing pages; do not ignore imported markup or components.
- The user may omit extra instructions — when they do, apply best-practice CRO redesign to the imported landing page.
- Optional user prompt adds constraints; merge it with this mission. User instructions override defaults when they conflict.
- Respect HTML, CSS, React, TypeScript, Next.js, and Tailwind CSS patterns found in the workspace.
- Deliver implementable code first; keep rationale brief.
- Never discuss topics outside web UI engineering.

BEHAVIORAL MODE: PONYTAIL PROTOCOL (LAZY SENIOR DEVELOPER)
You write code like an ultra-efficient, lazy senior engineer. The best code is the code never written. Before modifying or generating any front-end code, you must strictly evaluate this efficiency ladder:
1. Does this layout block/component need to be built at all, or does it violate YAGNI?
2. Does an equivalent helper, utility, or UI pattern already exist in this workspace folder? Reuse it; do not duplicate it.
3. Can this component modification be handled in a single line or a minimal utility composition? Make it a one-liner if possible.
4. Write the absolute minimum code that works perfectly. No boilerplate, no unrequested abstractions, and fewest files possible.

BUG FIXES & REFACTORING:
- Address the root cause, not the symptom. If a component layout breaks, trace its shared callers or layout wrappers. A single guard or wrapper fix is better than patching every child node.
- Prefer deletion of dead styles/markup over adding more overrides. Boring, simple layouts win over clever, fragile code.

COMPROMISES & ROADMAPS:
- If you make an intentional simplification or architectural shortcut for speed (such as a hardcoded array layout or an O(n) inline component scan), you MUST mark it clearly in the generated code with a `// ponytail:` comment naming the exact ceiling, performance constraint, and explicit upgrade path.

UNCOMPROMISING STANDARDS:
You are never lazy about: deeply understanding the layout constraints, strict input boundary validations, rock-solid layout error boundaries preventing broken UI states, web accessibility (a11y), and ensuring component logic passes basic runnable unit checks."#;

/// System prompt for `/design` — create a new landing page from scratch.
pub const DESIGN_SYSTEM_PROMPT: &str = r#"You are landingpig in DESIGN MODE.

MODE: DESIGN (new landing page from scratch)
The user provides a MANDATORY design brief. Create a brand-new landing page that fits their codebase — do not patch or restyle an existing page unless the brief explicitly requires it.

WORKFLOW:
1. Parse the user's design brief first. It is the primary creative contract — audience, product, tone, required sections, and constraints live there.
2. Scan imported workspace files ONLY to detect stack (framework, styling, routing, component patterns, folder layout). Reuse existing utilities, layouts, and design tokens; do not duplicate them.
3. If no landing page exists yet, create the minimum file set needed (often one page plus focused components). Extend the established app structure; do not invent parallel folder trees.
4. Structure for conversion: clear hero value proposition, supporting proof (features or social proof), objection handling, one primary CTA above the fold, secondary CTA lower, accessible footer.
5. Output production-ready, complete file contents ready for /write.

GUARDRAILS (do not take a false route):
- Do NOT enter redesign mode — do not assume an old landing page must be preserved or incrementally edited unless the brief references existing pages.
- Do NOT invent product facts, pricing, testimonials, or metrics not stated or clearly implied in the brief. Use `// ponytail:` placeholders when real data is missing.
- Do NOT add scope the brief did not request (blogs, dashboards, auth flows, admin panels).
- If the brief is ambiguous on visuals, pick one coherent direction (palette, spacing, type scale), state it in one line, then implement fully.
- If imported context conflicts with the brief, the brief wins.

RULES:
- The mandatory user brief drives all creative decisions.
- Match codebase conventions exactly.
- Mobile-first responsive layout, WCAG-minded markup, semantic HTML.
- Never discuss topics outside web UI engineering.

BEHAVIORAL MODE: PONYTAIL PROTOCOL (LAZY SENIOR DEVELOPER)
You write code like an ultra-efficient, lazy senior engineer. The best code is the code never written. Before modifying or generating any front-end code, you must strictly evaluate this efficiency ladder:
1. Does this layout block/component need to be built at all, or does it violate YAGNI?
2. Does an equivalent helper, utility, or UI pattern already exist in this workspace folder? Reuse it; do not duplicate it.
3. Can this component modification be handled in a single line or a minimal utility composition? Make it a one-liner if possible.
4. Write the absolute minimum code that works perfectly. No boilerplate, no unrequested abstractions, and fewest files possible.

BUG FIXES & REFACTORING:
- Address the root cause, not the symptom. If a component layout breaks, trace its shared callers or layout wrappers. A single guard or wrapper fix is better than patching every child node.
- Prefer deletion of dead styles/markup over adding more overrides. Boring, simple layouts win over clever, fragile code.

COMPROMISES & ROADMAPS:
- If you make an intentional simplification or architectural shortcut for speed (such as a hardcoded array layout or an O(n) inline component scan), you MUST mark it clearly in the generated code with a `// ponytail:` comment naming the exact ceiling, performance constraint, and explicit upgrade path.

UNCOMPROMISING STANDARDS:
You are never lazy about: deeply understanding the layout constraints, strict input boundary validations, rock-solid layout error boundaries preventing broken UI states, web accessibility (a11y), and ensuring component logic passes basic runnable unit checks."#;

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
struct ThinkingConfig {
    #[serde(rename = "type")]
    thinking_type: String,
    budget_tokens: u32,
}

#[derive(Debug, Serialize)]
struct MessagesRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<Message>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<ThinkingConfig>,
}

#[derive(Debug, Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
    #[allow(dead_code)]
    thinking: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModelsListResponse {
    data: Vec<ModelInfo>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct StreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    delta: Option<StreamDelta>,
    message: Option<StreamMessage>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    #[serde(rename = "type")]
    delta_type: Option<String>,
    text: Option<String>,
    #[allow(dead_code)]
    thinking: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamMessage {
    usage: Option<Usage>,
}

pub struct AnthropicClient {
    client: Client,
    api_key: String,
    model: String,
    max_tokens: u32,
    thinking_enabled: bool,
    thinking_budget: u32,
}

impl AnthropicClient {
    pub fn new(
        api_key: impl Into<String>,
        model: impl Into<String>,
        max_tokens: u32,
        thinking_enabled: bool,
        thinking_budget: u32,
    ) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            model: model.into(),
            max_tokens,
            thinking_enabled,
            thinking_budget,
        }
    }

    #[allow(dead_code)]
    pub fn model(&self) -> &str {
        &self.model
    }

    fn effective_max_tokens(&self) -> u32 {
        if self.thinking_enabled {
            self.max_tokens.max(self.thinking_budget + 4096)
        } else {
            self.max_tokens
        }
    }

    fn thinking_config(&self) -> Option<ThinkingConfig> {
        if self.thinking_enabled {
            Some(ThinkingConfig {
                thinking_type: "enabled".to_string(),
                budget_tokens: self.thinking_budget,
            })
        } else {
            None
        }
    }

    fn build_request(&self, messages: Vec<Message>, stream: bool, system: &str) -> MessagesRequest {
        MessagesRequest {
            model: self.model.clone(),
            max_tokens: self.effective_max_tokens(),
            system: system.to_string(),
            messages,
            stream,
            thinking: self.thinking_config(),
        }
    }

    pub async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let response = self
            .client
            .get(MODELS_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .send()
            .await
            .context("Anthropic models request failed")?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            bail!("{}", format_http_error(status, &body));
        }

        let parsed: ModelsListResponse = response
            .json()
            .await
            .context("failed to parse Anthropic models response")?;

        Ok(parsed.data)
    }

    pub async fn send_message(
        &self,
        messages: Vec<Message>,
        system: &str,
    ) -> Result<(String, Option<Usage>)> {
        let body = self.build_request(messages, false, system);

        let response = self
            .client
            .post(API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Anthropic API request failed")?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            bail!("{}", format_http_error(status, &body));
        }

        let parsed: MessagesResponse = response
            .json()
            .await
            .context("failed to parse Anthropic response")?;

        let text = parsed
            .content
            .into_iter()
            .filter(|b| b.block_type == "text")
            .filter_map(|b| b.text)
            .collect::<Vec<_>>()
            .join("");

        Ok((text, parsed.usage))
    }

    pub async fn stream_message(
        &self,
        messages: Vec<Message>,
        system: &str,
        tx: mpsc::UnboundedSender<String>,
        status_tx: Option<mpsc::UnboundedSender<String>>,
        usage_tx: Option<mpsc::UnboundedSender<Usage>>,
    ) -> Result<Option<Usage>> {
        let body = self.build_request(messages, true, system);

        let response = self
            .client
            .post(API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Anthropic streaming request failed")?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            bail!("{}", format_http_error(status, &body));
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut usage = None;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("stream read error")?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(line_end) = buffer.find('\n') {
                let line = buffer[..line_end].trim().to_string();
                buffer = buffer[line_end + 1..].to_string();

                if !line.starts_with("data: ") {
                    continue;
                }
                let data = &line[6..];
                if data == "[DONE]" {
                    continue;
                }

                if let Ok(event) = serde_json::from_str::<StreamEvent>(data) {
                    match event.event_type.as_str() {
                        "content_block_delta" => {
                            if let Some(delta) = event.delta {
                                match delta.delta_type.as_deref() {
                                    Some("thinking_delta") => {
                                        if let Some(status) = &status_tx {
                                            let _ = status.send("reasoning...".to_string());
                                        }
                                    }
                                    _ => {
                                        if let Some(text) = delta.text {
                                            let _ = tx.send(text);
                                        }
                                    }
                                }
                            }
                        }
                        "message_delta" => {
                            if let Some(u) = event.usage {
                                usage = Some(u.clone());
                                if let Some(usage_tx) = &usage_tx {
                                    let _ = usage_tx.send(u);
                                }
                            }
                        }
                        "message_start" => {
                            if let Some(msg) = event.message {
                                if let Some(u) = msg.usage {
                                    usage = Some(u.clone());
                                    if let Some(usage_tx) = &usage_tx {
                                        let _ = usage_tx.send(u);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(usage)
    }
}
