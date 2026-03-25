//! Settings tabs, personalization, model list, file exchange, and AI migration flows.

use super::*;
use crate::personalization::{self, PersonalizationSettings};
use crate::runtime::personalization::{self as runtime_personalization, AiMigrationOutcome};
use cosmic::dialog::file_chooser::{self, FileFilter};

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
        self.settings_ui.sync_ai_migration_selection();
        self.settings_ui.connection_test_state = ConnectionTestState::Idle;
    }

    pub(in crate::app) fn select_model_filter(&mut self, index: usize) {
        self.settings_ui.form.select_model_filter(index);
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
        self.settings_ui.personalization_notice = None;
    }

    pub(in crate::app) fn set_profile_language(&mut self, value: String) {
        self.settings_ui.form.profile_language = value;
        self.settings_ui.personalization_notice = None;
    }

    pub(in crate::app) fn edit_profile_occupation(&mut self, action: widget::text_editor::Action) {
        self.settings_ui.occupation_content.perform(action);
        self.settings_ui.form.profile_occupation = self.settings_ui.occupation_content.text();
        self.settings_ui.personalization_notice = None;
    }

    pub(in crate::app) fn add_memory_item(&mut self) {
        self.settings_ui.form.add_memory_item();
        self.settings_ui.memory_search_query.clear();
        self.settings_ui.personalization_notice = None;
    }

    pub(in crate::app) fn set_memory_search_query(&mut self, value: String) {
        self.settings_ui.memory_search_query = value;
    }

    pub(in crate::app) fn set_memory_item(&mut self, index: usize, value: String) {
        self.settings_ui.form.set_memory_item(index, value);
        self.settings_ui.personalization_notice = None;
    }

    pub(in crate::app) fn remove_memory_item(&mut self, index: usize) {
        self.settings_ui.form.remove_memory_item(index);
        self.settings_ui.personalization_notice = None;
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
        self.settings_ui.memory_search_query.clear();
        self.settings_ui.modal = None;
        self.settings_ui.modal_error = None;
        self.settings_ui.personalization_notice =
            Some(SettingsNotice::info("Personalization reset to defaults."));
    }

    pub(in crate::app) fn select_header_lists(&mut self, index: usize) {
        self.settings_ui.form.select_header_lists(index);
        self.settings_ui.personalization_notice = None;
    }

    pub(in crate::app) fn select_emoji(&mut self, index: usize) {
        self.settings_ui.form.select_emoji(index);
        self.settings_ui.personalization_notice = None;
    }

    pub(in crate::app) fn select_prompt_preview_mode(&mut self, mode: PromptPreviewMode) {
        self.settings_ui.preview_mode = mode;
    }

    pub(in crate::app) fn select_default_model(&mut self, index: usize) {
        self.settings_ui.form.select_default_model(index);
        self.settings_ui.sync_ai_migration_selection();
    }

    pub(in crate::app) fn select_active_model(&mut self, index: usize) {
        let Some(provider) = self.active_chat().map(|chat| chat.provider) else {
            return;
        };
        let model = self
            .state
            .settings
            .provider
            .saved_models
            .iter()
            .filter(|model| model.provider == provider)
            .nth(index)
            .cloned();

        let Some(model) = model else {
            return;
        };

        if let Some(chat) = self.active_chat_mut() {
            chat.provider = model.provider;
            chat.model = model.name;
            chat.touch();
        }
        self.clear_chat_error(self.state.active_chat_id);
        self.persist_state();
    }

    pub(in crate::app) fn open_manage_models_modal(&mut self) {
        self.settings_ui.modal = Some(SettingsModal::ManageModels);
        self.settings_ui.modal_error = None;
    }

    pub(in crate::app) fn open_manage_memory_modal(&mut self) {
        self.settings_ui.modal = Some(SettingsModal::ManageMemory);
        self.settings_ui.modal_error = None;
        self.settings_ui.memory_search_query.clear();
    }

    pub(in crate::app) fn open_add_model_modal(&mut self) {
        self.settings_ui.modal = Some(SettingsModal::AddModel);
        self.settings_ui.add_model_provider_index =
            self.settings_ui.form.add_model_provider().index();
        self.settings_ui.add_model_name.clear();
    }

    pub(in crate::app) fn close_settings_modal(&mut self) {
        if self.settings_ui.modal == Some(SettingsModal::AiMigration) {
            self.settings_ui.reset_ai_migration();
        }
        self.settings_ui.memory_search_query.clear();
        self.settings_ui.modal = None;
        self.settings_ui.modal_error = None;
    }

    pub(in crate::app) fn open_reset_personalization_confirm(&mut self) {
        self.settings_ui.modal = Some(SettingsModal::ConfirmResetPersonalization);
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
            self.settings_ui.sync_ai_migration_selection();
            self.settings_ui.modal = Some(SettingsModal::ManageModels);
            self.settings_ui.add_model_name.clear();
        }
    }

    pub(in crate::app) fn remove_saved_model(&mut self, index: usize) {
        self.settings_ui.form.remove_model(index);
        self.settings_ui.sync_ai_migration_selection();
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

    pub(in crate::app) fn import_personalization_from_file(
        &mut self,
    ) -> Task<cosmic::Action<Message>> {
        self.settings_ui.personalization_notice = None;

        cosmic::task::future(async move {
            let filter = FileFilter::new("Personalization JSON").glob("*.json");
            let dialog = file_chooser::open::Dialog::new()
                .title("Import personalization")
                .filter(filter);

            let response = match dialog.open_file().await {
                Ok(response) => response,
                Err(file_chooser::Error::Cancelled) => {
                    return Message::ImportPersonalizationFinished(Ok(None));
                }
                Err(error) => {
                    return Message::ImportPersonalizationFinished(Err(format!(
                        "Failed to open import dialog: {error}"
                    )));
                }
            };

            let path = match response.url().to_file_path() {
                Ok(path) => path,
                Err(_) => {
                    return Message::ImportPersonalizationFinished(Err(
                        "Selected import path was not a local file.".into(),
                    ));
                }
            };

            let contents = match tokio::fs::read_to_string(&path).await {
                Ok(contents) => contents,
                Err(error) => {
                    return Message::ImportPersonalizationFinished(Err(format!(
                        "Failed to read {}: {error}",
                        path.display()
                    )));
                }
            };

            match personalization::import_personalization(&contents) {
                Ok(imported) => Message::ImportPersonalizationFinished(Ok(Some((
                    path.display().to_string(),
                    imported,
                )))),
                Err(error) => Message::ImportPersonalizationFinished(Err(error)),
            }
        })
    }

    pub(in crate::app) fn finish_import_personalization(
        &mut self,
        result: Result<Option<(String, PersonalizationSettings)>, String>,
    ) {
        match result {
            Ok(Some((path, imported))) => {
                self.apply_personalization(imported);
                self.settings_ui.personalization_notice = Some(SettingsNotice::info(format!(
                    "Imported personalization from {path}"
                )));
            }
            Ok(None) => {}
            Err(error) => {
                self.settings_ui.personalization_notice = Some(SettingsNotice::error(error));
            }
        }
    }

    pub(in crate::app) fn export_personalization_to_file(
        &mut self,
    ) -> Task<cosmic::Action<Message>> {
        self.settings_ui.personalization_notice = None;
        let export = personalization::export_personalization(
            &self.settings_ui.form.personalization_settings(),
        );

        cosmic::task::future(async move {
            let export = match export {
                Ok(export) => export,
                Err(error) => return Message::ExportPersonalizationFinished(Err(error)),
            };

            let filter = FileFilter::new("Personalization JSON").glob("*.json");
            let dialog = file_chooser::save::Dialog::new()
                .title("Export personalization".to_string())
                .file_name("personalization.json".into())
                .filter(filter);

            let response = match dialog.save_file().await {
                Ok(response) => response,
                Err(file_chooser::Error::Cancelled) => {
                    return Message::ExportPersonalizationFinished(Ok(None));
                }
                Err(error) => {
                    return Message::ExportPersonalizationFinished(Err(format!(
                        "Failed to open save dialog: {error}"
                    )));
                }
            };

            let Some(url) = response.url() else {
                return Message::ExportPersonalizationFinished(Ok(None));
            };

            let path = match url.to_file_path() {
                Ok(path) => path,
                Err(_) => {
                    return Message::ExportPersonalizationFinished(Err(
                        "Selected export path was not a local file.".into(),
                    ));
                }
            };

            match tokio::fs::write(&path, export).await {
                Ok(()) => {
                    Message::ExportPersonalizationFinished(Ok(Some(path.display().to_string())))
                }
                Err(error) => Message::ExportPersonalizationFinished(Err(format!(
                    "Failed to write {}: {error}",
                    path.display()
                ))),
            }
        })
    }

    pub(in crate::app) fn finish_export_personalization(
        &mut self,
        result: Result<Option<String>, String>,
    ) {
        match result {
            Ok(Some(path)) => {
                self.settings_ui.personalization_notice = Some(SettingsNotice::info(format!(
                    "Exported personalization to {path}"
                )));
            }
            Ok(None) => {}
            Err(error) => {
                self.settings_ui.personalization_notice = Some(SettingsNotice::error(error));
            }
        }
    }

    pub(in crate::app) fn open_ai_migration_modal(&mut self) {
        self.settings_ui.reset_ai_migration();
        self.settings_ui.modal = Some(SettingsModal::AiMigration);
        self.settings_ui.modal_error = None;
    }

    pub(in crate::app) fn select_ai_migration_model(&mut self, index: usize) {
        self.settings_ui.ai_migration_model_index = self
            .settings_ui
            .form
            .provider_model_raw_index(self.settings_ui.form.provider(), index);
    }

    pub(in crate::app) fn edit_ai_migration_input(&mut self, action: widget::text_editor::Action) {
        self.settings_ui.ai_migration_input_content.perform(action);
        if matches!(
            self.settings_ui.ai_migration_state,
            AiMigrationState::Failed { .. }
        ) {
            self.settings_ui.ai_migration_state = AiMigrationState::Editor;
        }
    }

    pub(in crate::app) fn start_ai_migration(&mut self) -> Task<cosmic::Action<Message>> {
        let raw_input = self.settings_ui.ai_migration_input_content.text();
        if let Ok(outcome) = runtime_personalization::process_ai_migration_response(&raw_input) {
            self.complete_ai_migration_success(outcome);
            return Task::none();
        }

        let Some(model_index) = self.settings_ui.ai_migration_model_index else {
            self.settings_ui.finish_ai_migration_with_error(
                "Paste a valid JSON payload or choose a model for AI migration.".into(),
            );
            return Task::none();
        };
        let Some(model) = self.settings_ui.form.saved_models.get(model_index).cloned() else {
            self.settings_ui.finish_ai_migration_with_error(
                "Selected migration model is no longer available.".into(),
            );
            return Task::none();
        };

        let settings = self.runtime_provider_settings_from_form();
        let previous_error = self
            .settings_ui
            .ai_migration_state
            .error()
            .map(str::to_string);
        let request = match runtime_personalization::build_ai_migration_request(
            &settings,
            Some(&settings.provider.openrouter_api_key),
            &model,
            &raw_input,
            previous_error.as_deref(),
        ) {
            Ok(request) => request,
            Err(error) => {
                self.settings_ui.finish_ai_migration_with_error(error);
                return Task::none();
            }
        };

        let request_id = self.settings_ui.begin_ai_migration_request();
        self.settings_ui.personalization_notice = None;

        let client = self.provider_client.clone();
        cosmic::task::future(async move {
            let result = provider::generate_text(client, request)
                .await
                .and_then(|response| {
                    runtime_personalization::process_ai_migration_response(&response)
                });
            Message::AiMigrationFinished { request_id, result }
        })
    }

    pub(in crate::app) fn return_to_ai_migration_editor(&mut self) {
        self.settings_ui.cancel_ai_migration_processing();
    }

    pub(in crate::app) fn tick_ai_migration_progress(&mut self) {
        let completion_ratio = match &self.settings_ui.ai_migration_state {
            AiMigrationState::Success {
                completion_ratio, ..
            } => Some(*completion_ratio),
            AiMigrationState::Editor
            | AiMigrationState::Processing { .. }
            | AiMigrationState::Failed { .. } => None,
        };

        if self.settings_ui.tick_ai_migration()
            && let Some(completion_ratio) = completion_ratio
        {
            let percent = (completion_ratio * 100.0).round() as u8;
            self.settings_ui.reset_ai_migration();
            self.settings_ui.modal = None;
            self.settings_ui.personalization_notice = Some(SettingsNotice::info(format!(
                "AI migration applied ({percent}% filled)."
            )));
        }
    }

    pub(in crate::app) fn finish_ai_migration(
        &mut self,
        request_id: u64,
        result: Result<AiMigrationOutcome, String>,
    ) {
        if self.settings_ui.ai_migration_active_request_id != Some(request_id) {
            return;
        }

        match result {
            Ok(outcome) => self.complete_ai_migration_success(outcome),
            Err(error) => self.settings_ui.finish_ai_migration_with_error(error),
        }
    }

    pub(in crate::app) fn open_prompt_preview_modal(&mut self) {
        self.settings_ui.preview_mode = PromptPreviewMode::Code;
        self.settings_ui.modal = Some(SettingsModal::PromptPreview);
        self.settings_ui.modal_error = None;
    }

    pub(in crate::app) fn edit_settings_modal(&mut self, action: widget::text_editor::Action) {
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
                self.settings_ui.personalization_notice = None;
                self.settings_ui.modal = None;
            }
            TextEditorModal::ResponseStyle => {
                self.settings_ui.form.response_style = self.settings_ui.modal_editor_content.text();
                self.settings_ui.personalization_notice = None;
                self.settings_ui.modal = None;
            }
            TextEditorModal::MoreAboutYou => {
                self.settings_ui.form.more_about_you = self.settings_ui.modal_editor_content.text();
                self.settings_ui.personalization_notice = None;
                self.settings_ui.modal = None;
            }
        }
    }

    fn open_text_editor_modal(&mut self, kind: TextEditorModal) {
        let content = match kind {
            TextEditorModal::BaseSystemPrompt => self.settings_ui.form.base_system_prompt.clone(),
            TextEditorModal::ResponseStyle => self.settings_ui.form.response_style.clone(),
            TextEditorModal::MoreAboutYou => self.settings_ui.form.more_about_you.clone(),
        };

        self.settings_ui.set_modal_editor_text(&content);
        self.settings_ui.modal = Some(SettingsModal::Editor(kind));
        self.settings_ui.modal_error = None;
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

    fn runtime_provider_settings_from_form(&self) -> AppSettings {
        let mut settings = self.state.settings.clone();
        settings.provider.active_provider = self.settings_ui.form.provider();
        settings.provider.openrouter_api_key = self.settings_ui.form.openrouter_api_key.clone();
        settings.provider.lmstudio_base_url = self.settings_ui.form.lmstudio_base_url.clone();
        settings.provider.saved_models = self.settings_ui.form.saved_models.clone();
        settings.provider.default_model = self.settings_ui.form.default_model.clone();
        settings.provider.model_filter = self.settings_ui.form.model_filter();
        settings.provider.follow_provider_selection = false;
        settings.provider.timeout_seconds = self
            .settings_ui
            .form
            .timeout_seconds
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .unwrap_or(settings.provider.timeout_seconds);
        settings.provider.retry_attempts = self
            .settings_ui
            .form
            .retry_attempts
            .trim()
            .parse::<u8>()
            .unwrap_or(settings.provider.retry_attempts);
        settings.provider.retry_delay_seconds = self
            .settings_ui
            .form
            .retry_delay_seconds
            .trim()
            .parse::<u64>()
            .unwrap_or(settings.provider.retry_delay_seconds);
        settings.provider.normalize();
        settings
    }

    fn apply_personalization(&mut self, personalization: PersonalizationSettings) {
        self.settings_ui
            .form
            .apply_personalization(&personalization);
        self.settings_ui.sync_personalization_editors();
    }

    fn complete_ai_migration_success(&mut self, outcome: AiMigrationOutcome) {
        self.apply_personalization(outcome.personalization);
        self.settings_ui
            .finish_ai_migration_with_success(outcome.completion_ratio);
        self.settings_ui.personalization_notice = None;
    }
}
