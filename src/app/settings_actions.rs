//! Settings tabs, personalization, model list, and connection test flows.

use super::*;

impl AppModel {
    pub(in crate::app) fn open_settings(&mut self) {
        self.settings_ui.refresh_from_settings(&self.state.settings);
        self.panel_view = PanelView::Settings;
    }

    pub(in crate::app) fn close_settings(&mut self) -> Task<cosmic::Action<Message>> {
        self.settings_ui.modal = None;
        self.panel_view = PanelView::Chat;
        self.scroll_messages_to_end(true)
    }

    pub(in crate::app) fn select_settings_tab(&mut self, tab: SettingsTab) {
        self.settings_ui.active_tab = tab;
    }

    pub(in crate::app) fn select_settings_provider(&mut self, index: usize) {
        self.settings_ui
            .form
            .select_provider(ProviderKind::from_index(index));
        self.settings_ui.connection_test_state = ConnectionTestState::Idle;
    }

    pub(in crate::app) fn set_openrouter_key(&mut self, value: String) {
        self.settings_ui.form.openrouter_api_key = value;
        self.settings_ui.connection_test_state = ConnectionTestState::Idle;
    }

    pub(in crate::app) fn set_lmstudio_url(&mut self, value: String) {
        self.settings_ui.form.lmstudio_base_url = value;
        self.settings_ui.connection_test_state = ConnectionTestState::Idle;
    }

    pub(in crate::app) fn set_provider_timeout(&mut self, value: String) {
        self.settings_ui.form.timeout_seconds = value;
        self.settings_ui.connection_test_state = ConnectionTestState::Idle;
    }

    pub(in crate::app) fn set_provider_retry_attempts(&mut self, value: String) {
        self.settings_ui.form.retry_attempts = value;
        self.settings_ui.connection_test_state = ConnectionTestState::Idle;
    }

    pub(in crate::app) fn set_provider_retry_delay(&mut self, value: String) {
        self.settings_ui.form.retry_delay_seconds = value;
        self.settings_ui.connection_test_state = ConnectionTestState::Idle;
    }

    pub(in crate::app) fn set_context_limit(&mut self, value: String) {
        self.settings_ui.form.context_message_limit = value;
    }

    pub(in crate::app) fn set_profile_name(&mut self, value: String) {
        self.settings_ui.form.profile_name = value;
    }

    pub(in crate::app) fn set_profile_language(&mut self, value: String) {
        self.settings_ui.form.profile_language = value;
    }

    pub(in crate::app) fn set_response_style(&mut self, value: String) {
        self.settings_ui.form.response_style = value;
    }

    pub(in crate::app) fn add_memory_item(&mut self) {
        self.settings_ui.form.add_memory_item();
    }

    pub(in crate::app) fn set_memory_item(&mut self, index: usize, value: String) {
        self.settings_ui.form.set_memory_item(index, value);
    }

    pub(in crate::app) fn remove_memory_item(&mut self, index: usize) {
        self.settings_ui.form.remove_memory_item(index);
    }

    pub(in crate::app) fn toggle_skill_datetime(&mut self, value: bool) {
        self.settings_ui.form.skill_datetime = value;
    }

    pub(in crate::app) fn toggle_skill_clipboard(&mut self, value: bool) {
        self.settings_ui.form.skill_clipboard = value;
    }

    pub(in crate::app) fn toggle_skill_filesystem(&mut self, value: bool) {
        self.settings_ui.form.skill_filesystem = value;
    }

    pub(in crate::app) fn reset_personalization(&mut self) {
        self.settings_ui.form.reset_personalization();
        self.settings_ui.system_prompt_content =
            widget::text_editor::Content::with_text(&self.settings_ui.form.base_system_prompt);
    }

    pub(in crate::app) fn select_default_model(&mut self, index: usize) {
        self.settings_ui.form.select_default_model(index);
    }

    pub(in crate::app) fn select_active_model(&mut self, index: usize) {
        if let Some(model) = self
            .state
            .settings
            .provider
            .saved_models
            .get(index)
            .cloned()
        {
            if let Some(chat) = self.active_chat_mut() {
                chat.provider = model.provider;
                chat.model = model.name;
                chat.touch();
            }
            self.clear_chat_error(self.state.active_chat_id);
            self.persist_state();
        }
    }

    pub(in crate::app) fn open_add_model_modal(&mut self) {
        self.settings_ui.modal = Some(SettingsModal::AddModel);
        self.settings_ui.add_model_provider_index = self.settings_ui.form.provider().index();
        self.settings_ui.add_model_name.clear();
    }

    pub(in crate::app) fn close_settings_modal(&mut self) {
        self.settings_ui.modal = None;
    }

