//! Chat list and selection flows that do not involve provider requests.

use super::*;

impl AppModel {
    pub(in crate::app) fn handle_new_chat(&mut self) -> Task<cosmic::Action<Message>> {
        let new_chat_id = self.create_chat();
        self.state.active_chat_id = new_chat_id;
        self.panel_view = PanelView::Chat;
        self.reset_composer();
        self.reset_inline_edit();
        self.rename_chat_id = None;
        self.hovered_message_id = None;
        self.persist_state();
        self.scroll_messages_to_end(true)
    }

    pub(in crate::app) fn toggle_chat_list(&mut self) {
        self.rename_chat_id = None;
        self.rename_input.clear();
        self.panel_view = if self.panel_view == PanelView::Chats {
            PanelView::Chat
        } else {
            PanelView::Chats
        };
    }

    pub(in crate::app) fn select_chat(&mut self, chat_id: u64) -> Task<cosmic::Action<Message>> {
        self.state.active_chat_id = chat_id;
        self.panel_view = PanelView::Chat;
        self.reset_inline_edit();
        self.rename_chat_id = None;
        self.hovered_chat_id = None;
        self.hovered_message_id = None;
        self.scroll_messages_to_end(true)
    }

    pub(in crate::app) fn begin_rename_chat(&mut self, chat_id: u64) {
        self.rename_chat_id = Some(chat_id);
        self.rename_input = self
            .state
            .chats
            .iter()
            .find(|chat| chat.id == chat_id)
            .map(|chat| chat.title.clone())
            .unwrap_or_default();
    }

    pub(in crate::app) fn commit_rename_chat(&mut self, value: String) {
        if let Some(chat_id) = self.rename_chat_id {
            let next_title = value.trim();
            if !next_title.is_empty()
                && let Some(chat) = self.state.chats.iter_mut().find(|chat| chat.id == chat_id)
            {
                chat.title = next_title.to_string();
                chat.touch();
                self.persist_state();
            }
        }
        self.rename_chat_id = None;
        self.rename_input.clear();
        self.hovered_chat_id = None;
    }

    pub(in crate::app) fn cancel_rename_chat(&mut self) {
        self.rename_chat_id = None;
        self.rename_input.clear();
    }

    pub(in crate::app) fn delete_chat_action(&mut self, chat_id: u64) {
        if self
            .inflight_request
            .as_ref()
            .map(|request| request.chat_id)
            == Some(chat_id)
        {
            self.abort_inflight_request();
            self.inflight_request = None;
            self.loading_chat_id = None;
            self.loading_phase = 0;
        }
        self.delete_chat(chat_id);
        self.hovered_chat_id = None;
        if self.chat_error.as_ref().map(|error| error.chat_id) == Some(chat_id) {
            self.chat_error = None;
        }
        self.persist_state();
    }
}
