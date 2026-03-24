// SPDX-License-Identifier: MPL-2.0
//! Lightweight feedback widgets such as loading and error notices.

use super::super::*;
use super::super::style::*;
use cosmic::iced_widget::{container, row, text};

impl AppModel {
    pub(in crate::app) fn loading_indicator(&self) -> Element<'_, Message> {
        let sizes = [10.0, 11.5, 13.0, 14.0, 13.0, 11.5];
        let dot_size = sizes[self.loading_phase as usize % sizes.len()];
        let frame_size = 18.0;

        container(
            container(
                container(text(""))
                    .width(Length::Fixed(dot_size))
                    .height(Length::Fixed(dot_size))
                    .class(loading_dot_class()),
            )
            .width(Length::Fixed(frame_size))
            .height(Length::Fixed(frame_size))
            .center_x(Length::Fixed(frame_size))
            .center_y(Length::Fixed(frame_size)),
        )
        .width(Length::Fill)
        .align_x(alignment::Horizontal::Left)
        .padding([0, 4])
        .into()
    }

    pub(in crate::app) fn error_notice(&self) -> Element<'_, Message> {
        let Some(error) = &self.chat_error else {
            return container(text("")).into();
        };

        let retry_button: Element<'_, Message> =
            if error.assistant_message_id.is_some() || error.request.endpoint.is_empty() {
                container(text("")).width(Length::Fixed(20.0)).into()
            } else {
                button::icon(widget::icon::from_name("view-refresh-symbolic").size(16))
                    .class(cosmic::theme::Button::Icon)
                    .on_press(Message::RetryRequest(error.chat_id))
                    .into()
            };

        let content = row![
            widget::text::body(&error.message)
                .class(cosmic::theme::Text::Color(Color::from_rgb(1.0, 0.42, 0.42)))
                .width(Length::Fill),
            retry_button,
        ]
        .spacing(cosmic::theme::spacing().space_xs)
        .align_y(Alignment::Center);

        container(content)
            .width(Length::Fill)
            .padding([12, 14])
            .class(error_notice_class())
            .into()
    }

    pub(in crate::app) fn transient_chat_notice_card(&self, notice: &str) -> Element<'_, Message> {
        container(
            container(
                widget::text::caption(notice.to_owned()).class(cosmic::theme::Text::Color(
                    Color::from_rgba(1.0, 1.0, 1.0, 0.82),
                )),
            )
            .padding([8, 12])
            .class(transient_chat_notice_class()),
        )
        .width(Length::Fill)
        .align_x(alignment::Horizontal::Center)
        .padding([0, cosmic::theme::spacing().space_s])
        .into()
    }
}
