//! User-facing API error formatting.

use serde::Deserialize;

pub const BALANCE_TOO_LOW_MSG: &str =
    "Your balance is too low to perform this command. Please buy AI credits from your API key provider";

#[derive(Debug, Deserialize)]
struct ApiErrorEnvelope {
    error: Option<ApiErrorBody>,
}

#[derive(Debug, Deserialize)]
struct ApiErrorBody {
    message: Option<String>,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    error_type: Option<String>,
}

pub fn is_balance_error(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("balance")
        || lower.contains("credit")
        || lower.contains("billing")
        || lower.contains("insufficient")
        || lower.contains("too low")
        || lower.contains("purchase")
}

pub fn format_http_error(status: u16, body: &str) -> String {
    if status == 400 && is_balance_error(body) {
        return BALANCE_TOO_LOW_MSG.to_string();
    }

    if let Ok(parsed) = serde_json::from_str::<ApiErrorEnvelope>(body) {
        if let Some(err) = parsed.error {
            if let Some(msg) = &err.message {
                if status == 400 && is_balance_error(msg) {
                    return BALANCE_TOO_LOW_MSG.to_string();
                }
                return format!("Request failed ({status}): {msg}");
            }
        }
    }

    let snippet: String = body.chars().take(120).collect();
    if snippet.is_empty() {
        format!("Request failed with status {status}.")
    } else {
        format!("Request failed ({status}): {snippet}")
    }
}

pub fn format_anyhow(err: &anyhow::Error) -> String {
    let text = err.to_string();
    if is_balance_error(&text) {
        return BALANCE_TOO_LOW_MSG.to_string();
    }
    text.lines().next().unwrap_or(&text).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balance_error_uses_friendly_message() {
        let body = r#"{"error":{"type":"invalid_request_error","message":"Your credit balance is too low"}}"#;
        assert_eq!(
            format_http_error(400, body),
            BALANCE_TOO_LOW_MSG
        );
    }
}
