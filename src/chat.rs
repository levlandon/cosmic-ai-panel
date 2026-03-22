// SPDX-License-Identifier: MPL-2.0

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum ProviderKind {
    #[default]
    OpenRouter,
    LmStudio,
}

impl ProviderKind {
    pub const ALL: [Self; 2] = [Self::OpenRouter, Self::LmStudio];

    pub fn label(self) -> &'static str {
        match self {
            Self::OpenRouter => "OpenRouter",
            Self::LmStudio => "LM Studio",
        }
    }

    pub fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or_default()
    }

    pub fn index(self) -> usize {
        match self {
            Self::OpenRouter => 0,
            Self::LmStudio => 1,
        }
    }
}

impl fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: u64,
    pub role: ChatRole,
    pub content: String,
    pub created_at: u64,
}

impl ChatMessage {
    pub fn new(id: u64, role: ChatRole, content: impl Into<String>) -> Self {
        Self {
            id,
            role,
            content: content.into(),
            created_at: now_timestamp(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: u64,
    pub title: String,
    pub provider: ProviderKind,
    pub model: String,
    pub updated_at: u64,
    pub messages: Vec<ChatMessage>,
}

impl ChatSession {
    pub fn new(id: u64, provider: ProviderKind, model: impl Into<String>) -> Self {
        Self {
            id,
            title: format!("New Chat {id}"),
            provider,
            model: model.into(),
            updated_at: now_timestamp(),
            messages: Vec::new(),
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = now_timestamp();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub provider: ProviderKind,
    #[serde(default, skip_serializing)]
    pub openrouter_api_key: String,
    pub openrouter_model: String,
    pub lmstudio_model: String,
    pub lmstudio_base_url: String,
    pub system_prompt: String,
    pub context_message_limit: usize,
}

impl AppSettings {
    pub fn active_model(&self) -> &str {
        match self.provider {
            ProviderKind::OpenRouter => self.openrouter_model.as_str(),
            ProviderKind::LmStudio => self.lmstudio_model.as_str(),
        }
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            provider: ProviderKind::OpenRouter,
            openrouter_api_key: String::new(),
            openrouter_model: String::new(),
            lmstudio_model: String::new(),
            lmstudio_base_url: "http://127.0.0.1:1234/v1".into(),
            system_prompt: "You are a concise assistant integrated into COSMIC.".into(),
            context_message_limit: 10,
        }
    }
}

pub fn now_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
