//! Central message dispatch extracted from the application trait entrypoint.

use super::*;

impl AppModel {
    pub(in crate::app) fn handle_message(
        &mut self,
        message: Message,
    ) -> Task<cosmic::Action<Message>> {
        match message {
            Message::AppletPressed => self.handle_applet_pressed(),
            Message::UpdateConfig(config) => {
                self.config = config;
                Task::none()
            }
            Message::NewChat => self.handle_new_chat(),
            Message::ToggleChatList => {
                self.toggle_chat_list();
                Task::none()
            }
            Message::SelectChat(chat_id) => self.select_chat(chat_id),
            Message::ChatHovered(chat_id) => {
                self.hovered_chat_id = Some(chat_id);
                Task::none()
            }
            Message::ChatUnhovered(chat_id) => {
                if self.hovered_chat_id == Some(chat_id) {
                    self.hovered_chat_id = None;
                }
                Task::none()
            }
            Message::MessageHovered(message_id) => {
                self.hovered_message_id = Some(message_id);
                Task::none()
            }
            Message::MessageUnhovered(message_id) => {
                if self.hovered_message_id == Some(message_id) {
                    self.hovered_message_id = None;
                }
                Task::none()
            }
            Message::BeginRenameChat(chat_id) => {
                self.begin_rename_chat(chat_id);
                Task::none()
            }
            Message::RenameInputChanged(value) => {
                self.rename_input = value;
                Task::none()
            }
            Message::CommitRenameChat(value) => {
                self.commit_rename_chat(value);
                Task::none()
            }
            Message::CancelRenameChat => {
                self.cancel_rename_chat();
                Task::none()
            }
            Message::DeleteChat(chat_id) => {
                self.delete_chat_action(chat_id);
                Task::none()
            }
            Message::OpenSettings => {
                self.open_settings();
                Task::none()
            }
            Message::CloseSettings => self.close_settings(),
            Message::ChatScrolled(viewport) => {
                self.messages_bottom_distance = viewport.absolute_offset_reversed().y;
                Task::none()
            }
            Message::ComposerEdited(action) => {
                self.composer_content.perform(action);
                Task::none()
            }
            Message::InlineEditEdited(action) => {
                self.editing_content.perform(action);
                Task::none()
            }
            Message::MessageViewerEdited(message_id, action) => {
                self.perform_message_view_action(message_id, action);
                Task::none()
            }
            Message::MarkdownLink(_uri) => Task::none(),
            Message::SubmitComposer => self.submit_message(),
            Message::SaveEditedMessage => self.save_edited_message(),
            Message::CancelEditedMessage => {
                self.reset_inline_edit();
                Task::none()
            }
            Message::StopGeneration => self.stop_generation(),
            Message::LoadingTick => {
                self.loading_phase = (self.loading_phase + 1) % 6;
                Task::none()
            }
            Message::ProviderEvent(event) => self.handle_provider_event(event),
            Message::CopyMessage(message_id) => {
                if let Some(content) = self.message_content_by_id(message_id) {
                    self.copy_to_clipboard(CopiedTarget::Message(message_id), content)
                } else {
                    Task::none()
                }
            }
            Message::CopyCodeBlock {
                message_id,
                block_index,
                content,
            } => self.copy_to_clipboard(
                CopiedTarget::CodeBlock {
                    message_id,
                    block_index,
                },
                content,
            ),
            Message::ResetCopiedTarget(target) => {
                if self.copied_target.as_ref() == Some(&target) {
                    self.copied_target = None;
                }
                Task::none()
            }
            Message::RetryRequest(chat_id) => self.retry_request(chat_id),
            Message::RegenerateLastAssistant(message_id) => {
                self.regenerate_last_assistant(message_id)
            }
            Message::EditUserMessage(message_id) => self.edit_user_message(message_id),
            Message::DeleteLastAssistant(message_id) => self.delete_last_assistant(message_id),
            Message::BranchConversation(message_id) => self.branch_conversation(message_id),
            Message::ProviderSelected(index) => {
                self.select_settings_provider(index);
                Task::none()
            }
            Message::OpenRouterKeyChanged(value) => {
                self.set_openrouter_key(value);
                Task::none()
            }
            Message::LmStudioUrlChanged(value) => {
                self.set_lmstudio_url(value);
                Task::none()
            }
            Message::ContextLimitChanged(value) => {
                self.set_context_limit(value);
                Task::none()
            }
            Message::DefaultModelSelected(index) => {
                self.select_default_model(index);
                Task::none()
            }
            Message::ActiveModelSelected(index) => {
                self.select_active_model(index);
                Task::none()
            }
            Message::OpenAddModelModal => {
                self.open_add_model_modal();
                Task::none()
            }
            Message::CloseSettingsModal => {
                self.close_settings_modal();
                Task::none()
            }
            Message::AddModelProviderSelected(index) => {
                self.select_add_model_provider(index);
                Task::none()
            }
            Message::AddModelNameChanged(value) => {
                self.set_add_model_name(value);
                Task::none()
            }
            Message::SaveAddedModel => {
                self.save_added_model();
                Task::none()
            }
            Message::RemoveSavedModel(index) => {
                self.remove_saved_model(index);
                Task::none()
            }
            Message::OpenSystemPromptModal => {
                self.open_system_prompt_modal();
                Task::none()
            }
            Message::SystemPromptEdited(action) => {
                self.edit_system_prompt(action);
                Task::none()
            }
            Message::SaveSystemPrompt => {
                self.save_system_prompt();
                Task::none()
            }
            Message::TestConnection => self.test_connection(),
            Message::ConnectionTestFinished(result) => {
                self.finish_connection_test(result);
                Task::none()
            }
            Message::SaveSettings => {
                self.save_settings_and_close();
                Task::none()
            }
            Message::TogglePanel => self.toggle_panel(),
            Message::EscapePressed(id) => self.handle_escape_pressed(id),
            Message::PanelClosed(id) => self.handle_panel_closed(id),
        }
    }

    fn handle_provider_event(
        &mut self,
        event: provider::ProviderEvent,
    ) -> Task<cosmic::Action<Message>> {
        match event {
            provider::ProviderEvent::Delta {
                request_id,
                chat_id,
                delta,
            } => {
                self.handle_provider_delta(request_id, chat_id, delta);
                self.scroll_messages_to_end(false)
            }
            provider::ProviderEvent::Finished {
                request_id,
                chat_id,
            } => {
                self.handle_provider_finished(request_id, chat_id);
                self.scroll_messages_to_end(false)
            }
            provider::ProviderEvent::Failed {
                request_id,
                chat_id,
                error,
            } => {
                self.handle_provider_failed(request_id, chat_id, error);
                self.scroll_messages_to_end(false)
            }
        }
    }
}
