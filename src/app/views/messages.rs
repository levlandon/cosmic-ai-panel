// SPDX-License-Identifier: MPL-2.0
//! Message rendering helpers, action rows, and markdown viewer widgets.

use super::super::style::*;
use super::super::*;
use cosmic::iced_widget::{column, container, rich_text, row, scrollable};

impl AppModel {
    pub(in crate::app) fn message_viewer<'a>(
        &'a self,
        message: &'a ChatMessage,
    ) -> Option<Element<'a, Message>> {
        let content = self.message_view_content.get(&message.id)?;
        let color = match message.role {
            ChatRole::User => Color::WHITE,
            _ => Color::from_rgb(0.93, 0.93, 0.93),
        };
        let width = match message.role {
            ChatRole::User => self.user_message_text_width(&message.content),
            _ => MESSAGE_MAX_WIDTH,
        };

        Some(
            widget::text_editor(content)
                .placeholder("")
                .on_action(move |action| Message::ViewerEdited(message.id, action))
                .padding([0, 0])
                .height(Length::Shrink)
                .min_height(COMPOSER_LINE_HEIGHT + 4.0)
                .wrapping(core_text::Wrapping::WordOrGlyph)
                .class(message_viewer_class(color))
                .width(width)
                .into(),
        )
    }

    pub(in crate::app) fn message_card<'a>(
        &'a self,
        message: &'a ChatMessage,
    ) -> Element<'a, Message> {
        let spacing = cosmic::theme::spacing();
        let side_gutter = spacing.space_s;
        let is_editing = self.editing_message_id == Some(message.id);
        let base: Element<'a, Message> = match message.role {
            ChatRole::User => {
                let text_block: Element<'a, Message> = if is_editing {
                    container(
                        widget::text_editor(&self.editing_content)
                            .id(self.editing_editor_id.clone())
                            .placeholder("")
                            .on_action(Message::InlineEditEdited)
                            .key_binding(message_edit_key_binding)
                            .padding([0, 0])
                            .height(Length::Shrink)
                            .min_height(COMPOSER_LINE_HEIGHT + 6.0)
                            .max_height(COMPOSER_MAX_HEIGHT)
                            .wrapping(core_text::Wrapping::WordOrGlyph)
                            .class(composer_editor_class())
                            .width(self.user_message_text_width(&self.editing_content.text())),
                    )
                    .padding([spacing.space_s, spacing.space_m])
                    .width(Length::Shrink)
                    .max_width(USER_MESSAGE_BUBBLE_WIDTH)
                    .into()
                } else {
                    container(self.message_viewer(message).unwrap_or_else(|| {
                        widget::text::body(&message.content)
                            .class(cosmic::theme::Text::Color(Color::WHITE))
                            .wrapping(cosmic::iced::widget::text::Wrapping::Word)
                            .into()
                    }))
                    .padding([spacing.space_s, spacing.space_m])
                    .width(Length::Shrink)
                    .max_width(USER_MESSAGE_BUBBLE_WIDTH)
                    .into()
                };

                container(text_block)
                    .width(Length::Fill)
                    .align_x(alignment::Horizontal::Right)
                    .padding([0, side_gutter, 0, side_gutter])
                    .into()
            }
            ChatRole::Assistant => {
                let markdown_theme = if cosmic::theme::is_dark() {
                    cosmic::iced::Theme::Dark
                } else {
                    cosmic::iced::Theme::Light
                };
                let markdown_viewer = AssistantMarkdownViewer {
                    message_id: message.id,
                    copied_target: self.copied_target.as_ref(),
                    next_code_block_index: Cell::new(0),
                };
                let assistant_content: Element<'a, Message> =
                    if let Some(markdown) = self.assistant_markdown.get(&message.id) {
                        widget::markdown::view_with(
                            markdown.items(),
                            widget::markdown::Settings::with_style(markdown_theme),
                            &markdown_viewer,
                        )
                    } else {
                        widget::text::body(&message.content)
                            .wrapping(cosmic::iced::widget::text::Wrapping::Word)
                            .into()
                    };

                let bubble = container(assistant_content)
                    .padding([spacing.space_s, spacing.space_m])
                    .max_width(MESSAGE_MAX_WIDTH)
                    .class(chat_bubble_class(false));

                container(bubble)
                    .width(Length::Fill)
                    .align_x(alignment::Horizontal::Left)
                    .padding([0, side_gutter, 0, side_gutter])
                    .into()
            }
            ChatRole::System => {
                let bubble = container(widget::text::body(&message.content))
                    .padding([spacing.space_s, spacing.space_m])
                    .max_width(MESSAGE_MAX_WIDTH)
                    .class(chat_bubble_class(false));

                container(bubble)
                    .width(Length::Fill)
                    .align_x(alignment::Horizontal::Left)
                    .padding([0, side_gutter, 0, side_gutter])
                    .into()
            }
        };

        let action_gap = if message.role == ChatRole::User {
            spacing.space_xxxs
        } else {
            spacing.space_xxs
        };
        let mut content = widget::column().spacing(action_gap).push(base);
        if let Some(action_row) = self.message_actions_row(message) {
            content = content.push(action_row);
        }

        let content: Element<'a, Message> = content.width(Length::Fill).into();

        if message.role == ChatRole::User {
            widget::mouse_area(content)
                .on_enter(Message::HoverMessageRow(message.id))
                .on_exit(Message::LeaveMessageRow(message.id))
                .interaction(mouse::Interaction::Idle)
                .into()
        } else {
            content
        }
    }

    pub(in crate::app) fn message_actions_row<'a>(
        &'a self,
        message: &'a ChatMessage,
    ) -> Option<Element<'a, Message>> {
        let spacing = cosmic::theme::spacing();
        let side_gutter = spacing.space_s;
        let chat = self.active_chat()?;

        let is_last_assistant = self.last_assistant_message_id(chat.id) == Some(message.id);
        let streaming_assistant_id = self
            .inflight_request
            .as_ref()
            .and_then(|request| {
                (request.chat_id == chat.id).then_some(request.assistant_message_id)
            })
            .flatten();
        let failed_assistant_id = self
            .chat_error
            .as_ref()
            .and_then(|error| (error.chat_id == chat.id).then_some(error.assistant_message_id))
            .flatten();
        let actions_locked = self.inflight_request.is_some();
        let user_actions_visible = self.hovered_message_id == Some(message.id)
            || self.editing_message_id == Some(message.id);
        let copy_icon = if matches!(
            self.copied_target.as_ref(),
            Some(CopiedTarget::Message(copied_id)) if *copied_id == message.id
        ) {
            "object-select-symbolic"
        } else {
            "edit-copy-symbolic"
        };

        let actions: Vec<Element<'a, Message>> = match message.role {
            ChatRole::User => {
                if self.editing_message_id == Some(message.id) {
                    vec![
                        self.message_action_button(
                            "object-select-symbolic",
                            Some(Message::SaveInlineEdit),
                            true,
                            true,
                        ),
                        self.message_action_button(
                            "window-close-symbolic",
                            Some(Message::CancelInlineEdit),
                            true,
                            true,
                        ),
                    ]
                } else {
                    let edit_button: Element<'a, Message> = if !actions_locked {
                        self.message_action_button(
                            "edit-symbolic",
                            Some(Message::BeginUserEdit(message.id)),
                            user_actions_visible,
                            true,
                        )
                    } else {
                        self.message_action_button("edit-symbolic", None, false, true)
                    };

                    vec![
                        edit_button,
                        self.message_action_button(
                            copy_icon,
                            Some(Message::CopyEntry(message.id)),
                            user_actions_visible,
                            true,
                        ),
                    ]
                }
            }
            ChatRole::Assistant => {
                if streaming_assistant_id == Some(message.id) {
                    return None;
                }

                if failed_assistant_id == Some(message.id) {
                    return Some(
                        container(
                            row![self.message_action_button(
                                "view-refresh-symbolic",
                                Some(Message::RegenerateLastAssistant(message.id)),
                                true,
                                false,
                            )]
                            .spacing(cosmic::theme::spacing().space_xs)
                            .align_y(Alignment::Center),
                        )
                        .width(Length::Fill)
                        .align_x(alignment::Horizontal::Left)
                        .padding([0, side_gutter, 0, side_gutter])
                        .into(),
                    );
                }

                let mut buttons = vec![self.message_action_button(
                    copy_icon,
                    Some(Message::CopyEntry(message.id)),
                    true,
                    false,
                )];

                buttons.push(self.message_action_button(
                    "object-merge-symbolic",
                    (!actions_locked).then_some(Message::BranchConversation(message.id)),
                    true,
                    false,
                ));

                if is_last_assistant && !actions_locked {
                    buttons.push(self.message_action_button(
                        "view-refresh-symbolic",
                        Some(Message::RegenerateLastAssistant(message.id)),
                        true,
                        false,
                    ));
                    buttons.push(self.message_action_button(
                        "user-trash-symbolic",
                        Some(Message::DeleteLastAssistant(message.id)),
                        true,
                        false,
                    ));
                }

                buttons
            }
            ChatRole::System => return None,
        };

        let action_row = row(actions)
            .spacing(if message.role == ChatRole::User {
                cosmic::theme::spacing().space_xxxs
            } else {
                cosmic::theme::spacing().space_xs
            })
            .align_y(Alignment::Center);

        Some(match message.role {
            ChatRole::User => container(action_row)
                .width(Length::Fill)
                .align_x(alignment::Horizontal::Right)
                .padding([0, side_gutter, 0, side_gutter])
                .into(),
            _ => container(action_row)
                .width(Length::Fill)
                .align_x(alignment::Horizontal::Left)
                .padding([0, side_gutter, 0, side_gutter])
                .into(),
        })
    }

    pub(in crate::app) fn message_action_button(
        &self,
        icon_name: &'static str,
        on_press: Option<Message>,
        visible: bool,
        compact: bool,
    ) -> Element<'_, Message> {
        let icon_size = if compact { 14 } else { 16 };
        let frame_size = if compact { 24.0 } else { 28.0 };
        let mut button = button::icon(widget::icon::from_name(icon_name).size(icon_size))
            .class(message_action_button_class(visible));

        if visible && let Some(on_press) = on_press {
            button = button.on_press(on_press);
        }

        container(button)
            .width(Length::Fixed(frame_size))
            .center_x(Length::Fixed(frame_size))
            .into()
    }
}

