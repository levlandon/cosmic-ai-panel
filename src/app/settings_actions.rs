//! Settings tabs, personalization, model list, and connection test flows.

use super::*;
use crate::personalization;

impl AppModel {
    pub(in crate::app) fn open_settings(&mut self) {
        self.settings_ui.refresh_from_settings(&self.state.settings);
        self.panel_view = PanelView::Settings;
    }

    pub(in crate::app) fn close_settings(&mut self) -> Task<cosmic::Action<Message>> {
        self.settings_ui.modal = None;
        self.settings_ui.modal_error = None;
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

    pub(in crate::app) fn edit_profile_occupation(&mut self, action: widget::text_editor::Action) {
        self.settings_ui.occupation_content.perform(action);
        self.settings_ui.form.profile_occupation = self.settings_ui.occupation_content.text();
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
        self.settings_ui.sync_personalization_editors();
        let base_system_prompt = self.settings_ui.form.base_system_prompt.clone();
        self.settings_ui.set_modal_editor_text(&base_system_prompt);
        self.settings_ui.modal_error = None;
    }

    pub(in crate::app) fn select_header_lists(&mut self, index: usize) {
        self.settings_ui.form.select_header_lists(index);
    }

    pub(in crate::app) fn select_emoji(&mut self, index: usize) {
        self.settings_ui.form.select_emoji(index);
    }

    pub(in crate::app) fn select_prompt_preview_mode(&mut self, mode: PromptPreviewMode) {
        self.settings_ui.preview_mode = mode;
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
        self.settings_ui.modal_error = None;
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
        self.open_text_editor_modal(TextEditorModal::BaseSystemPrompt);
    }

    pub(in crate::app) fn open_response_style_modal(&mut self) {
        self.open_text_editor_modal(TextEditorModal::ResponseStyle);
    }

    pub(in crate::app) fn open_more_about_you_modal(&mut self) {
        self.open_text_editor_modal(TextEditorModal::MoreAboutYou);
    }

    pub(in crate::app) fn open_import_personalization_modal(&mut self) {
        self.open_text_editor_modal(TextEditorModal::ImportPersonalization);
    }

    pub(in crate::app) fn open_export_personalization_modal(&mut self) {
        self.open_text_editor_modal(TextEditorModal::ExportPersonalization);
    }

    pub(in crate::app) fn open_prompt_preview_modal(&mut self) {
        self.settings_ui.preview_mode = PromptPreviewMode::Code;
        self.settings_ui.modal = Some(SettingsModal::PromptPreview);
        self.settings_ui.modal_error = None;
    }

    pub(in crate::app) fn edit_settings_modal(&mut self, action: widget::text_editor::Action) {
        if matches!(
            self.settings_ui.modal,
            Some(SettingsModal::Editor(
                TextEditorModal::ExportPersonalization
            ))
        ) {
            return;
        }

        self.settings_ui.modal_editor_content.perform(action);
        self.settings_ui.modal_error = None;
    }

    pub(in crate::app) fn save_settings_modal(&mut self) {
        let Some(SettingsModal::Editor(kind)) = self.settings_ui.modal else {
            return;
        };

        match kind {
            TextEditorModal::BaseSystemPrompt => {
                self.settings_ui.form.base_system_prompt =
                    self.settings_ui.modal_editor_content.text();
                self.settings_ui.modal = None;
            }
            TextEditorModal::ResponseStyle => {
                self.settings_ui.form.response_style = self.settings_ui.modal_editor_content.text();
                self.settings_ui.modal = None;
            }
            TextEditorModal::MoreAboutYou => {
                self.settings_ui.form.more_about_you = self.settings_ui.modal_editor_content.text();
                self.settings_ui.modal = None;
            }
            TextEditorModal::ImportPersonalization => {
                match personalization::import_personalization(
                    &self.settings_ui.modal_editor_content.text(),
                ) {
                    Ok(imported) => {
                        self.settings_ui.form.apply_personalization(&imported);
                        self.settings_ui.sync_personalization_editors();
                        self.settings_ui.modal = None;
                        self.settings_ui.modal_error = None;
                    }
                    Err(error) => {
                        self.settings_ui.modal_error = Some(error);
                    }
                }
            }
            TextEditorModal::ExportPersonalization => {}
        }
    }

    pub(in crate::app) fn copy_exported_personalization(
        &mut self,
    ) -> Task<cosmic::Action<Message>> {
        self.copy_to_clipboard(
            CopiedTarget::SettingsExport,
            self.settings_ui.modal_editor_content.text(),
        )
    }

    fn open_text_editor_modal(&mut self, kind: TextEditorModal) {
        let mut modal_error = None;
        let content = match kind {
            TextEditorModal::BaseSystemPrompt => self.settings_ui.form.base_system_prompt.clone(),
            TextEditorModal::ResponseStyle => self.settings_ui.form.response_style.clone(),
            TextEditorModal::MoreAboutYou => self.settings_ui.form.more_about_you.clone(),
            TextEditorModal::ImportPersonalization => String::new(),
            TextEditorModal::ExportPersonalization => {
                match personalization::export_personalization(
                    &self.settings_ui.form.personalization_settings(),
                ) {
                    Ok(content) => content,
                    Err(error) => {
                        modal_error = Some(error);
                        String::new()
                    }
                }
            }
        };

        self.settings_ui.set_modal_editor_text(&content);
        self.settings_ui.modal = Some(SettingsModal::Editor(kind));
        self.settings_ui.modal_error = modal_error;
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
