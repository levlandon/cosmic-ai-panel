//! Settings-related form state, tabs, validation, and modal UI state.

use super::*;
use crate::chat::{PreferenceLevel, UserProfile};
use crate::personalization::PersonalizationSettings;

const SETTINGS_PROVIDER_OPTIONS: [&str; 2] = ["OpenRouter", "LM Studio"];
const PREFERENCE_LEVEL_OPTIONS: [&str; 3] = ["More", "Default", "Less"];

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub(crate) enum SettingsTab {
    #[default]
    Provider,
    Personalization,
    Skills,
}

impl SettingsTab {
    pub(in crate::app) const ALL: [Self; 3] = [Self::Provider, Self::Personalization, Self::Skills];

    pub(in crate::app) fn label(self) -> &'static str {
        match self {
            Self::Provider => "Provider",
            Self::Personalization => "Personalization",
            Self::Skills => "Skills",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(in crate::app) enum SettingsValidationError {
    ContextLimit,
    TimeoutSeconds,
    RetryAttempts,
    RetryDelaySeconds,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(in crate::app) enum SettingsModal {
    AddModel,
    Editor(TextEditorModal),
    PromptPreview,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(in crate::app) enum TextEditorModal {
    BaseSystemPrompt,
    ResponseStyle,
    MoreAboutYou,
    ImportPersonalization,
    ExportPersonalization,
}

impl TextEditorModal {
    pub(in crate::app) fn title(self) -> &'static str {
        match self {
            Self::BaseSystemPrompt => "Edit base system prompt",
            Self::ResponseStyle => "Edit response style",
            Self::MoreAboutYou => "Edit more about you",
            Self::ImportPersonalization => "Import personalization",
            Self::ExportPersonalization => "Export personalization",
        }
    }

    pub(in crate::app) fn action_label(self) -> Option<&'static str> {
        match self {
            Self::BaseSystemPrompt => Some("Save"),
            Self::ResponseStyle => Some("Save"),
            Self::MoreAboutYou => Some("Save"),
            Self::ImportPersonalization => Some("Import"),
            Self::ExportPersonalization => None,
        }
    }

    pub(in crate::app) fn is_read_only(self) -> bool {
        matches!(self, Self::ExportPersonalization)
    }
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub(crate) enum PromptPreviewMode {
    #[default]
    Code,
    Text,
}

impl PromptPreviewMode {
    pub(in crate::app) const ALL: [Self; 2] = [Self::Code, Self::Text];

    pub(in crate::app) fn label(self) -> &'static str {
        match self {
            Self::Code => "Code",
            Self::Text => "Text",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(in crate::app) enum ConnectionTestState {
    #[default]
    Idle,
    Testing,
    Success,
    Failed(String),
}

#[derive(Debug, Clone)]
pub(in crate::app) struct SettingsForm {
    pub(in crate::app) provider_index: usize,
    pub(in crate::app) openrouter_api_key: String,
    pub(in crate::app) lmstudio_base_url: String,
    pub(in crate::app) saved_models: Vec<SavedModel>,
    pub(in crate::app) default_model: Option<SavedModel>,
    pub(in crate::app) timeout_seconds: String,
    pub(in crate::app) retry_attempts: String,
    pub(in crate::app) retry_delay_seconds: String,
    pub(in crate::app) base_system_prompt: String,
    pub(in crate::app) context_message_limit: String,
    pub(in crate::app) profile_name: String,
    pub(in crate::app) profile_language: String,
    pub(in crate::app) profile_occupation: String,
    pub(in crate::app) response_style: String,
    pub(in crate::app) more_about_you: String,
    pub(in crate::app) header_lists_index: usize,
    pub(in crate::app) emoji_index: usize,
    pub(in crate::app) memory_items: Vec<String>,
    pub(in crate::app) skill_datetime: bool,
    pub(in crate::app) skill_clipboard: bool,
    pub(in crate::app) skill_filesystem: bool,
}

impl SettingsForm {
    pub(in crate::app) fn from_settings(settings: &AppSettings) -> Self {
        Self {
            provider_index: settings.provider.active_provider.index(),
            openrouter_api_key: settings.provider.openrouter_api_key.clone(),
            lmstudio_base_url: settings.provider.lmstudio_base_url.clone(),
            saved_models: settings.provider.saved_models.clone(),
            default_model: settings.provider.default_model.clone(),
            timeout_seconds: settings.provider.timeout_seconds.to_string(),
            retry_attempts: settings.provider.retry_attempts.to_string(),
            retry_delay_seconds: settings.provider.retry_delay_seconds.to_string(),
            base_system_prompt: settings.base_system_prompt.clone(),
            context_message_limit: settings.context_message_limit.to_string(),
            profile_name: settings.profile.name.clone().unwrap_or_default(),
            profile_language: settings.profile.language.clone().unwrap_or_default(),
            profile_occupation: settings.profile.occupation.clone().unwrap_or_default(),
            response_style: settings.profile.response_style.clone().unwrap_or_default(),
            more_about_you: settings.profile.more_about_you.clone().unwrap_or_default(),
            header_lists_index: settings.profile.header_lists.index(),
            emoji_index: settings.profile.emoji.index(),
            memory_items: settings.memory.clone(),
            skill_datetime: settings.skills.datetime,
            skill_clipboard: settings.skills.clipboard,
            skill_filesystem: settings.skills.filesystem,
        }
    }

    pub(in crate::app) fn provider(&self) -> ProviderKind {
        ProviderKind::from_index(self.provider_index)
    }

    pub(in crate::app) fn provider_labels() -> &'static [&'static str; 2] {
        &SETTINGS_PROVIDER_OPTIONS
    }

    pub(in crate::app) fn preference_labels() -> &'static [&'static str; 3] {
        &PREFERENCE_LEVEL_OPTIONS
    }

    pub(in crate::app) fn default_model_options(&self) -> Vec<String> {
        self.saved_models
            .iter()
            .map(SavedModel::dropdown_label)
            .collect()
    }

    pub(in crate::app) fn default_model_index(&self) -> Option<usize> {
        let selected = self.default_model.as_ref()?;
        self.saved_models.iter().position(|model| model == selected)
    }

    pub(in crate::app) fn select_provider(&mut self, provider: ProviderKind) {
        self.provider_index = provider.index();

        if self
            .default_model
            .as_ref()
            .is_none_or(|model| model.provider != provider)
        {
            self.default_model = self
                .saved_models
                .iter()
                .find(|model| model.provider == provider)
                .cloned();
        }
    }

    pub(in crate::app) fn select_default_model(&mut self, index: usize) {
        if let Some(model) = self.saved_models.get(index).cloned() {
            self.provider_index = model.provider.index();
            self.default_model = Some(model);
        }
    }

    pub(in crate::app) fn add_model(&mut self, provider: ProviderKind, name: &str) -> bool {
        let Some(model) = SavedModel::normalized(provider, name) else {
            return false;
        };

        if !self.saved_models.contains(&model) {
            self.saved_models.push(model.clone());
        }

        if self.default_model.is_none() {
            self.default_model = Some(model.clone());
            self.provider_index = model.provider.index();
        }

        true
    }

    pub(in crate::app) fn remove_model(&mut self, index: usize) {
        if index >= self.saved_models.len() {
            return;
        }

        let removed = self.saved_models.remove(index);
        if self.default_model.as_ref() == Some(&removed) {
            self.default_model = self
                .saved_models
                .iter()
                .find(|model| model.provider == self.provider())
                .cloned()
                .or_else(|| self.saved_models.first().cloned());

            if let Some(model) = &self.default_model {
                self.provider_index = model.provider.index();
            }
        }
    }

    pub(in crate::app) fn add_memory_item(&mut self) {
        self.memory_items.push(String::new());
    }

    pub(in crate::app) fn set_memory_item(&mut self, index: usize, value: String) {
        if let Some(item) = self.memory_items.get_mut(index) {
            *item = value;
        }
    }

    pub(in crate::app) fn remove_memory_item(&mut self, index: usize) {
        if index < self.memory_items.len() {
            self.memory_items.remove(index);
        }
    }

    pub(in crate::app) fn select_header_lists(&mut self, index: usize) {
        self.header_lists_index = PreferenceLevel::from_index(index).index();
    }

    pub(in crate::app) fn select_emoji(&mut self, index: usize) {
        self.emoji_index = PreferenceLevel::from_index(index).index();
    }

    pub(in crate::app) fn header_lists(&self) -> PreferenceLevel {
        PreferenceLevel::from_index(self.header_lists_index)
    }

    pub(in crate::app) fn emoji(&self) -> PreferenceLevel {
        PreferenceLevel::from_index(self.emoji_index)
    }

    pub(in crate::app) fn reset_personalization(&mut self) {
        let defaults = AppSettings::default();
        self.base_system_prompt = defaults.base_system_prompt;
        self.profile_name.clear();
        self.profile_language.clear();
        self.profile_occupation.clear();
        self.response_style.clear();
        self.more_about_you.clear();
        self.header_lists_index = PreferenceLevel::Default.index();
        self.emoji_index = PreferenceLevel::Default.index();
        self.memory_items.clear();
    }

    pub(in crate::app) fn personalization_settings(&self) -> PersonalizationSettings {
        let mut settings = PersonalizationSettings {
            base_system_prompt: self.base_system_prompt.clone(),
            profile: UserProfile {
                name: normalize_form_field(&self.profile_name),
                language: normalize_form_field(&self.profile_language),
                occupation: normalize_form_field(&self.profile_occupation),
                response_style: normalize_form_field(&self.response_style),
                more_about_you: normalize_form_field(&self.more_about_you),
                header_lists: self.header_lists(),
                emoji: self.emoji(),
            },
            memory: self.memory_items.clone(),
        };
        settings.normalize();
        settings
    }

    pub(in crate::app) fn apply_personalization(
        &mut self,
        personalization: &PersonalizationSettings,
    ) {
        self.base_system_prompt = personalization.base_system_prompt.clone();
        self.profile_name = personalization.profile.name.clone().unwrap_or_default();
        self.profile_language = personalization.profile.language.clone().unwrap_or_default();
        self.profile_occupation = personalization
            .profile
            .occupation
            .clone()
            .unwrap_or_default();
        self.response_style = personalization
            .profile
            .response_style
            .clone()
            .unwrap_or_default();
        self.more_about_you = personalization
            .profile
            .more_about_you
            .clone()
            .unwrap_or_default();
        self.header_lists_index = personalization.profile.header_lists.index();
        self.emoji_index = personalization.profile.emoji.index();
        self.memory_items = personalization.memory.clone();
    }

    pub(in crate::app) fn apply_to_settings(
        &self,
        settings: &mut AppSettings,
    ) -> Result<(), SettingsValidationError> {
        let parsed_context_limit = self
            .context_message_limit
            .trim()
            .parse::<usize>()
            .map_err(|_| SettingsValidationError::ContextLimit)?;
        let parsed_timeout = self
            .timeout_seconds
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or(SettingsValidationError::TimeoutSeconds)?;
        let parsed_retry_attempts = self
            .retry_attempts
            .trim()
            .parse::<u8>()
            .map_err(|_| SettingsValidationError::RetryAttempts)?;
        let parsed_retry_delay = self
            .retry_delay_seconds
            .trim()
            .parse::<u64>()
            .map_err(|_| SettingsValidationError::RetryDelaySeconds)?;

        settings.provider.active_provider = self.provider();
        settings.provider.openrouter_api_key = self.openrouter_api_key.trim().to_string();
        settings.provider.lmstudio_base_url = self.lmstudio_base_url.trim().to_string();
        settings.provider.saved_models = self.saved_models.clone();
        settings.provider.default_model = self.default_model.clone();
        settings.provider.timeout_seconds = parsed_timeout;
        settings.provider.retry_attempts = parsed_retry_attempts;
        settings.provider.retry_delay_seconds = parsed_retry_delay;
        settings.base_system_prompt = self.base_system_prompt.clone();
        settings.context_message_limit = parsed_context_limit;
        settings.profile.name = normalize_form_field(&self.profile_name);
        settings.profile.language = normalize_form_field(&self.profile_language);
        settings.profile.occupation = normalize_form_field(&self.profile_occupation);
        settings.profile.response_style = normalize_form_field(&self.response_style);
        settings.profile.more_about_you = normalize_form_field(&self.more_about_you);
        settings.profile.header_lists = self.header_lists();
        settings.profile.emoji = self.emoji();
        settings.memory = self.memory_items.clone();
        settings.skills.datetime = self.skill_datetime;
        settings.skills.clipboard = self.skill_clipboard;
        settings.skills.filesystem = self.skill_filesystem;
        settings.normalize();
        Ok(())
    }
}

impl Default for SettingsForm {
    fn default() -> Self {
        Self::from_settings(&AppSettings::default())
    }
}

#[derive(Debug, Clone)]
pub(in crate::app) struct SettingsUiState {
    pub(in crate::app) active_tab: SettingsTab,
    pub(in crate::app) form: SettingsForm,
    pub(in crate::app) modal: Option<SettingsModal>,
    pub(in crate::app) add_model_provider_index: usize,
    pub(in crate::app) add_model_name: String,
    pub(in crate::app) modal_editor_content: widget::text_editor::Content,
    pub(in crate::app) modal_editor_id: widget::Id,
    pub(in crate::app) occupation_content: widget::text_editor::Content,
    pub(in crate::app) occupation_editor_id: widget::Id,
    pub(in crate::app) preview_mode: PromptPreviewMode,
    pub(in crate::app) modal_error: Option<String>,
    pub(in crate::app) connection_test_state: ConnectionTestState,
}

impl SettingsUiState {
    pub(in crate::app) fn refresh_from_settings(&mut self, settings: &AppSettings) {
        self.active_tab = SettingsTab::Provider;
        self.form = SettingsForm::from_settings(settings);
        self.modal = None;
        self.add_model_provider_index = settings.provider.active_provider.index();
        self.add_model_name.clear();
        self.sync_personalization_editors();
        self.modal_editor_content =
            widget::text_editor::Content::with_text(&settings.base_system_prompt);
        self.preview_mode = PromptPreviewMode::Code;
        self.modal_error = None;
        self.connection_test_state = ConnectionTestState::Idle;
    }

    pub(in crate::app) fn sync_personalization_editors(&mut self) {
        self.occupation_content =
            widget::text_editor::Content::with_text(&self.form.profile_occupation);
    }

    pub(in crate::app) fn set_modal_editor_text(&mut self, value: &str) {
        self.modal_editor_content = widget::text_editor::Content::with_text(value);
    }
}

impl Default for SettingsUiState {
    fn default() -> Self {
        Self {
            active_tab: SettingsTab::Provider,
            form: SettingsForm::default(),
            modal: None,
            add_model_provider_index: ProviderKind::OpenRouter.index(),
            add_model_name: String::new(),
            modal_editor_content: widget::text_editor::Content::new(),
            modal_editor_id: widget::Id::unique(),
            occupation_content: widget::text_editor::Content::new(),
            occupation_editor_id: widget::Id::unique(),
            preview_mode: PromptPreviewMode::Code,
            modal_error: None,
            connection_test_state: ConnectionTestState::Idle,
        }
    }
}

fn normalize_form_field(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
