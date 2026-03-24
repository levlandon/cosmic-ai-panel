// SPDX-License-Identifier: MPL-2.0
//! Settings-specific reusable rows, tab controls, and modal overlays.

use super::super::style::*;
use super::super::*;
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
            Some(SettingsModal::SystemPrompt) => container(
                column![
                    widget::text::heading("Edit base system prompt"),
                    container(
                        widget::text_editor(&self.settings_ui.system_prompt_content)
                            .id(self.settings_ui.system_prompt_editor_id.clone())
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
}
