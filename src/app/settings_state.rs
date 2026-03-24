//! Settings-related form state, tabs, validation, and modal UI state.

use super::*;

const SETTINGS_PROVIDER_OPTIONS: [&str; 2] = ["OpenRouter", "LM Studio"];

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
    SystemPrompt,
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
    pub(in crate::app) response_style: String,
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
            response_style: settings.profile.response_style.clone().unwrap_or_default(),
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

    pub(in crate::app) fn reset_personalization(&mut self) {
        let defaults = AppSettings::default();
        self.base_system_prompt = defaults.base_system_prompt;
        self.profile_name.clear();
        self.profile_language.clear();
        self.response_style.clear();
        self.memory_items.clear();
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
        settings.profile.response_style = normalize_form_field(&self.response_style);
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
    pub(in crate::app) system_prompt_content: widget::text_editor::Content,
    pub(in crate::app) system_prompt_editor_id: widget::Id,
    pub(in crate::app) connection_test_state: ConnectionTestState,
}

impl SettingsUiState {
    pub(in crate::app) fn refresh_from_settings(&mut self, settings: &AppSettings) {
        self.active_tab = SettingsTab::Provider;
        self.form = SettingsForm::from_settings(settings);
        self.modal = None;
        self.add_model_provider_index = settings.provider.active_provider.index();
        self.add_model_name.clear();
        self.system_prompt_content =
            widget::text_editor::Content::with_text(&settings.base_system_prompt);
        self.connection_test_state = ConnectionTestState::Idle;
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
            system_prompt_content: widget::text_editor::Content::new(),
            system_prompt_editor_id: widget::Id::unique(),
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
