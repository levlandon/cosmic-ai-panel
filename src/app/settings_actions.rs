//! Settings modal, model list, and connection test flows.

use super::*;

impl AppModel {
    pub(in crate::app) fn open_settings(&mut self) {
        self.settings_form = SettingsForm::from_settings(&self.state.settings);
        self.settings_modal = None;
        self.add_model_provider_index = self.state.settings.provider.index();
        self.add_model_name.clear();
        self.system_prompt_content =
            widget::text_editor::Content::with_text(&self.state.settings.system_prompt);
        self.connection_test_state = ConnectionTestState::Idle;
        self.panel_view = PanelView::Settings;
    }

    pub(in crate::app) fn close_settings(&mut self) -> Task<cosmic::Action<Message>> {
        self.settings_modal = None;
        self.panel_view = PanelView::Chat;
        self.scroll_messages_to_end(true)
    }

    pub(in crate::app) fn select_settings_provider(&mut self, index: usize) {
        self.settings_form
            .select_provider(ProviderKind::from_index(index));
        self.connection_test_state = ConnectionTestState::Idle;
    }

    pub(in crate::app) fn set_openrouter_key(&mut self, value: String) {
        self.settings_form.openrouter_api_key = value;
        self.connection_test_state = ConnectionTestState::Idle;
    }

    pub(in crate::app) fn set_lmstudio_url(&mut self, value: String) {
        self.settings_form.lmstudio_base_url = value;
        self.connection_test_state = ConnectionTestState::Idle;
    }

    pub(in crate::app) fn set_context_limit(&mut self, value: String) {
        self.settings_form.context_message_limit = value;
    }

    pub(in crate::app) fn select_default_model(&mut self, index: usize) {
        self.settings_form.select_default_model(index);
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
        self.settings_modal = Some(SettingsModal::AddModel);
        self.add_model_provider_index = self.settings_form.provider().index();
        self.add_model_name.clear();
    }

    pub(in crate::app) fn close_settings_modal(&mut self) {
        self.settings_modal = None;
    }

    pub(in crate::app) fn select_add_model_provider(&mut self, index: usize) {
        self.add_model_provider_index = index;
    }

    pub(in crate::app) fn set_add_model_name(&mut self, value: String) {
        self.add_model_name = value;
    }

    pub(in crate::app) fn save_added_model(&mut self) {
        if self.settings_form.add_model(
            ProviderKind::from_index(self.add_model_provider_index),
            &self.add_model_name,
        ) {
            self.settings_modal = None;
            self.add_model_name.clear();
        }
    }

    pub(in crate::app) fn remove_saved_model(&mut self, index: usize) {
        self.settings_form.remove_model(index);
    }

    pub(in crate::app) fn open_system_prompt_modal(&mut self) {
        self.system_prompt_content =
            widget::text_editor::Content::with_text(&self.settings_form.system_prompt);
        self.settings_modal = Some(SettingsModal::SystemPrompt);
    }

    pub(in crate::app) fn edit_system_prompt(&mut self, action: widget::text_editor::Action) {
        self.system_prompt_content.perform(action);
    }

    pub(in crate::app) fn save_system_prompt(&mut self) {
        self.settings_form.system_prompt = self.system_prompt_content.text();
        self.settings_modal = None;
    }

    pub(in crate::app) fn test_connection(&mut self) -> Task<cosmic::Action<Message>> {
        self.connection_test_state = ConnectionTestState::Testing;

        let client = self.provider_client.clone();
        let provider = self.settings_form.provider();
        let endpoint = self.settings_form.lmstudio_base_url.clone();
        let api_key = Some(self.settings_form.openrouter_api_key.clone());

        cosmic::task::future(async move {
            Message::ConnectionTestFinished(
                provider::test_connection(client, provider, endpoint, api_key).await,
            )
        })
    }

    pub(in crate::app) fn finish_connection_test(&mut self, result: Result<(), String>) {
        self.connection_test_state = match result {
            Ok(()) => ConnectionTestState::Success,
            Err(error) => ConnectionTestState::Failed(error),
        };
    }

    pub(in crate::app) fn save_settings_and_close(&mut self) {
        let context_limit_valid = self.settings_form.apply_to_settings(&mut self.state.settings);
        if !context_limit_valid {
            self.settings_form.context_message_limit =
                self.state.settings.context_message_limit.to_string();
            self.status = Some("Context limit must be a whole number.".into());
        }
        if let Err(error) = secrets::save_openrouter_api_key(&self.state.settings.openrouter_api_key)
        {
            self.status = Some(error);
        }
        self.settings_modal = None;
        self.connection_test_state = ConnectionTestState::Idle;
        self.clear_chat_error(self.state.active_chat_id);
        self.panel_view = PanelView::Chat;
        self.persist_state();
    }
}
