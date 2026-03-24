// SPDX-License-Identifier: MPL-2.0

use crate::chat::{AppSettings, ChatMessage, ChatRole, ChatSession, ProviderKind};
use crate::runtime::context::builder::build_context;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

const EMPTY_RESPONSE_ERROR: &str = "Empty response from provider";
const PROVIDER_TIMEOUT_ERROR: &str = "Provider timeout";
const MALFORMED_RESPONSE_ERROR: &str = "Malformed response from provider";

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderReliability {
    pub timeout_seconds: u64,
    pub retry_attempts: u8,
    pub retry_delay_seconds: u64,
}

impl ProviderReliability {
    pub fn timeout(self) -> Duration {
        Duration::from_secs(self.timeout_seconds.max(1))
    }

    pub fn retry_delay(self) -> Duration {
        Duration::from_secs(self.retry_delay_seconds)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRequest {
    pub provider: ProviderKind,
    pub endpoint: String,
    pub api_key: Option<String>,
    pub model: String,
    pub messages: Vec<ProviderMessage>,
    pub reliability: ProviderReliability,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub enum ProviderEvent {
    Delta {
        request_id: u64,
        chat_id: u64,
        delta: String,
    },
    Finished {
        request_id: u64,
        chat_id: u64,
    },
    Failed {
        request_id: u64,
        chat_id: u64,
        error: String,
    },
}

#[derive(Debug, Serialize)]
struct ChatCompletionsRequest<'a> {
    model: &'a str,
    messages: &'a [ProviderMessage],
    stream: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum FailureKind {
    RetryableNetwork,
    Fatal,
}

#[derive(Debug, Clone)]
struct ProviderFailure {
    message: String,
    kind: FailureKind,
}

impl ProviderFailure {
    fn fatal(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: FailureKind::Fatal,
        }
    }

    fn retryable(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: FailureKind::RetryableNetwork,
        }
    }
}

pub fn build_request(
    settings: &AppSettings,
    session_openrouter_key: Option<&str>,
    chat: &ChatSession,
) -> Result<ProviderRequest, String> {
    let model = chat.model.trim().to_string();
    if model.is_empty() {
        return Err("Set a model in Provider settings first.".into());
    }

    let context = build_context(
        &settings.base_system_prompt,
        &settings.profile,
        &settings.memory,
        context_tail(&chat.messages, settings.context_message_limit),
    );

    let mut messages = Vec::new();
    if let Some(system_prompt) = context.system_prompt {
        messages.push(ProviderMessage {
            role: "system".into(),
            content: system_prompt,
        });
    }

    for message in context.history {
        messages.push(ProviderMessage {
            role: role_name(message.role).into(),
            content: message.content,
        });
    }

    if messages.is_empty() {
        return Err("Nothing to send yet.".into());
    }

    let (endpoint, api_key) = match chat.provider {
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
        ProviderKind::LmStudio => (
            lmstudio_chat_endpoint(&settings.provider.lmstudio_base_url),
            None,
        ),
    };

    Ok(ProviderRequest {
        provider: chat.provider,
        endpoint,
        api_key,
        model,
        messages,
        reliability: ProviderReliability {
            timeout_seconds: settings.provider.timeout_seconds,
            retry_attempts: settings.provider.retry_attempts,
            retry_delay_seconds: settings.provider.retry_delay_seconds,
        },
    })
}

pub async fn stream_chat(
    client: Client,
    request_id: u64,
    chat_id: u64,
    request: ProviderRequest,
    tx: UnboundedSender<ProviderEvent>,
) {
    let max_attempts = usize::from(request.reliability.retry_attempts) + 1;

    for attempt in 0..max_attempts {
        match stream_chat_attempt(client.clone(), request_id, chat_id, &request, &tx).await {
            Ok(()) => {
                let _ = tx.send(ProviderEvent::Finished {
                    request_id,
                    chat_id,
                });
                return;
            }
            Err(error)
                if error.kind == FailureKind::RetryableNetwork && attempt + 1 < max_attempts =>
            {
                log_provider_warning(format!(
                    "retrying provider request (attempt {}/{max_attempts}, chat_id={chat_id}, request_id={request_id}): {}",
                    attempt + 2,
                    error.message
                ));
                tokio::time::sleep(request.reliability.retry_delay()).await;
            }
            Err(error) => {
                let _ = tx.send(ProviderEvent::Failed {
                    request_id,
                    chat_id,
                    error: error.message,
                });
                return;
            }
        }
    }
}

pub async fn test_connection(
    client: Client,
    provider: ProviderKind,
    endpoint_or_base_url: String,
    api_key: Option<String>,
    reliability: ProviderReliability,
) -> Result<(), String> {
    let (endpoint, api_key) = match provider {
        ProviderKind::OpenRouter => {
            let api_key = api_key
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "OpenRouter API key is missing.".to_string())?;

            (
                "https://openrouter.ai/api/v1/models".to_string(),
                Some(api_key),
            )
        }
        ProviderKind::LmStudio => (lmstudio_models_endpoint(&endpoint_or_base_url), None),
    };

