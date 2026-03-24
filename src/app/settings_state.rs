//! Settings-related form state and modal UI state.

use super::*;

const SETTINGS_PROVIDER_OPTIONS: [&str; 2] = ["OpenRouter", "LM Studio"];

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

#[derive(Debug, Clone, Default)]
pub(in crate::app) struct SettingsForm {
    pub(in crate::app) provider_index: usize,
    pub(in crate::app) openrouter_api_key: String,
    pub(in crate::app) lmstudio_base_url: String,
    pub(in crate::app) saved_models: Vec<SavedModel>,
    pub(in crate::app) default_model: Option<SavedModel>,
    pub(in crate::app) system_prompt: String,
    pub(in crate::app) context_message_limit: String,
}

impl SettingsForm {
    pub(in crate::app) fn from_settings(settings: &AppSettings) -> Self {
        Self {
            provider_index: settings.provider.index(),
            openrouter_api_key: settings.openrouter_api_key.clone(),
            lmstudio_base_url: settings.lmstudio_base_url.clone(),
            saved_models: settings.saved_models.clone(),
            default_model: settings.default_model.clone(),
            system_prompt: settings.system_prompt.clone(),
            context_message_limit: settings.context_message_limit.to_string(),
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

    pub(in crate::app) fn apply_to_settings(&self, settings: &mut AppSettings) -> bool {
        settings.provider = self.provider();
        settings.openrouter_api_key = self.openrouter_api_key.trim().to_string();
        settings.lmstudio_base_url = self.lmstudio_base_url.trim().to_string();
        settings.saved_models = self.saved_models.clone();
        settings.default_model = self.default_model.clone();
        settings.system_prompt = self.system_prompt.clone();

        let parsed_context_limit = self.context_message_limit.trim().parse::<usize>().ok();

        if let Some(limit) = parsed_context_limit {
            settings.context_message_limit = limit;
        }

        settings.normalize();
        parsed_context_limit.is_some()
    }
}

#[derive(Debug, Clone)]
pub(in crate::app) struct SettingsUiState {
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
        self.form = SettingsForm::from_settings(settings);
        self.modal = None;
        self.add_model_provider_index = settings.provider.index();
        self.add_model_name.clear();
        self.system_prompt_content = widget::text_editor::Content::with_text(&settings.system_prompt);
        self.connection_test_state = ConnectionTestState::Idle;
    }
}

impl Default for SettingsUiState {
    fn default() -> Self {
        Self {
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
