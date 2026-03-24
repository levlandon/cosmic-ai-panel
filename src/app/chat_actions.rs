//! Chat mutation flows such as submit, edit, branch, and stop.

use super::*;

impl AppModel {
    pub(in crate::app) fn submit_message(&mut self) -> Task<cosmic::Action<Message>> {
        if self.inflight_request.is_some() {
            return Task::none();
        }

        if !self.has_selected_model() {
            return Task::none();
        }

        let prompt = self.composer_text().trim().to_string();
        if prompt.is_empty() {
            return Task::none();
        }

        if self.active_chat().is_none() {
            let chat_id = self.create_chat();
            self.state.active_chat_id = chat_id;
        }

        let active_chat_id = self.state.active_chat_id;
        self.clear_chat_error(active_chat_id);

        let user_message = ChatMessage::new(self.next_message_id(), ChatRole::User, &prompt);
        let user_message_id = user_message.id;

        if let Some(chat) = self.active_chat_mut() {
            chat.messages.push(user_message);
            if chat.title.starts_with("New Chat") {
                chat.title = summarize_title(&prompt);
            }
            chat.touch();
        }

        self.clear_transient_chat_notice(active_chat_id);
        self.reset_composer();
        self.sync_message_view_content(user_message_id, &prompt);
        self.persist_state();

        let request = match self.build_provider_request(active_chat_id) {
            Ok(request) => request,
            Err(error) => {
                self.chat_error = Some(ChatErrorState {
                    chat_id: active_chat_id,
                    message: error,
                    request: ProviderRequest {
                        provider: self.state.settings.provider,
                        endpoint: String::new(),
                        api_key: None,
                        model: String::new(),
                        messages: Vec::new(),
                    },
                    assistant_message_id: None,
                });
                return cosmic::iced::widget::operation::focus(self.composer_editor_id.clone());
            }
        };

        self.start_provider_request(active_chat_id, request);

        Task::batch([
            cosmic::iced::widget::operation::focus(self.composer_editor_id.clone()),
            self.scroll_messages_to_end(true),
        ])
    }

    pub(in crate::app) fn regenerate_last_assistant(&mut self, message_id: u64) -> Task<cosmic::Action<Message>> {
        if self.inflight_request.is_some() {
            return Task::none();
        }

        let chat_id = self.state.active_chat_id;
        if self.last_assistant_message_id(chat_id) != Some(message_id) {
            return Task::none();
        }

        let mut removed = false;
        if let Some(chat) = self.state.chats.iter_mut().find(|chat| chat.id == chat_id)
            && let Some(last) = chat.messages.last()
            && last.id == message_id
            && last.role == ChatRole::Assistant
        {
            chat.messages.pop();
            chat.touch();
            removed = true;
        }

        if !removed {
            return Task::none();
        }

        self.remove_message_ids(&[message_id]);
        self.clear_chat_error(chat_id);

        let request = match self.build_provider_request(chat_id) {
            Ok(request) => request,
            Err(error) => {
                self.chat_error = Some(ChatErrorState {
                    chat_id,
                    message: error,
                    request: ProviderRequest {
                        provider: self.state.settings.provider,
                        endpoint: String::new(),
                        api_key: None,
                        model: String::new(),
                        messages: Vec::new(),
                    },
                    assistant_message_id: None,
                });
                self.persist_state();
                return Task::none();
            }
        };

        self.persist_state();
        self.start_provider_request(chat_id, request);
        self.scroll_messages_to_end(true)
    }

    pub(in crate::app) fn edit_user_message(&mut self, message_id: u64) -> Task<cosmic::Action<Message>> {
        if self.inflight_request.is_some() {
            return Task::none();
        }

        let chat_id = self.state.active_chat_id;
        let Some(content) = self
            .state
            .chats
            .iter()
            .find(|chat| chat.id == chat_id)
            .and_then(|chat| {
                chat.messages
                    .iter()
                    .find(|message| message.id == message_id && message.role == ChatRole::User)
            })
            .map(|message| message.content.clone())
        else {
            return Task::none();
        };

        self.editing_message_id = Some(message_id);
        self.clear_chat_error(chat_id);
        self.editing_content = widget::text_editor::Content::with_text(&content);

        Task::none()
    }

