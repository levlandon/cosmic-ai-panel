// SPDX-License-Identifier: MPL-2.0
//! Chat panel rendering and empty-state UI.

use super::super::*;
use super::super::style::*;
use crate::fl;
use cosmic::iced_widget::{container, row, scrollable, text};

impl AppModel {
    pub(in crate::app) fn chat_screen(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let model_selected = self.has_selected_model();
        let is_generating = self
            .inflight_request
            .as_ref()
            .map(|request| request.chat_id)
            == Some(self.state.active_chat_id);
        let composer_has_text = !self.composer_text().trim().is_empty();
        let composer_is_multiline = self.composer_content.line_count() > 1;
        let placeholder = if model_selected {
            fl!("composer-placeholder")
        } else {
            "Choose a model in settings".into()
        };

        let composer_editor_width =
            (PANEL_WIDTH - (spacing.space_m as f32 * 4.0) - (spacing.space_xs as f32) - 36.0)
                .max(160.0);

        let messages = scrollable(self.message_column())
            .anchor_bottom()
            .id(self.messages_scroll_id.clone())
            .on_scroll(Message::ChatScrolled)
            .class(cosmic::style::iced::Scrollable::Minimal)
            .direction(thin_vertical_scrollbar())
            .height(Length::Fill)
            .width(Length::Fill);

        let composer_editor = widget::text_editor(&self.composer_content)
            .id(self.composer_editor_id.clone())
            .placeholder(placeholder)
            .on_action(Message::ComposerEdited)
            .key_binding(composer_key_binding)
            .padding([11, 0, 0, 0])
            .height(Length::Shrink)
            .min_height(COMPOSER_LINE_HEIGHT + 6.0)
            .max_height(COMPOSER_MAX_HEIGHT)
            .wrapping(core_text::Wrapping::WordOrGlyph)
            .class(composer_editor_class())
            .width(composer_editor_width);

        let mut send_button = button::custom(
            container(
                widget::text::body(if is_generating { "■" } else { "↑" })
                    .size(if is_generating { 14 } else { 20 }),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
        )
        .width(Length::Fixed(36.0))
        .height(Length::Fixed(36.0))
        .padding(0)
        .class(send_button_class());

        if is_generating {
            send_button = send_button.on_press(Message::StopGeneration);
        } else if model_selected && composer_has_text {
            send_button = send_button.on_press(Message::SubmitComposer);
        }

        let composer = container(
            row![container(composer_editor).width(Length::Fill), send_button]
                .spacing(spacing.space_xs)
                .width(Length::Fill)
                .align_y(if composer_is_multiline {
                    Alignment::End
                } else {
                    Alignment::Center
                }),
        )
        .padding([spacing.space_s, spacing.space_m])
        .class(composer_container_class());

        let mut content = widget::column().spacing(spacing.space_m);
        content = content.push(messages).push(composer);

        if let Some(status) = &self.status {
            content = content.push(widget::text::caption(status));
        }

        content
            .padding(spacing.space_m)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    pub(in crate::app) fn chat_model_header(&self) -> Element<'_, Message> {
        let tone = if self.has_selected_model() {
            Color::from_rgba(1.0, 1.0, 1.0, 0.82)
        } else {
            Color::from_rgb(1.0, 0.42, 0.42)
        };

        if self.state.settings.saved_models.is_empty() {
            return widget::text::body(self.active_model_label())
                .class(cosmic::theme::Text::Color(tone))
                .width(Length::Fixed(CHAT_MODEL_DROPDOWN_WIDTH))
                .align_x(alignment::Horizontal::Center)
                .into();
        }

        widget::dropdown(
            self.active_model_options(),
            self.active_model_index(),
            Message::ActiveModelSelected,
        )
        .width(Length::Fixed(CHAT_MODEL_DROPDOWN_WIDTH))
        .padding([8, 18, 8, 16])
        .into()
    }

    pub(in crate::app) fn message_column(&self) -> Element<'_, Message> {
        let Some(chat) = self.active_chat() else {
            return container(text(""))
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        };

        if self.should_show_empty_chat_placeholder(chat) {
            return self.empty_chat_placeholder();
        }

        let spacing = cosmic::theme::spacing();
        let mut messages = widget::column()
            .spacing(spacing.space_m)
            .width(Length::Fill);
        for message in &chat.messages {
            messages = messages.push(self.message_card(message));
        }

        if let Some(notice) = self.active_transient_chat_notice() {
            messages = messages.push(self.transient_chat_notice_card(notice));
        }

        if self.chat_error.as_ref().map(|error| error.chat_id) == Some(chat.id) {
            messages = messages.push(self.error_notice());
        }

        if self.loading_chat_id == Some(chat.id) {
            messages = messages.push(self.loading_indicator());
        }

        messages.into()
    }

    pub(in crate::app) fn empty_chat_placeholder(&self) -> Element<'_, Message> {
        container(widget::text::heading("What can I help you with?").class(
            cosmic::theme::Text::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.44)),
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(alignment::Horizontal::Center)
        .align_y(Alignment::Center)
        .into()
    }
}