    pub(in crate::app) fn select_add_model_provider(&mut self, index: usize) {
        self.settings_ui.add_model_provider_index = index;
    }

    pub(in crate::app) fn set_add_model_name(&mut self, value: String) {
        self.settings_ui.add_model_name = value;
    }

    pub(in crate::app) fn save_added_model(&mut self) {
        if self.settings_ui.form.add_model(
            ProviderKind::from_index(self.settings_ui.add_model_provider_index),
            &self.settings_ui.add_model_name,
        ) {
            self.settings_ui.modal = None;
            self.settings_ui.add_model_name.clear();
        }
    }

    pub(in crate::app) fn remove_saved_model(&mut self, index: usize) {
        self.settings_ui.form.remove_model(index);
    }

    pub(in crate::app) fn open_system_prompt_modal(&mut self) {
        self.settings_ui.system_prompt_content =
            widget::text_editor::Content::with_text(&self.settings_ui.form.base_system_prompt);
        self.settings_ui.modal = Some(SettingsModal::SystemPrompt);
    }

    pub(in crate::app) fn edit_system_prompt(&mut self, action: widget::text_editor::Action) {
        self.settings_ui.system_prompt_content.perform(action);
    }

    pub(in crate::app) fn save_system_prompt(&mut self) {
        self.settings_ui.form.base_system_prompt = self.settings_ui.system_prompt_content.text();
        self.settings_ui.modal = None;
    }

    pub(in crate::app) fn test_connection(&mut self) -> Task<cosmic::Action<Message>> {
        self.settings_ui.connection_test_state = ConnectionTestState::Testing;

        let client = self.provider_client.clone();
        let provider = self.settings_ui.form.provider();
        let endpoint = self.settings_ui.form.lmstudio_base_url.clone();
        let api_key = Some(self.settings_ui.form.openrouter_api_key.clone());
        let reliability = provider::ProviderReliability {
            timeout_seconds: self
                .settings_ui
                .form
                .timeout_seconds
                .trim()
                .parse::<u64>()
                .ok()
                .filter(|value| *value > 0)
                .unwrap_or(self.state.settings.provider.timeout_seconds),
            retry_attempts: self
                .settings_ui
                .form
                .retry_attempts
                .trim()
                .parse::<u8>()
                .unwrap_or(self.state.settings.provider.retry_attempts),
            retry_delay_seconds: self
                .settings_ui
                .form
                .retry_delay_seconds
                .trim()
                .parse::<u64>()
                .unwrap_or(self.state.settings.provider.retry_delay_seconds),
        };

        cosmic::task::future(async move {
            Message::ConnectionTestFinished(
                provider::test_connection(client, provider, endpoint, api_key, reliability).await,
            )
        })
    }

    pub(in crate::app) fn finish_connection_test(&mut self, result: Result<(), String>) {
        self.settings_ui.connection_test_state = match result {
            Ok(()) => ConnectionTestState::Success,
            Err(error) => ConnectionTestState::Failed(error),
        };
    }

    pub(in crate::app) fn save_settings_and_close(&mut self) {
        match self
            .settings_ui
            .form
            .apply_to_settings(&mut self.state.settings)
        {
            Ok(()) => {}
            Err(SettingsValidationError::ContextLimit) => {
                self.settings_ui.form.context_message_limit =
                    self.state.settings.context_message_limit.to_string();
                self.status = Some("Context limit must be a whole number.".into());
                return;
            }
            Err(SettingsValidationError::TimeoutSeconds) => {
                self.settings_ui.form.timeout_seconds =
                    self.state.settings.provider.timeout_seconds.to_string();
                self.status = Some("Timeout must be a positive whole number.".into());
                return;
            }
            Err(SettingsValidationError::RetryAttempts) => {
                self.settings_ui.form.retry_attempts =
                    self.state.settings.provider.retry_attempts.to_string();
                self.status = Some("Retry attempts must be a whole number.".into());
                return;
            }
            Err(SettingsValidationError::RetryDelaySeconds) => {
                self.settings_ui.form.retry_delay_seconds =
                    self.state.settings.provider.retry_delay_seconds.to_string();
                self.status = Some("Retry delay must be a whole number.".into());
                return;
            }
        }

        if let Err(error) =
            secrets::save_openrouter_api_key(&self.state.settings.provider.openrouter_api_key)
        {
            self.status = Some(error);
        }
        self.settings_ui.modal = None;
        self.settings_ui.connection_test_state = ConnectionTestState::Idle;
        self.clear_chat_error(self.state.active_chat_id);
        self.panel_view = PanelView::Chat;
        self.persist_state();
    }
}