    pub(in crate::app) fn save_edited_message(&mut self) -> Task<cosmic::Action<Message>> {
        if self.inflight_request.is_some() {
            return Task::none();
        }

        let chat_id = self.state.active_chat_id;
        let Some(message_id) = self.editing_message_id else {
            return Task::none();
        };

        let edited_text = self.editing_content.text().trim().to_string();
        if edited_text.is_empty() {
            return cosmic::iced::widget::operation::focus(self.editing_editor_id.clone());
        }

        let mut removed_ids = Vec::new();
        let mut updated = false;
        if let Some(chat) = self.state.chats.iter_mut().find(|chat| chat.id == chat_id)
            && let Some(index) = chat
                .messages
                .iter()
                .position(|message| message.id == message_id && message.role == ChatRole::User)
        {
            removed_ids.extend(chat.messages[index + 1..].iter().map(|message| message.id));
            if let Some(message) = chat.messages.get_mut(index) {
                message.content = edited_text.clone();
            }
            chat.messages.truncate(index + 1);
            if index == 0 {
                chat.title = summarize_title(&edited_text);
            }
            chat.touch();
            updated = true;
        }

        if !updated {
            self.reset_inline_edit();
            return Task::none();
        }

        self.remove_message_ids(&removed_ids);
        self.sync_message_view_content(message_id, &edited_text);
        self.clear_chat_error(chat_id);
        self.clear_transient_chat_notice(chat_id);
        self.reset_inline_edit();
        self.persist_state();

        let request = match self.build_provider_request(chat_id) {
            Ok(request) => request,
            Err(error) => {
                self.chat_error = Some(ChatErrorState {
                    chat_id,
                    message: error,
                    request: ProviderRequest {
                        provider: self.state.settings.provider,
                        endpoint: String::new(),
                        api_key: None,
                        model: String::new(),
                        messages: Vec::new(),
                    },
                    assistant_message_id: None,
                });
                return Task::none();
            }
        };

        self.start_provider_request(chat_id, request);
        self.scroll_messages_to_end(true)
    }

    pub(in crate::app) fn delete_last_assistant(&mut self, message_id: u64) -> Task<cosmic::Action<Message>> {
        if self.inflight_request.is_some() {
            return Task::none();
        }

        let chat_id = self.state.active_chat_id;
        if self.last_assistant_message_id(chat_id) != Some(message_id) {
            return Task::none();
        }

        let mut removed = false;
        if let Some(chat) = self.state.chats.iter_mut().find(|chat| chat.id == chat_id)
            && let Some(last) = chat.messages.last()
            && last.id == message_id
            && last.role == ChatRole::Assistant
        {
            chat.messages.pop();
            chat.touch();
            removed = true;
        }

        if !removed {
            return Task::none();
        }

        self.remove_message_ids(&[message_id]);
        self.clear_chat_error(chat_id);
        self.persist_state();

        self.scroll_messages_to_end(false)
    }

    pub(in crate::app) fn branch_conversation(&mut self, message_id: u64) -> Task<cosmic::Action<Message>> {
        if self.inflight_request.is_some() {
            return Task::none();
        }

        let source_chat_id = self.state.active_chat_id;
        let Some(source_chat) = self
            .state
            .chats
            .iter()
            .find(|chat| chat.id == source_chat_id)
            .cloned()
        else {
            return Task::none();
        };

        let Some((branch_end, branch_message)) = source_chat
            .messages
            .iter()
            .enumerate()
            .find(|(_, message)| message.id == message_id)
        else {
            return Task::none();
        };
        if branch_message.role != ChatRole::Assistant {
            return Task::none();
        }

        let new_chat_id = self.state.next_chat_id;
        self.state.next_chat_id += 1;

        let mut branched_chat =
            ChatSession::new(new_chat_id, source_chat.provider, source_chat.model.clone());
        branched_chat.title = format!("{} (Branch)", source_chat.title);
        branched_chat.messages = source_chat.messages[..=branch_end]
            .iter()
            .cloned()
            .map(|mut message| {
                message.id = self.next_message_id();
                message
            })
            .collect();
        branched_chat.touch();

        self.state.chats.push(branched_chat);
        self.state.active_chat_id = new_chat_id;
        self.panel_view = PanelView::Chat;
        self.set_transient_chat_notice(
            new_chat_id,
            format!(
                "Branch created: {}",
                self.active_chat()
                    .map(|chat| chat.title.as_str())
                    .unwrap_or("New chat")
            ),
        );
        self.reset_inline_edit();
        self.reset_composer();
        self.rebuild_markdown_cache();
        self.rebuild_message_view_cache();
        self.persist_state();

        Task::batch([
            cosmic::iced::widget::operation::focus(self.composer_editor_id.clone()),
            self.scroll_messages_to_end(true),
        ])
    }

    pub(in crate::app) fn stop_generation(&mut self) -> Task<cosmic::Action<Message>> {
        if self.inflight_request.is_none() {
            return Task::none();
        }

        let stopped_chat_id = self
            .inflight_request
            .as_ref()
            .map(|request| request.chat_id);
        self.abort_inflight_request();
        self.inflight_request = None;
        self.loading_chat_id = None;
        self.loading_phase = 0;
        if let Some(chat_id) = stopped_chat_id {
            self.clear_chat_error(chat_id);
        }
        self.persist_state();

        Task::batch([
            cosmic::iced::widget::operation::focus(self.composer_editor_id.clone()),
            self.scroll_messages_to_end(false),
        ])
    }
}
