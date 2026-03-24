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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderSettings {
    pub active_provider: ProviderKind,
    #[serde(default, skip_serializing)]
    pub openrouter_api_key: String,
    pub lmstudio_base_url: String,
    pub saved_models: Vec<SavedModel>,
    pub default_model: Option<SavedModel>,
    pub timeout_seconds: u64,
    pub retry_attempts: u8,
    pub retry_delay_seconds: u64,
    #[serde(default, skip_serializing, alias = "openrouter_model")]
    pub(crate) legacy_openrouter_model: String,
    #[serde(default, skip_serializing, alias = "lmstudio_model")]
    pub(crate) legacy_lmstudio_model: String,
}

impl ProviderSettings {
    pub fn normalize(&mut self) {
        let mut seen = HashSet::new();
        let mut saved_models = Vec::new();

        for model in self.saved_models.drain(..) {
            if let Some(model) = SavedModel::normalized(model.provider, &model.name)
                && seen.insert(model.clone())
            {
                saved_models.push(model);
            }
        }

        if let Some(model) =
            SavedModel::normalized(ProviderKind::OpenRouter, &self.legacy_openrouter_model)
            && seen.insert(model.clone())
        {
            saved_models.push(model);
        }

        if let Some(model) =
            SavedModel::normalized(ProviderKind::LmStudio, &self.legacy_lmstudio_model)
            && seen.insert(model.clone())
        {
            saved_models.push(model);
        }

        self.saved_models = saved_models;

        self.default_model = self
            .default_model
            .take()
            .and_then(|model| SavedModel::normalized(model.provider, &model.name));

        if let Some(default_model) = self.default_model.clone()
            && !self.saved_models.contains(&default_model)
        {
            self.saved_models.push(default_model);
        }

        if self
            .default_model
            .as_ref()
            .is_none_or(|model| model.provider != self.active_provider)
        {
            self.default_model = self
                .saved_models
                .iter()
                .find(|model| model.provider == self.active_provider)
                .cloned()
                .or_else(|| self.saved_models.first().cloned());
        }

        if let Some(default_model) = &self.default_model {
            self.active_provider = default_model.provider;
        }

        self.lmstudio_base_url = self.lmstudio_base_url.trim().to_string();
        self.timeout_seconds = self.timeout_seconds.clamp(5, 120);
        self.retry_attempts = self.retry_attempts.min(5);
        self.retry_delay_seconds = self.retry_delay_seconds.min(60);
    }

    pub fn active_model(&self) -> &str {
        self.default_model
            .as_ref()
            .filter(|model| model.provider == self.active_provider)
            .map(|model| model.name.as_str())
            .unwrap_or("")
    }
}

impl Default for ProviderSettings {
    fn default() -> Self {
        Self {
            active_provider: ProviderKind::OpenRouter,
            openrouter_api_key: String::new(),
            lmstudio_base_url: "http://127.0.0.1:1234/v1".into(),
            saved_models: Vec::new(),
            default_model: None,
            timeout_seconds: 20,
            retry_attempts: 1,
            retry_delay_seconds: 2,
            legacy_openrouter_model: String::new(),
            legacy_lmstudio_model: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreferenceLevel {
    More,
    #[default]
    Default,
    Less,
}

impl PreferenceLevel {
    pub const ALL: [Self; 3] = [Self::More, Self::Default, Self::Less];

    pub fn label(self) -> &'static str {
        match self {
            Self::More => "More",
            Self::Default => "Default",
            Self::Less => "Less",
        }
    }

    pub fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or_default()
    }

    pub fn index(self) -> usize {
        match self {
            Self::More => 0,
            Self::Default => 1,
            Self::Less => 2,
        }
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UserProfile {
    pub name: Option<String>,
    pub language: Option<String>,
    pub occupation: Option<String>,
    pub response_style: Option<String>,
    pub more_about_you: Option<String>,
    pub header_lists: PreferenceLevel,
    pub emoji: PreferenceLevel,
}

impl UserProfile {
    pub fn normalize(&mut self) {
        self.name = normalize_optional_field(self.name.take());
        self.language = normalize_optional_field(self.language.take());
        self.occupation = normalize_optional_field(self.occupation.take());
        self.response_style = normalize_optional_field(self.response_style.take());
        self.more_about_you = normalize_optional_field(self.more_about_you.take());
    }

    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.language.is_none()
            && self.occupation.is_none()
            && self.response_style.is_none()
            && self.more_about_you.is_none()
            && self.header_lists == PreferenceLevel::Default
            && self.emoji == PreferenceLevel::Default
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SkillsSettings {
    pub datetime: bool,
    pub clipboard: bool,
    pub filesystem: bool,
}

impl Default for SkillsSettings {
    fn default() -> Self {
        Self {
            datetime: true,
            clipboard: false,
            filesystem: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub provider: ProviderSettings,
    pub profile: UserProfile,
    pub memory: Vec<String>,
    pub skills: SkillsSettings,
    pub base_system_prompt: String,
    pub context_message_limit: usize,
}

impl AppSettings {
    pub fn normalize(&mut self) {
        self.provider.normalize();
        self.profile.normalize();
        self.memory = self
            .memory
            .drain(..)
            .filter_map(|item| {
                let trimmed = item.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .collect();
    }

    pub fn active_model(&self) -> &str {
        self.provider.active_model()
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            provider: ProviderSettings::default(),
            profile: UserProfile::default(),
            memory: Vec::new(),
            skills: SkillsSettings::default(),
            base_system_prompt: default_base_system_prompt().into(),
            context_message_limit: 10,
        }
    }
}

pub fn default_base_system_prompt() -> &'static str {
    "You are a concise assistant integrated into COSMIC."
}

fn normalize_optional_field(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub fn now_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
