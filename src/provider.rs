// SPDX-License-Identifier: MPL-2.0

use crate::chat::{AppSettings, ChatMessage, ChatRole, ChatSession, ProviderKind};
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRequest {
    pub provider: ProviderKind,
    pub endpoint: String,
    pub api_key: Option<String>,
    pub model: String,
    pub messages: Vec<ProviderMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub enum ProviderEvent {
    Delta { chat_id: u64, delta: String },
    Finished { chat_id: u64 },
    Failed { chat_id: u64, error: String },
}

#[derive(Debug, Serialize)]
struct ChatCompletionsRequest<'a> {
    model: &'a str,
    messages: &'a [ProviderMessage],
    stream: bool,
}

pub fn build_request(
    settings: &AppSettings,
    session_openrouter_key: Option<&str>,
    chat: &ChatSession,
) -> Result<ProviderRequest, String> {
    let model = settings.active_model().trim().to_string();
    if model.is_empty() {
        return Err("Set a model in Provider settings first.".into());
    }

    let mut messages = Vec::new();
    let system_prompt = settings.system_prompt.trim();
    if !system_prompt.is_empty() {
        messages.push(ProviderMessage {
            role: "system".into(),
            content: system_prompt.into(),
        });
    }

    let tail = context_tail(&chat.messages, settings.context_message_limit);
    for message in tail {
        if message.content.trim().is_empty() {
            continue;
        }

        messages.push(ProviderMessage {
            role: role_name(message.role).into(),
            content: message.content.clone(),
        });
    }

    if messages.is_empty() {
        return Err("Nothing to send yet.".into());
    }

    let (endpoint, api_key) = match settings.provider {
        ProviderKind::OpenRouter => {
            let key = session_openrouter_key
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| "OpenRouter API key is missing in settings.".to_string())?;
            (
                "https://openrouter.ai/api/v1/chat/completions".into(),
                Some(key),
            )
        }
        ProviderKind::LmStudio => (lmstudio_chat_endpoint(&settings.lmstudio_base_url), None),
    };

    Ok(ProviderRequest {
        provider: settings.provider,
        endpoint,
        api_key,
        model,
        messages,
    })
}

pub async fn stream_chat(
    client: Client,
    chat_id: u64,
    request: ProviderRequest,
    tx: UnboundedSender<ProviderEvent>,
) {
    match stream_chat_inner(client, chat_id, &request, &tx).await {
        Ok(()) => {
            let _ = tx.send(ProviderEvent::Finished { chat_id });
        }
        Err(error) => {
            let _ = tx.send(ProviderEvent::Failed { chat_id, error });
        }
    }
}

async fn stream_chat_inner(
    client: Client,
    chat_id: u64,
    request: &ProviderRequest,
    tx: &UnboundedSender<ProviderEvent>,
) -> Result<(), String> {
    let mut builder = client
        .post(&request.endpoint)
        .json(&ChatCompletionsRequest {
            model: &request.model,
            messages: &request.messages,
            stream: true,
        });

    if let Some(api_key) = &request.api_key {
        builder = builder.bearer_auth(api_key);
    }

    if request.provider == ProviderKind::OpenRouter {
        builder = builder
            .header(
                "HTTP-Referer",
                "https://github.com/pop-os/cosmic-applet-template",
            )
            .header("X-Title", "Cosmic AI Panel");
    }

    let response = builder
        .send()
        .await
        .map_err(|error| format!("Request failed: {error}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let message = extract_error_message(&body)
            .unwrap_or_else(|| format!("Provider request failed with status {status}."));
        return Err(message);
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("Stream error: {error}"))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(event_end) = buffer.find("\n\n") {
            let event = buffer[..event_end].to_string();
            buffer.drain(..event_end + 2);

            if handle_sse_event(chat_id, &event, tx)? {
                return Ok(());
            }
        }
    }

    if !buffer.trim().is_empty() {
        let _ = handle_sse_event(chat_id, &buffer, tx)?;
    }

    Ok(())
}

fn handle_sse_event(
    chat_id: u64,
    event: &str,
    tx: &UnboundedSender<ProviderEvent>,
) -> Result<bool, String> {
    for line in event.lines() {
        let line = line.trim();
        if !line.starts_with("data:") {
            continue;
        }

        let payload = line.trim_start_matches("data:").trim();
        if payload.is_empty() {
            continue;
        }
        if payload == "[DONE]" {
            return Ok(true);
        }

        let value: Value = serde_json::from_str(payload)
            .map_err(|error| format!("Invalid stream event: {error}"))?;

        if let Some(error_message) = value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
        {
            return Err(error_message.to_string());
        }

        if let Some(delta) = value
            .pointer("/choices/0/delta/content")
            .and_then(Value::as_str)
            .filter(|delta| !delta.is_empty())
        {
            let _ = tx.send(ProviderEvent::Delta {
                chat_id,
                delta: delta.to_string(),
            });
        }
    }

    Ok(false)
}

fn extract_error_message(body: &str) -> Option<String> {
    let json: Value = serde_json::from_str(body).ok()?;
    json.get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            json.get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

fn context_tail(messages: &[ChatMessage], limit: usize) -> &[ChatMessage] {
    if limit == 0 || messages.len() <= limit {
        messages
    } else {
        &messages[messages.len() - limit..]
    }
}

fn lmstudio_chat_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/chat/completions")
    } else {
        format!("{trimmed}/v1/chat/completions")
    }
}

fn role_name(role: ChatRole) -> &'static str {
    match role {
        ChatRole::System => "system",
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
    }
}
