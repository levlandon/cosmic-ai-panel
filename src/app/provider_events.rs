//! Provider request orchestration and streaming event handling.

use super::*;

impl AppModel {
    pub(in crate::app) fn empty_provider_request(&self) -> ProviderRequest {
        ProviderRequest {
            provider: self.state.settings.provider,
            endpoint: String::new(),
            api_key: None,
            model: String::new(),
            messages: Vec::new(),
            response_start_timeout_secs: self.state.settings.response_start_timeout_secs,
        }
    }

    pub(in crate::app) fn build_provider_request(&self, chat_id: u64) -> Result<ProviderRequest, String> {
        let chat = self
            .state
            .chats
            .iter()
            .find(|chat| chat.id == chat_id)
            .ok_or_else(|| "Chat not found.".to_string())?;

        provider::build_request(
            &self.state.settings,
            Some(self.state.settings.openrouter_api_key.as_str()),
            chat,
        )
    }

    pub(in crate::app) fn start_provider_request(&mut self, chat_id: u64, request: ProviderRequest) {
        self.abort_inflight_request();

        let request_id = self.next_request_id;
        self.next_request_id += 1;
        self.loading_chat_id = Some(chat_id);
        self.loading_phase = 0;
        self.chat_error = None;
        self.inflight_request = Some(InflightRequest {
            request_id,
            chat_id,
            request: request.clone(),
            assistant_message_id: None,
        });

        let client = self.provider_client.clone();
        let tx = self.provider_events_tx.clone();
        self.provider_task = Some(tokio::spawn(async move {
            provider::stream_chat(client, request_id, chat_id, request, tx).await;
        }));
    }

    pub(in crate::app) fn retry_request(&mut self, chat_id: u64) -> Task<cosmic::Action<Message>> {
        if self.inflight_request.is_some() {
            return Task::none();
        }

        let Some(error) = self.chat_error.clone() else {
            return Task::none();
        };
        if error.chat_id != chat_id {
            return Task::none();
        }

        let request = match self.build_provider_request(chat_id) {
            Ok(request) => request,
            Err(message) => {
                self.chat_error = Some(ChatErrorState {
                    chat_id,
                    message,
                    request: self.empty_provider_request(),
                    assistant_message_id: error.assistant_message_id,
                });
                return Task::none();
            }
        };

        self.start_provider_request(chat_id, request);
        Task::none()
    }

    pub(in crate::app) fn handle_provider_delta(&mut self, request_id: u64, chat_id: u64, delta: String) {
        if delta.is_empty() {
            return;
        }

        let Some((inflight_request_id, inflight_chat_id)) = self
            .inflight_request
            .as_ref()
            .map(|request| (request.request_id, request.chat_id))
        else {
            return;
        };
        if inflight_request_id != request_id || inflight_chat_id != chat_id {
            return;
        }

        self.loading_chat_id = None;
        self.clear_transient_chat_notice(chat_id);

        if let Some(message_id) = self
            .inflight_request
            .as_ref()
            .and_then(|request| request.assistant_message_id)
        {
            let mut next_content = None;
            if let Some(chat) = self.state.chats.iter_mut().find(|chat| chat.id == chat_id)
                && let Some(message) = chat
                    .messages
                    .iter_mut()
                    .find(|message| message.id == message_id)
            {
                message.content.push_str(&delta);
                next_content = Some(message.content.clone());
                self.assistant_markdown
                    .entry(message_id)
                    .or_default()
                    .push_str(&delta);
                chat.touch();
            }
            if let Some(content) = next_content {
                self.sync_message_view_content(message_id, &content);
            }
            return;
        }

        let assistant_message_id = self.next_message_id();
        let assistant_message = ChatMessage::new(assistant_message_id, ChatRole::Assistant, delta);
        if let Some(chat) = self.state.chats.iter_mut().find(|chat| chat.id == chat_id) {
            chat.messages.push(assistant_message);
            chat.touch();
        }

        let assistant_content = self
            .state
            .chats
            .iter()
            .find(|chat| chat.id == chat_id)
            .and_then(|chat| chat.messages.last())
            .map(|message| message.content.clone())
            .unwrap_or_default();
        self.sync_message_view_content(assistant_message_id, &assistant_content);
        self.assistant_markdown.insert(
            assistant_message_id,
            widget::markdown::Content::parse(
                self.state
                    .chats
                    .iter()
                    .find(|chat| chat.id == chat_id)
                    .and_then(|chat| chat.messages.last())
                    .map(|message| message.content.as_str())
                    .unwrap_or_default(),
            ),
        );
        if let Some(inflight) = self.inflight_request.as_mut() {
            inflight.assistant_message_id = Some(assistant_message_id);
        }
    }

    pub(in crate::app) fn handle_provider_finished(&mut self, request_id: u64, chat_id: u64) {
        if self
            .inflight_request
            .as_ref()
            .map(|request| (request.request_id, request.chat_id))
            != Some((request_id, chat_id))
        {
            return;
        }

        self.provider_task = None;
        self.inflight_request = None;
        self.loading_chat_id = None;
        self.loading_phase = 0;
        self.persist_state();
    }

    pub(in crate::app) fn handle_provider_failed(&mut self, request_id: u64, chat_id: u64, error: String) {
        let Some(inflight) = self.inflight_request.take() else {
            return;
        };
        if inflight.request_id != request_id || inflight.chat_id != chat_id {
            self.inflight_request = Some(inflight);
            return;
        }

        self.provider_task = None;
        self.loading_chat_id = None;
        self.loading_phase = 0;
        self.chat_error = Some(ChatErrorState {
            chat_id,
            message: error,
            request: inflight.request,
            assistant_message_id: inflight.assistant_message_id,
        });
        self.persist_state();
    }

    pub(in crate::app) fn clear_chat_error(&mut self, chat_id: u64) {
        if self.chat_error.as_ref().map(|error| error.chat_id) == Some(chat_id) {
            self.chat_error = None;
        }
    }

    pub(in crate::app) fn abort_inflight_request(&mut self) {
        if let Some(task) = self.provider_task.take() {
            task.abort();
        }
    }
}
