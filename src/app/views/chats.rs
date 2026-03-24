// SPDX-License-Identifier: MPL-2.0
//! Chat list rendering and row-level chat actions.

use super::super::*;
use super::super::style::*;
use crate::fl;
use cosmic::iced_widget::{column, container, row, scrollable, text};

impl AppModel {
    pub(in crate::app) fn chats_screen(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let mut content = column![
            row![
                button::icon(widget::icon::from_name("go-previous-symbolic").size(16))
                    .on_press(Message::ToggleChatList),
                container(text("")).width(Length::Fill),
                button::icon(widget::icon::from_name("document-new-symbolic").size(16))
                    .on_press(Message::NewChat),
            ]
            .spacing(spacing.space_xs)
            .align_y(Alignment::Center)
        ]
        .spacing(spacing.space_s)
        .padding(spacing.space_m)
        .width(Length::Fill);

        if self.state.chats.is_empty() {
            content = content.push(widget::text::body(fl!("chat-list-empty")));
        } else {
            for chat in &self.state.chats {
                if self.rename_chat_id == Some(chat.id) {
                    content = content.push(
                        container(
                            column![
                                widget::text_input::text_input("Chat title", &self.rename_input)
                                    .padding([8, 10])
                                    .style(composer_input_class())
                                    .on_input(Message::RenameInputChanged)
                                    .on_submit(Message::CommitRenameChat),
                                row![
                                    button::icon(
                                        widget::icon::from_name("object-select-symbolic").size(16)
                                    )
                                    .on_press(Message::CommitRenameChat(self.rename_input.clone())),
                                    button::icon(
                                        widget::icon::from_name("window-close-symbolic").size(16)
                                    )
                                    .on_press(Message::CancelRenameChat),
                                ]
                                .spacing(spacing.space_s),
                            ]
                            .spacing(spacing.space_s),
                        )
                        .padding([spacing.space_s, spacing.space_m])
                        .class(chat_list_card_class()),
                    );
                    continue;
                }

                content = content.push(
                    widget::mouse_area(
                        row![
                            button::custom(
                                column![
                                    widget::text::body(&chat.title),
                                    widget::text::caption(chat.provider.label()).class(
                                        cosmic::theme::Text::Color(Color::from_rgba(
                                            1.0, 1.0, 1.0, 0.62
                                        ))
                                    ),
                                ]
                                .spacing(spacing.space_xxs)
                                .width(Length::Fill),
                            )
                            .width(Length::Fill)
                            .padding([spacing.space_s, spacing.space_m])
                            .class(chat_row_button_class(chat.id == self.state.active_chat_id))
                            .selected(chat.id == self.state.active_chat_id)
                            .on_press(Message::SelectChat(chat.id)),
                            self.chat_action_buttons(chat.id),
                        ]
                        .spacing(spacing.space_xs)
                        .align_y(Alignment::Center),
                    )
                    .on_enter(Message::ChatHovered(chat.id))
                    .on_exit(Message::ChatUnhovered(chat.id))
                    .interaction(mouse::Interaction::Pointer),
                );
            }
        }

        if let Some(status) = &self.status {
            content = content.push(widget::text::caption(status));
        }

        scrollable(content)
            .class(cosmic::style::iced::Scrollable::Minimal)
            .direction(thin_vertical_scrollbar())
            .height(Length::Fill)
            .into()
    }

    pub(in crate::app) fn chat_action_buttons(&self, chat_id: u64) -> Element<'_, Message> {
        if self.hovered_chat_id != Some(chat_id) {
            return container(text(""))
                .width(Length::Fixed(CHAT_ACTIONS_WIDTH))
                .into();
        }

        row![
            button::icon(widget::icon::from_name("edit-symbolic").size(16))
                .class(cosmic::theme::Button::Icon)
                .on_press(Message::BeginRenameChat(chat_id)),
            button::icon(widget::icon::from_name("user-trash-symbolic").size(16))
                .class(cosmic::theme::Button::Icon)
                .on_press(Message::DeleteChat(chat_id)),
        ]
        .spacing(cosmic::theme::spacing().space_xxs)
        .width(Length::Fixed(CHAT_ACTIONS_WIDTH))
        .align_y(Alignment::Center)
        .into()
    }
}
