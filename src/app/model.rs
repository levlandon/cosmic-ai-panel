//! AppModel helper methods that maintain local UI state and cached chat data.

use super::*;

impl AppModel {
    pub(in crate::app) fn reset_composer(&mut self) {
        self.composer_content = widget::text_editor::Content::new();
    }

    pub(in crate::app) fn reset_inline_edit(&mut self) {
        self.editing_message_id = None;
        self.editing_content = widget::text_editor::Content::new();
    }

    pub(in crate::app) fn set_transient_chat_notice(
        &mut self,
        chat_id: u64,
        message: impl Into<String>,
    ) {
        self.transient_chat_notice = Some(TransientChatNotice {
            chat_id,
            message: message.into(),
        });
    }

    pub(in crate::app) fn clear_transient_chat_notice(&mut self, chat_id: u64) {
        if self
            .transient_chat_notice
            .as_ref()
            .map(|notice| notice.chat_id)
            == Some(chat_id)
        {
            self.transient_chat_notice = None;
        }
    }

    pub(in crate::app) fn active_transient_chat_notice(&self) -> Option<&str> {
        self.transient_chat_notice
            .as_ref()
            .filter(|notice| notice.chat_id == self.state.active_chat_id)
            .map(|notice| notice.message.as_str())
    }

    pub(in crate::app) fn sync_message_view_content(&mut self, message_id: u64, content: &str) {
        let needs_refresh = self
            .message_view_text
            .get(&message_id)
            .map(|current| current != content)
            .unwrap_or(true);

        if needs_refresh {
            self.message_view_text
                .insert(message_id, content.to_string());
            self.message_view_content
                .insert(message_id, widget::text_editor::Content::with_text(content));
        }
    }

    pub(in crate::app) fn rebuild_message_view_cache(&mut self) {
        let mut live_ids = HashSet::new();

        let messages: Vec<(u64, String)> = self
            .state
            .chats
            .iter()
            .flat_map(|chat| {
                chat.messages
                    .iter()
                    .map(|message| (message.id, message.content.clone()))
            })
            .collect();

        for (message_id, content) in messages {
            live_ids.insert(message_id);
            self.sync_message_view_content(message_id, &content);
        }

        self.message_view_content
            .retain(|message_id, _| live_ids.contains(message_id));
        self.message_view_text
            .retain(|message_id, _| live_ids.contains(message_id));
    }

    pub(in crate::app) fn perform_message_view_action(
        &mut self,
        message_id: u64,
        action: widget::text_editor::Action,
    ) {
        use widget::text_editor::Action;

        let Some(content) = self.message_view_content.get_mut(&message_id) else {
            return;
        };

        match action {
            Action::Move(_)
            | Action::Select(_)
            | Action::SelectWord
            | Action::SelectLine
            | Action::SelectAll
            | Action::Click(_)
            | Action::Drag(_)
            | Action::Scroll { .. } => content.perform(action),
            Action::Edit(_) => {}
        }
    }

    pub(in crate::app) fn composer_text(&self) -> String {
        self.composer_content.text()
    }

    pub(in crate::app) fn active_chat(&self) -> Option<&ChatSession> {
        self.state
            .chats
            .iter()
            .find(|chat| chat.id == self.state.active_chat_id)
    }

    pub(in crate::app) fn active_chat_mut(&mut self) -> Option<&mut ChatSession> {
        self.state
            .chats
            .iter_mut()
            .find(|chat| chat.id == self.state.active_chat_id)
    }

    pub(in crate::app) fn message_content_by_id(&self, message_id: u64) -> Option<String> {
        self.active_chat()?
            .messages
            .iter()
            .find(|message| message.id == message_id)
            .map(|message| message.content.clone())
    }

    pub(in crate::app) fn should_show_empty_chat_placeholder(&self, chat: &ChatSession) -> bool {
        self.loading_chat_id != Some(chat.id)
            && !chat
                .messages
                .iter()
                .any(|message| matches!(message.role, ChatRole::User | ChatRole::Assistant))
    }

    pub(in crate::app) fn last_assistant_message_id(&self, chat_id: u64) -> Option<u64> {
        self.state
            .chats
            .iter()
            .find(|chat| chat.id == chat_id)?
            .messages
            .iter()
            .rev()
            .find(|message| message.role == ChatRole::Assistant)
            .map(|message| message.id)
    }

    pub(in crate::app) fn create_chat(&mut self) -> u64 {
        let chat_id = self.state.next_chat_id;
        self.state.next_chat_id += 1;

        let provider = self.state.settings.provider.active_provider;
        let model = self.state.settings.active_model().to_string();

        self.state
            .chats
            .push(ChatSession::new(chat_id, provider, model));
        chat_id
    }

    pub(in crate::app) fn delete_chat(&mut self, chat_id: u64) {
        self.state.chats.retain(|chat| chat.id != chat_id);

        if self.state.chats.is_empty() {
            let new_chat_id = self.create_chat();
            self.state.active_chat_id = new_chat_id;
        } else if self.state.active_chat_id == chat_id {
            self.state.active_chat_id = self.state.chats[0].id;
        }

        if self.rename_chat_id == Some(chat_id) {
            self.rename_chat_id = None;
            self.rename_input.clear();
        }

        self.rebuild_markdown_cache();
        self.rebuild_message_view_cache();
    }

