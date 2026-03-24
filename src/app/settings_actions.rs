//! Settings modal, model list, and connection test flows.

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

    pub(in crate::app) fn set_context_limit(&mut self, value: String) {
        self.settings_ui.form.context_message_limit = value;
    }

    pub(in crate::app) fn set_response_start_timeout(&mut self, value: String) {
        self.settings_ui.form.response_start_timeout_secs = value;
        self.settings_ui.connection_test_state = ConnectionTestState::Idle;
    }

    pub(in crate::app) fn select_default_model(&mut self, index: usize) {
        self.settings_ui.form.select_default_model(index);
    }

    pub(in crate::app) fn select_active_model(&mut self, index: usize) {
        if let Some(model) = self.state.settings.saved_models.get(index).cloned() {
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
            widget::text_editor::Content::with_text(&self.settings_ui.form.system_prompt);
        self.settings_ui.modal = Some(SettingsModal::SystemPrompt);
    }

    pub(in crate::app) fn edit_system_prompt(&mut self, action: widget::text_editor::Action) {
        self.settings_ui.system_prompt_content.perform(action);
    }

    pub(in crate::app) fn save_system_prompt(&mut self) {
        self.settings_ui.form.system_prompt = self.settings_ui.system_prompt_content.text();
        self.settings_ui.modal = None;
    }

    pub(in crate::app) fn test_connection(&mut self) -> Task<cosmic::Action<Message>> {
        self.settings_ui.connection_test_state = ConnectionTestState::Testing;

        let client = self.provider_client.clone();
        let provider = self.settings_ui.form.provider();
        let endpoint = self.settings_ui.form.lmstudio_base_url.clone();
        let api_key = Some(self.settings_ui.form.openrouter_api_key.clone());
        let response_start_timeout_secs = self
            .settings_ui
            .form
            .response_start_timeout_secs
            .trim()
            .parse::<u64>()
            .unwrap_or(self.state.settings.response_start_timeout_secs);

        cosmic::task::future(async move {
            Message::ConnectionTestFinished(
                provider::test_connection(
                    client,
                    provider,
                    endpoint,
                    api_key,
                    response_start_timeout_secs,
                )
                .await,
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
        let validation = self.settings_ui.form.apply_to_settings(&mut self.state.settings);
        let mut invalid_messages = Vec::new();

        if !validation.context_limit_valid {
            self.settings_ui.form.context_message_limit =
                self.state.settings.context_message_limit.to_string();
            invalid_messages.push("Context limit must be a whole number.");
        }
        if !validation.response_start_timeout_valid {
            self.settings_ui.form.response_start_timeout_secs =
                self.state.settings.response_start_timeout_secs.to_string();
            invalid_messages.push("Response start timeout must be a whole number of seconds.");
        }
        if !invalid_messages.is_empty() {
            self.status = Some(invalid_messages.join(" "));
        }
        if let Err(error) = secrets::save_openrouter_api_key(&self.state.settings.openrouter_api_key)
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