    let mut last_failure = None;
    let max_attempts = usize::from(reliability.retry_attempts) + 1;

    for attempt in 0..max_attempts {
        let mut request = client.get(&endpoint);
        if let Some(api_key) = &api_key {
            request = request.bearer_auth(api_key);
        }

        match send_with_timeout(request, reliability.timeout()).await {
            Ok(response) => {
                if response.status().is_success() {
                    return Ok(());
                }

                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                let message = extract_error_message(&body)
                    .unwrap_or_else(|| format!("Connection failed ({status})"));
                return Err(message);
            }
            Err(error)
                if error.kind == FailureKind::RetryableNetwork && attempt + 1 < max_attempts =>
            {
                last_failure = Some(error.message);
                tokio::time::sleep(reliability.retry_delay()).await;
            }
            Err(error) => return Err(error.message),
        }
    }

    Err(last_failure.unwrap_or_else(|| "Connection failed".to_string()))
}

async fn stream_chat_attempt(
    client: Client,
    request_id: u64,
    chat_id: u64,
    request: &ProviderRequest,
    tx: &UnboundedSender<ProviderEvent>,
) -> Result<(), ProviderFailure> {
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
            .header("HTTP-Referer", "https://github.com/levlon/cosmic-ai-panel")
            .header("X-Title", "Cosmic AI Panel");
    }

    let response = send_with_timeout(builder, request.reliability.timeout())
        .await
        .inspect_err(|error| {
            if error.kind == FailureKind::RetryableNetwork {
                log_provider_warning(format!(
                    "provider start error while waiting for response headers (chat_id={chat_id}, request_id={request_id}): {}",
                    error.message
                ));
            }
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let message = extract_error_message(&body)
            .unwrap_or_else(|| format!("Provider request failed with status {status}."));
        return Err(ProviderFailure::fatal(message));
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut has_assistant_text = false;

    match tokio::time::timeout(request.reliability.timeout(), stream.next()).await {
        Ok(Some(chunk)) => {
            let chunk = chunk.map_err(|error| stream_chunk_failure(error, true))?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            if process_stream_buffer(
                request_id,
                chat_id,
                &mut buffer,
                tx,
                &mut has_assistant_text,
            )
            .map_err(ProviderFailure::fatal)?
            {
                return finish_stream(has_assistant_text);
            }
        }
        Ok(None) => return Err(ProviderFailure::fatal(EMPTY_RESPONSE_ERROR)),
        Err(_) => {
            log_provider_warning(format!(
                "stream start timeout while waiting for first chunk (chat_id={chat_id}, request_id={request_id})"
            ));
            return Err(ProviderFailure::retryable(PROVIDER_TIMEOUT_ERROR));
        }
    }

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| stream_chunk_failure(error, !has_assistant_text))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        if process_stream_buffer(
            request_id,
            chat_id,
            &mut buffer,
            tx,
            &mut has_assistant_text,
        )
        .map_err(ProviderFailure::fatal)?
        {
            return finish_stream(has_assistant_text);
        }
    }

    if !buffer.trim().is_empty() {
        let outcome = handle_sse_event(request_id, chat_id, &buffer, tx, !has_assistant_text)
            .map_err(ProviderFailure::fatal)?;
        if outcome.emitted_text {
            has_assistant_text = true;
        }
    }

    finish_stream(has_assistant_text)
}

async fn send_with_timeout(
    request: reqwest::RequestBuilder,
    timeout: Duration,
) -> Result<reqwest::Response, ProviderFailure> {
    let pending = request.send();
    match tokio::time::timeout(timeout, pending).await {
        Ok(Ok(response)) => Ok(response),
        Ok(Err(error)) => {
            if is_retryable_network_error(&error) {
                Err(ProviderFailure::retryable(format!(
                    "Request failed: {error}"
                )))
            } else {
                Err(ProviderFailure::fatal(format!("Request failed: {error}")))
            }
        }
        Err(_) => Err(ProviderFailure::retryable(PROVIDER_TIMEOUT_ERROR)),
    }
}

