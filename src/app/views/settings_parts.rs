// SPDX-License-Identifier: MPL-2.0
//! Settings-specific reusable rows, tab controls, and modal overlays.

use super::super::style::*;
use super::super::*;
use crate::runtime::context::builder::build_prompt_preview;
use cosmic::iced_widget::{column, container, row};

impl AppModel {
    pub(in crate::app) fn settings_tab_bar(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let mut tabs = row![].spacing(spacing.space_xs).width(Length::Fill);

        for tab in SettingsTab::ALL {
            let button = if self.settings_ui.active_tab == tab {
                button::standard(tab.label())
            } else {
                button::text(tab.label())
            }
            .on_press(Message::SettingsTabSelected(tab));

            tabs = tabs.push(button);
        }

        tabs.into()
    }

    pub(in crate::app) fn settings_connection_status(&self) -> Option<Element<'_, Message>> {
        let text = match &self.settings_ui.connection_test_state {
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
        let is_default = self.settings_ui.form.default_model.as_ref() == Some(model);

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

    pub(in crate::app) fn memory_item_row<'a>(
        &'a self,
        index: usize,
        value: &'a str,
    ) -> Element<'a, Message> {
        let spacing = cosmic::theme::spacing();

        container(
            row![
                widget::text_input::text_input("Remember this about the user", value)
                    .on_input(move |next| Message::MemoryItemChanged(index, next))
                    .width(Length::Fill),
                button::icon(widget::icon::from_name("window-close-symbolic").size(14))
                    .on_press(Message::RemoveMemoryItem(index)),
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

        let card: Element<'_, Message> = match self.settings_ui.modal {
            Some(SettingsModal::AddModel) => {
                let provider_options = SettingsForm::provider_labels();
                let mut save_button = button::standard("Save");
                if !self.settings_ui.add_model_name.trim().is_empty() {
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
                                    Some(self.settings_ui.add_model_provider_index),
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
                                        &self.settings_ui.add_model_name,
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
            Some(SettingsModal::Editor(kind)) => self.settings_text_editor_modal(kind),
            Some(SettingsModal::PromptPreview) => self.prompt_preview_modal(),
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

    fn settings_text_editor_modal(&self, kind: TextEditorModal) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let editor = widget::text_editor(&self.settings_ui.modal_editor_content)
            .id(self.settings_ui.modal_editor_id.clone())
            .padding([8, 0])
            .height(Length::Fixed(240.0))
            .wrapping(core_text::Wrapping::WordOrGlyph)
            .class(composer_editor_class());
        let editor = if kind.is_read_only() {
            editor
        } else {
            editor.on_action(Message::SettingsModalEdited)
        };

        let mut actions = row![].spacing(spacing.space_s);
        if let Some(label) = kind.action_label() {
            actions = actions.push(button::standard(label).on_press(Message::SaveSettingsModal));
        }
        if matches!(kind, TextEditorModal::ExportPersonalization) {
            let copy_label = if self.copied_target == Some(CopiedTarget::SettingsExport) {
                "Copied"
            } else {
                "Copy"
            };
            actions = actions
                .push(button::standard(copy_label).on_press(Message::CopyExportedPersonalization));
        }
        let close_label = if kind.is_read_only() {
            "Close"
        } else {
            "Cancel"
        };
        actions = actions.push(button::text(close_label).on_press(Message::CloseSettingsModal));

        let mut content = column![
            widget::text::heading(kind.title()),
            container(editor)
                .padding([spacing.space_s, spacing.space_m])
                .class(composer_container_class()),
        ]
        .spacing(spacing.space_m);

        if let Some(error) = &self.settings_ui.modal_error {
            content = content.push(
                widget::text::caption(error.clone())
                    .class(cosmic::theme::Text::Color(Color::from_rgb(1.0, 0.42, 0.42))),
            );
        }

        container(content.push(actions))
            .padding(spacing.space_m)
            .width(Length::Fixed(PANEL_WIDTH - 32.0))
            .class(chat_list_card_class())
            .into()
    }

    fn prompt_preview_modal(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let personalization = self.settings_ui.form.personalization_settings();
        let preview = build_prompt_preview(
            &personalization.base_system_prompt,
            &personalization.profile,
            &personalization.memory,
        );
        let is_code = self.settings_ui.preview_mode == PromptPreviewMode::Code;
        let body = if is_code { preview.code } else { preview.text };
        let font = if is_code {
            cosmic::font::mono()
        } else {
            cosmic::font::default()
        };
        let mut mode_switch = row![].spacing(spacing.space_xs);
        for mode in PromptPreviewMode::ALL {
            let button = if self.settings_ui.preview_mode == mode {
                button::standard(mode.label())
            } else {
                button::text(mode.label())
            }
            .on_press(Message::PromptPreviewModeSelected(mode));
            mode_switch = mode_switch.push(button);
        }

        container(
            column![
                widget::text::heading("Preview prompt"),
                mode_switch,
                container(
                    cosmic::widget::scrollable(
                        widget::text::body(body).font(font).width(Length::Fill)
                    )
                    .height(Length::Fixed(260.0))
                    .class(cosmic::style::iced::Scrollable::Minimal)
                    .direction(thin_vertical_scrollbar())
                )
                .padding([spacing.space_s, spacing.space_m])
                .class(composer_container_class()),
                row![button::text("Close").on_press(Message::CloseSettingsModal)]
                    .spacing(spacing.space_s),
            ]
            .spacing(spacing.space_m),
        )
        .padding(spacing.space_m)
        .width(Length::Fixed(PANEL_WIDTH - 32.0))
        .class(chat_list_card_class())
        .into()
    }
}