struct AssistantMarkdownViewer<'a> {
    message_id: u64,
    copied_target: Option<&'a CopiedTarget>,
    next_code_block_index: Cell<usize>,
}

impl<'a> widget::markdown::Viewer<'a, Message, cosmic::Theme, cosmic::Renderer>
    for AssistantMarkdownViewer<'a>
{
    fn on_link_click(url: widget::markdown::Uri) -> Message {
        Message::MarkdownLink(url)
    }

    fn code_block(
        &self,
        settings: widget::markdown::Settings,
        _language: Option<&'a str>,
        code: &'a str,
        lines: &'a [widget::markdown::Text],
    ) -> Element<'a, Message> {
        let block_index = self.next_code_block_index.get();
        self.next_code_block_index.set(block_index + 1);

        let copied = matches!(
            self.copied_target,
            Some(CopiedTarget::CodeBlock {
                message_id,
                block_index: copied_block_index,
            }) if *message_id == self.message_id && *copied_block_index == block_index
        );
        let copy_icon = if copied {
            "object-select-symbolic"
        } else {
            "edit-copy-symbolic"
        };

        let copy_button = button::icon(widget::icon::from_name(copy_icon).size(14))
            .class(cosmic::theme::Button::Icon)
            .on_press(Message::CopyCodeBlock {
                message_id: self.message_id,
                block_index,
                content: code.to_owned(),
            });

        let header = row![
            widget::space::horizontal(),
            container(copy_button).width(Length::Shrink),
        ]
        .align_y(Alignment::Center);

        let code_lines = column(lines.iter().map(|line| {
            rich_text(line.spans(settings.style))
                .on_link_click(Message::MarkdownLink)
                .font(settings.style.code_block_font)
                .size(settings.code_size)
                .into()
        }));

        container(
            column![
                header,
                scrollable(container(code_lines).padding(settings.code_size)).direction(
                    cosmic::iced::widget::scrollable::Direction::Horizontal(
                        cosmic::iced::widget::scrollable::Scrollbar::default()
                            .width(settings.code_size / 2)
                            .scroller_width(settings.code_size / 2),
                    )
                ),
            ]
            .spacing(settings.spacing / 4.0),
        )
        .width(Length::Fill)
        .padding(settings.code_size / 4)
        .class(<cosmic::Theme as widget::markdown::Catalog>::code_block())
        .into()
    }
}
