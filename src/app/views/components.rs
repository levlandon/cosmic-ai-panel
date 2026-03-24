// SPDX-License-Identifier: MPL-2.0
//! Reusable view helpers for message rendering, notices, and settings widgets.

use super::super::*;
use super::super::style::*;
use cosmic::iced_widget::{column, container, rich_text, row, scrollable, text};

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
                .on_action(move |action| Message::MessageViewerEdited(message.id, action))
                .padding([0, 0])
                .height(Length::Shrink)
                .min_height(COMPOSER_LINE_HEIGHT + 4.0)
                .wrapping(core_text::Wrapping::WordOrGlyph)
                .class(message_viewer_class(color))
                .width(width)
                .into(),
        )
    }

    pub(in crate::app) fn settings_connection_status(&self) -> Option<Element<'_, Message>> {
        let text = match &self.connection_test_state {
            ConnectionTestState::Idle => return None,
            ConnectionTestState::Testing => {
                return Some(
                    widget::text::caption("Testing connection...")
                        .class(cosmic::theme::Text::Color(Color::from_rgba(
                            1.0, 1.0, 1.0, 0.62,
                        )))
                        .into(),
                );
            }
            ConnectionTestState::Success => widget::text::caption("Connection OK")
                .class(cosmic::theme::Text::Color(Color::from_rgb(0.48, 0.9, 0.62)))
                .into(),
            ConnectionTestState::Failed(error) => column![
                widget::text::caption("Connection failed")
                    .class(cosmic::theme::Text::Color(Color::from_rgb(1.0, 0.42, 0.42))),
                widget::text::caption(error.clone()).class(cosmic::theme::Text::Color(
                    Color::from_rgba(1.0, 1.0, 1.0, 0.56)
                )),
            ]
            .spacing(4)
            .into(),
        };

        Some(text)
    }

    pub(in crate::app) fn saved_model_row(
        &self,
        index: usize,
        model: &SavedModel,
    ) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let is_default = self.settings_form.default_model.as_ref() == Some(model);

        let meta = if is_default {
            format!("{} · Default", model.provider.label())
        } else {
            model.provider.label().to_string()
        };

        container(
            row![
                column![
                    widget::text::body(model.name.clone()),
                    widget::text::caption(meta).class(cosmic::theme::Text::Color(
                        Color::from_rgba(1.0, 1.0, 1.0, 0.56)
                    )),
                ]
                .spacing(4)
                .width(Length::Fill),
                button::icon(widget::icon::from_name("window-close-symbolic").size(14))
                    .on_press(Message::RemoveSavedModel(index)),
            ]
            .spacing(spacing.space_s)
            .align_y(Alignment::Center),
        )
        .padding([spacing.space_s, spacing.space_m])
        .class(chat_list_card_class())
        .into()
    }

    pub(in crate::app) fn settings_modal_overlay(&self) -> Option<Element<'_, Message>> {
        let spacing = cosmic::theme::spacing();

        let card: Element<'_, Message> = match self.settings_modal {
            Some(SettingsModal::AddModel) => {
                let provider_options = SettingsForm::provider_labels();
                let mut save_button = button::standard("Save");
                if !self.add_model_name.trim().is_empty() {
                    save_button = save_button.on_press(Message::SaveAddedModel);
                }

                container(
                    column![
                        widget::text::heading("Add model"),
                        widget::settings::section()
                            .add(widget::settings::item(
                                "Provider",
                                widget::dropdown(
                                    provider_options,
                                    Some(self.add_model_provider_index),
                                    Message::AddModelProviderSelected,
                                )
                                .padding([8, 0, 8, 16]),
                            ))
                            .add(
                                column![
                                    widget::text::caption("Model name").class(
                                        cosmic::theme::Text::Color(Color::from_rgba(
                                            1.0, 1.0, 1.0, 0.62
                                        ))
                                    ),
                                    widget::text_input::text_input(
                                        "openrouter/free or deepseek-chat",
                                        &self.add_model_name,
                                    )
                                    .on_input(Message::AddModelNameChanged),
                                ]
                                .spacing(spacing.space_xxs),
                            ),
                        row![
                            save_button,
                            button::text("Cancel").on_press(Message::CloseSettingsModal),
                        ]
                        .spacing(spacing.space_s),
                    ]
                    .spacing(spacing.space_m),
                )
                .padding(spacing.space_m)
                .width(Length::Fixed(PANEL_WIDTH - 48.0))
                .class(chat_list_card_class())
                .into()
            }
            Some(SettingsModal::SystemPrompt) => container(
                column![
                    widget::text::heading("Edit system prompt"),
                    container(
                        widget::text_editor(&self.system_prompt_content)
                            .id(self.system_prompt_editor_id.clone())
                            .on_action(Message::SystemPromptEdited)
                            .padding([8, 0])
                            .height(Length::Fixed(220.0))
                            .wrapping(core_text::Wrapping::WordOrGlyph)
                            .class(composer_editor_class())
                    )
                    .padding([spacing.space_s, spacing.space_m])
                    .class(composer_container_class()),
                    row![
                        button::standard("Save").on_press(Message::SaveSystemPrompt),
                        button::text("Cancel").on_press(Message::CloseSettingsModal),
                    ]
                    .spacing(spacing.space_s),
                ]
                .spacing(spacing.space_m),
            )
            .padding(spacing.space_m)
            .width(Length::Fixed(PANEL_WIDTH - 32.0))
            .class(chat_list_card_class())
            .into(),
            None => return None,
        };

        Some(
            container(card)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .padding(spacing.space_m)
                .class(settings_modal_backdrop_class())
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
                .on_enter(Message::MessageHovered(message.id))
                .on_exit(Message::MessageUnhovered(message.id))
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
                            Some(Message::SaveEditedMessage),
                            true,
                            true,
                        ),
                        self.message_action_button(
                            "window-close-symbolic",
                            Some(Message::CancelEditedMessage),
                            true,
                            true,
                        ),
                    ]
                } else {
                    let edit_button: Element<'a, Message> = if !actions_locked {
                        self.message_action_button(
                            "edit-symbolic",
                            Some(Message::EditUserMessage(message.id)),
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
                            Some(Message::CopyMessage(message.id)),
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
                    Some(Message::CopyMessage(message.id)),
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
