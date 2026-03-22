// SPDX-License-Identifier: MPL-2.0

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct SavedModel {
    pub provider: ProviderKind,
    pub name: String,
}

impl SavedModel {
    pub fn new(provider: ProviderKind, name: impl Into<String>) -> Self {
        Self {
            provider,
            name: name.into(),
        }
    }

    pub fn normalized(provider: ProviderKind, name: &str) -> Option<Self> {
        let name = name.trim();
        if name.is_empty() {
            None
        } else {
            Some(Self::new(provider, name))
        }
    }

    pub fn dropdown_label(&self) -> String {
        match self.provider {
            ProviderKind::OpenRouter => format!("OpenRouter · {}", self.name),
            ProviderKind::LmStudio => format!("LM Studio · {}", self.name),
        }
    }

    pub fn chat_dropdown_label(&self) -> String {
        let trimmed = self
            .name
            .rsplit('/')
            .next()
            .filter(|segment| !segment.is_empty())
            .unwrap_or(&self.name);
        let truncated = truncate_model_label(trimmed, 20);

        match self.provider {
            ProviderKind::OpenRouter => truncated,
            ProviderKind::LmStudio => format!("{truncated} (local)"),
        }
    }
}

fn truncate_model_label(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();

    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        value.to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub provider: ProviderKind,
    #[serde(default, skip_serializing)]
    pub openrouter_api_key: String,
    pub lmstudio_base_url: String,
    pub saved_models: Vec<SavedModel>,
    pub default_model: Option<SavedModel>,
    pub system_prompt: String,
    pub context_message_limit: usize,
    #[serde(default, skip_serializing, alias = "openrouter_model")]
    legacy_openrouter_model: String,
    #[serde(default, skip_serializing, alias = "lmstudio_model")]
    legacy_lmstudio_model: String,
}

impl AppSettings {
    pub fn normalize(&mut self) {
        let mut seen = HashSet::new();
        let mut saved_models = Vec::new();

        for model in self.saved_models.drain(..) {
            if let Some(model) = SavedModel::normalized(model.provider, &model.name) {
                if seen.insert(model.clone()) {
                    saved_models.push(model);
                }
            }
        }

        if let Some(model) =
            SavedModel::normalized(ProviderKind::OpenRouter, &self.legacy_openrouter_model)
        {
            if seen.insert(model.clone()) {
                saved_models.push(model);
            }
        }

        if let Some(model) =
            SavedModel::normalized(ProviderKind::LmStudio, &self.legacy_lmstudio_model)
        {
            if seen.insert(model.clone()) {
                saved_models.push(model);
            }
        }

        self.saved_models = saved_models;

        self.default_model = self
            .default_model
            .take()
            .and_then(|model| SavedModel::normalized(model.provider, &model.name));

        if let Some(default_model) = self.default_model.clone() {
            if !self.saved_models.contains(&default_model) {
                self.saved_models.push(default_model);
            }
        }

        if self
            .default_model
            .as_ref()
            .is_none_or(|model| model.provider != self.provider)
        {
            self.default_model = self
                .saved_models
                .iter()
                .find(|model| model.provider == self.provider)
                .cloned()
                .or_else(|| self.saved_models.first().cloned());
        }

        if let Some(default_model) = &self.default_model {
            self.provider = default_model.provider;
        }
    }

    pub fn active_model(&self) -> &str {
        self.default_model
            .as_ref()
            .filter(|model| model.provider == self.provider)
            .map(|model| model.name.as_str())
            .unwrap_or("")
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            provider: ProviderKind::OpenRouter,
            openrouter_api_key: String::new(),
            lmstudio_base_url: "http://127.0.0.1:1234/v1".into(),
            saved_models: Vec::new(),
            default_model: None,
            system_prompt: "You are a concise assistant integrated into COSMIC.".into(),
            context_message_limit: 10,
            legacy_openrouter_model: String::new(),
            legacy_lmstudio_model: String::new(),
        }
    }
}

pub fn now_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