fn stream_chunk_failure(error: reqwest::Error, allow_retry: bool) -> ProviderFailure {
    if allow_retry && is_retryable_network_error(&error) {
        ProviderFailure::retryable(format!("Stream error: {error}"))
    } else {
        ProviderFailure::fatal(format!("Stream error: {error}"))
    }
}

fn is_retryable_network_error(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect() || error.is_request()
}

#[derive(Debug, Default, Clone, Copy)]
struct SseEventOutcome {
    finished: bool,
    emitted_text: bool,
}

fn process_stream_buffer(
    request_id: u64,
    chat_id: u64,
    buffer: &mut String,
    tx: &UnboundedSender<ProviderEvent>,
    has_assistant_text: &mut bool,
) -> Result<bool, String> {
    while let Some((event_end, delimiter_len)) = find_event_boundary(buffer) {
        let event = buffer[..event_end].to_string();
        buffer.drain(..event_end + delimiter_len);

        let outcome = handle_sse_event(request_id, chat_id, &event, tx, !*has_assistant_text)?;
        if outcome.emitted_text {
            *has_assistant_text = true;
        }
        if outcome.finished {
            return Ok(true);
        }
    }

    Ok(false)
}

fn handle_sse_event(
    request_id: u64,
    chat_id: u64,
    event: &str,
    tx: &UnboundedSender<ProviderEvent>,
    allow_message_fallback: bool,
) -> Result<SseEventOutcome, String> {
    let payloads = event_payloads(event);
    let mut outcome = SseEventOutcome::default();

    for payload in payloads {
        if payload == "[DONE]" {
            outcome.finished = true;
            break;
        }

        let value: Value = serde_json::from_str(&payload).map_err(|error| {
            log_provider_warning(format!(
                "malformed stream payload (chat_id={chat_id}, request_id={request_id}): {error}; payload={payload}"
            ));
            MALFORMED_RESPONSE_ERROR.to_string()
        })?;

        if let Some(error_message) = value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
        {
            return Err(error_message.to_string());
        }

        if let Some(content) =
            extract_stream_text(&value, allow_message_fallback && !outcome.emitted_text)
        {
            let _ = tx.send(ProviderEvent::Delta {
                request_id,
                chat_id,
                delta: content,
            });
            outcome.emitted_text = true;
        }
    }

    Ok(outcome)
}

fn extract_stream_text(value: &Value, allow_message_fallback: bool) -> Option<String> {
    if let Some(content) = extract_string(value.pointer("/choices/0/delta/content")) {
        return Some(content);
    }

    if allow_message_fallback
        && let Some(content) = extract_string(value.pointer("/choices/0/message/content"))
    {
        return Some(content);
    }

    None
}

fn extract_string(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).and_then(|content| {
        if content.is_empty() {
            None
        } else {
            Some(content.to_string())
        }
    })
}

fn event_payloads(event: &str) -> Vec<String> {
    let data_lines: Vec<String> = event
        .lines()
        .filter_map(|line| {
            let line = line.strip_suffix('\r').unwrap_or(line);
            let payload = line.strip_prefix("data:")?;
            Some(payload.strip_prefix(' ').unwrap_or(payload).to_string())
        })
        .filter(|payload| !payload.is_empty())
        .collect();

    if !data_lines.is_empty() {
        return vec![data_lines.join("\n")];
    }

    let trimmed = event.trim();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return vec![trimmed.to_string()];
    }

    Vec::new()
}

fn find_event_boundary(buffer: &str) -> Option<(usize, usize)> {
    let lf_boundary = buffer.find("\n\n").map(|index| (index, 2));
    let crlf_boundary = buffer.find("\r\n\r\n").map(|index| (index, 4));

    match (lf_boundary, crlf_boundary) {
        (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
        (Some(boundary), None) | (None, Some(boundary)) => Some(boundary),
        (None, None) => None,
    }
}

fn finish_stream(has_assistant_text: bool) -> Result<(), ProviderFailure> {
    if has_assistant_text {
        Ok(())
    } else {
        Err(ProviderFailure::fatal(EMPTY_RESPONSE_ERROR))
    }
}

fn log_provider_warning(message: impl AsRef<str>) {
    eprintln!("[cosmic-ai-panel][provider][warn] {}", message.as_ref());
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

fn lmstudio_models_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/models") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/models")
    } else {
        format!("{trimmed}/v1/models")
    }
}