    pub(in crate::app) fn persist_state(&mut self) {
        match storage::save_state(&self.state) {
            Ok(()) => {}
            Err(error) => {
                self.status = Some(format!("{}: {error}", fl!("status-save-failed")));
            }
        }
    }

    pub(in crate::app) fn next_message_id(&mut self) -> u64 {
        let message_id = self.state.next_message_id;
        self.state.next_message_id += 1;
        message_id
    }

    pub(in crate::app) fn rebuild_markdown_cache(&mut self) {
        self.assistant_markdown.clear();

        for chat in &self.state.chats {
            for message in &chat.messages {
                if message.role == ChatRole::Assistant {
                    self.assistant_markdown.insert(
                        message.id,
                        widget::markdown::Content::parse(&message.content),
                    );
                }
            }
        }
    }

    pub(in crate::app) fn user_message_text_width(&self, content: &str) -> f32 {
        type PlainParagraph = cosmic::iced_core::text::paragraph::Plain<
            <cosmic::Renderer as cosmic::iced_core::text::Renderer>::Paragraph,
        >;

        let paragraph = PlainParagraph::new(core_text::Text {
            content: content.to_string(),
            bounds: Size::new(f32::INFINITY, f32::INFINITY),
            size: Pixels(MESSAGE_TEXT_SIZE_PX),
            line_height: core_text::LineHeight::default(),
            font: Font::default(),
            align_x: core_text::Alignment::Left,
            align_y: alignment::Vertical::Top,
            shaping: core_text::Shaping::Advanced,
            wrapping: core_text::Wrapping::None,
            ellipsize: core_text::Ellipsize::None,
        });

        (paragraph.min_width() + USER_MESSAGE_EDITOR_WIDTH_BUFFER).clamp(
            1.0,
            USER_MESSAGE_BUBBLE_WIDTH - USER_MESSAGE_HORIZONTAL_PADDING,
        )
    }

    pub(in crate::app) fn has_selected_model(&self) -> bool {
        self.active_chat()
            .map(|chat| !chat.model.trim().is_empty())
            .unwrap_or(false)
    }

    pub(in crate::app) fn active_model_label(&self) -> String {
        let Some(chat) = self.active_chat() else {
            return "No model selected".into();
        };
        let model = chat.model.trim();
        if model.is_empty() {
            "No model selected".into()
        } else {
            let trimmed = model
                .rsplit('/')
                .next()
                .filter(|segment| !segment.is_empty())
                .unwrap_or(model);
            match chat.provider {
                ProviderKind::LmStudio => format!("{trimmed} (local)"),
                ProviderKind::OpenRouter => trimmed.to_string(),
            }
        }
    }

    pub(in crate::app) fn active_model_options(&self) -> Vec<String> {
        self.state
            .settings
            .provider
            .saved_models
            .iter()
            .map(SavedModel::chat_dropdown_label)
            .collect()
    }

    pub(in crate::app) fn active_model_index(&self) -> Option<usize> {
        let chat = self.active_chat()?;
        let selected = SavedModel::normalized(chat.provider, &chat.model)?;
        self.state
            .settings
            .provider
            .saved_models
            .iter()
            .position(|model| model == &selected)
    }

    pub(in crate::app) fn can_follow_chat(&self) -> bool {
        self.messages_bottom_distance <= CHAT_AUTOSCROLL_THRESHOLD_PX
    }

    pub(in crate::app) fn scroll_messages_to_end(
        &mut self,
        force: bool,
    ) -> Task<cosmic::Action<Message>> {
        if force {
            self.messages_bottom_distance = 0.0;
        }

        if force || self.can_follow_chat() {
            cosmic::iced::widget::operation::snap_to_end(self.messages_scroll_id.clone())
        } else {
            Task::none()
        }
    }

    pub(in crate::app) fn copy_to_clipboard(
        &mut self,
        target: CopiedTarget,
        content: String,
    ) -> Task<cosmic::Action<Message>> {
        self.copied_target = Some(target.clone());

        Task::batch([
            cosmic::iced::clipboard::write::<cosmic::Action<Message>>(content),
            cosmic::task::future(async move {
                tokio::time::sleep(Duration::from_secs(5)).await;
                Message::ResetCopiedTarget(target)
            }),
        ])
    }

    pub(in crate::app) fn remove_message_ids(&mut self, message_ids: &[u64]) {
        for message_id in message_ids {
            self.assistant_markdown.remove(message_id);
            self.message_view_content.remove(message_id);
            self.message_view_text.remove(message_id);
        }
        if let Some(copied) = &self.copied_target {
            let should_clear = match copied {
                CopiedTarget::Message(message_id) => message_ids.contains(message_id),
                CopiedTarget::CodeBlock { message_id, .. } => message_ids.contains(message_id),
                CopiedTarget::SettingsExport => false,
            };
            if should_clear {
                self.copied_target = None;
            }
        }
        if let Some(hovered) = self.hovered_message_id
            && message_ids.contains(&hovered)
        {
            self.hovered_message_id = None;
        }
    }
}