fn role_name(role: ChatRole) -> &'static str {
    match role {
        ChatRole::System => "system",
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FailureKind, ProviderEvent, ProviderReliability, build_request, extract_stream_text,
        handle_sse_event,
    };
    use crate::chat::{AppSettings, ChatMessage, ChatRole, ChatSession, ProviderKind, UserProfile};
    use serde_json::json;
    use tokio::sync::mpsc::unbounded_channel;

    #[test]
    fn build_request_injects_profile_and_memory_context() {
        let mut settings = AppSettings::default();
        settings.profile = UserProfile {
            name: Some("Lev".into()),
            language: Some("English".into()),
            response_style: Some("Concise".into()),
        };
        settings.memory = vec!["User uses COSMIC desktop".into()];
        settings.provider.default_model = Some(crate::chat::SavedModel::new(
            ProviderKind::OpenRouter,
            "openrouter/model",
        ));
        settings.provider.saved_models = settings
            .provider
            .default_model
            .clone()
            .into_iter()
            .collect();

        let mut chat = ChatSession::new(1, ProviderKind::OpenRouter, "openrouter/model");
        chat.messages
            .push(ChatMessage::new(1, ChatRole::User, "hello there"));

        let request = build_request(&settings, Some("key"), &chat).unwrap();

        assert_eq!(request.reliability.timeout_seconds, 20);
        assert_eq!(request.messages.len(), 2);
        assert!(request.messages[0].content.contains("User profile:"));
        assert!(request.messages[0].content.contains("Known user memory:"));
    }

    #[test]
    fn extract_stream_text_uses_message_fallback() {
        let payload = json!({
            "choices": [{
                "message": {
                    "content": "hello from fallback"
                }
            }]
        });

        assert_eq!(
            extract_stream_text(&payload, true),
            Some("hello from fallback".to_string())
        );
    }

    #[test]
    fn extract_stream_text_preserves_whitespace() {
        let payload = json!({
            "choices": [{
                "delta": {
                    "content": " hello\nworld "
                }
            }]
        });

        assert_eq!(
            extract_stream_text(&payload, false),
            Some(" hello\nworld ".to_string())
        );
    }

    #[test]
    fn handle_sse_event_accepts_raw_json_payload() {
        let (tx, mut rx) = unbounded_channel();
        let raw_json = r#"{"choices":[{"message":{"content":"raw-json"}}]}"#;

        let outcome = handle_sse_event(7, 9, raw_json, &tx, true).unwrap();

        assert!(outcome.emitted_text);
        assert!(!outcome.finished);
        match rx.try_recv().unwrap() {
            ProviderEvent::Delta {
                request_id,
                chat_id,
                delta,
            } => {
                assert_eq!(request_id, 7);
                assert_eq!(chat_id, 9);
                assert_eq!(delta, "raw-json");
            }
            other => panic!("unexpected provider event: {other:?}"),
        }
    }

    #[test]
    fn handle_sse_event_preserves_space_only_delta() {
        let (tx, mut rx) = unbounded_channel();
        let raw_json = r#"data: {"choices":[{"delta":{"content":" "}}]}"#;

        let outcome = handle_sse_event(7, 9, raw_json, &tx, false).unwrap();

        assert!(outcome.emitted_text);
        assert!(!outcome.finished);
        match rx.try_recv().unwrap() {
            ProviderEvent::Delta {
                request_id,
                chat_id,
                delta,
            } => {
                assert_eq!(request_id, 7);
                assert_eq!(chat_id, 9);
                assert_eq!(delta, " ");
            }
            other => panic!("unexpected provider event: {other:?}"),
        }
    }

    #[test]
    fn handle_sse_event_rejects_malformed_json() {
        let (tx, _rx) = unbounded_channel();
        let error = handle_sse_event(1, 2, "data: {not-json}", &tx, true).unwrap_err();

        assert_eq!(error, "Malformed response from provider");
    }

    #[test]
    fn reliability_exposes_timeout_and_retry_delay() {
        let reliability = ProviderReliability {
            timeout_seconds: 11,
            retry_attempts: 3,
            retry_delay_seconds: 4,
        };

        assert_eq!(reliability.timeout().as_secs(), 11);
        assert_eq!(reliability.retry_delay().as_secs(), 4);
        assert_eq!(FailureKind::RetryableNetwork, FailureKind::RetryableNetwork);
    }
}
