// SPDX-License-Identifier: MPL-2.0
//! Settings screen rendering split into provider, personalization, and skills tabs.

use super::super::style::*;
use super::super::*;
use crate::fl;
use cosmic::iced_widget::{column, container, row, scrollable, stack};

impl AppModel {
    pub(in crate::app) fn settings_screen(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let actions = row![
            button::standard(fl!("save")).on_press(Message::SaveSettings),
            button::text(fl!("cancel")).on_press(Message::CloseSettings),
        ]
        .spacing(spacing.space_s);

        let tab_content = match self.settings_ui.active_tab {
            SettingsTab::Provider => self.provider_settings_content(),
            SettingsTab::Personalization => self.personalization_settings_content(),
            SettingsTab::Skills => self.skills_settings_content(),
        };

        let base = scrollable(
            column![
                button::icon(widget::icon::from_name("go-previous-symbolic").size(16))
                    .on_press(Message::CloseSettings),
                self.settings_tab_bar(),
                tab_content,
                actions,
            ]
            .spacing(spacing.space_m)
            .padding(spacing.space_m)
            .width(Length::Fill),
        )
        .height(Length::Fill);

        if let Some(modal) = self.settings_modal_overlay() {
            stack![base, modal]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            base.into()
        }
    }

    fn provider_settings_content(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let provider_options = SettingsForm::provider_labels();
        let mut test_button = button::standard("Test connection");
        if !matches!(
            self.settings_ui.connection_test_state,
            ConnectionTestState::Testing
        ) {
            test_button = test_button.on_press(Message::TestConnection);
        }

        let provider_control = widget::dropdown(
            provider_options,
            Some(self.settings_ui.form.provider_index),
            Message::ProviderSelected,
        )
        .padding([8, 0, 8, 16]);

        let provider_section = match self.settings_ui.form.provider() {
            ProviderKind::OpenRouter => widget::settings::section()
                .title("Provider")
                .add(widget::settings::item("Provider", provider_control))
                .add(widget::settings::item(
                    "API key",
                    column![
                        container(
                            widget::text_input::secure_input(
                                "sk-or-...",
                                &self.settings_ui.form.openrouter_api_key,
                                None,
                                true,
                            )
                            .on_input(Message::OpenRouterKeyChanged)
                        )
                        .width(Length::Fill),
                        container(test_button)
                            .width(Length::Fill)
                            .align_x(alignment::Horizontal::Right),
                    ]
                    .spacing(spacing.space_s)
                    .width(Length::Fill),
                ))
                .add_maybe(self.settings_connection_status()),
            ProviderKind::LmStudio => widget::settings::section()
                .title("Provider")
                .add(widget::settings::item("Provider", provider_control))
                .add(widget::settings::item(
                    "Endpoint",
                    column![
                        container(
                            widget::text_input::text_input(
                                "http://127.0.0.1:1234",
                                &self.settings_ui.form.lmstudio_base_url,
                            )
                            .on_input(Message::LmStudioUrlChanged)
                        )
                        .width(Length::Fill),
                        container(test_button)
                            .width(Length::Fill)
                            .align_x(alignment::Horizontal::Right),
                    ]
                    .spacing(spacing.space_s)
                    .width(Length::Fill),
                ))
                .add_maybe(self.settings_connection_status()),
        };

        let timeout_invalid = self
            .settings_ui
            .form
            .timeout_seconds
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .is_none();
        let retry_attempts_invalid = self
            .settings_ui
            .form
            .retry_attempts
            .trim()
            .parse::<u8>()
            .is_err();
        let retry_delay_invalid = self
            .settings_ui
            .form
            .retry_delay_seconds
            .trim()
            .parse::<u64>()
            .is_err();
        let context_limit_invalid = self
            .settings_ui
            .form
            .context_message_limit
            .trim()
            .parse::<usize>()
            .is_err();

        let mut network_section = widget::settings::section()
            .title("Network")
            .add(widget::settings::item(
                "Timeout (seconds)",
                widget::text_input::text_input("20", &self.settings_ui.form.timeout_seconds)
                    .on_input(Message::ProviderTimeoutChanged),
            ))
            .add(widget::settings::item(
                "Retry attempts",
                widget::text_input::text_input("1", &self.settings_ui.form.retry_attempts)
                    .on_input(Message::ProviderRetryAttemptsChanged),
            ))
            .add(widget::settings::item(
                "Retry delay (seconds)",
                widget::text_input::text_input("2", &self.settings_ui.form.retry_delay_seconds)
                    .on_input(Message::ProviderRetryDelayChanged),
            ));

        if timeout_invalid {
            network_section =
                network_section.add(invalid_caption("Timeout must be a positive whole number"));
        }
        if retry_attempts_invalid {
            network_section =
                network_section.add(invalid_caption("Retry attempts must be a whole number"));
        }
        if retry_delay_invalid {
            network_section =
                network_section.add(invalid_caption("Retry delay must be a whole number"));
        }

        let mut saved_models_list = widget::column().spacing(spacing.space_s);
        for (index, model) in self.settings_ui.form.saved_models.iter().enumerate() {
            saved_models_list = saved_models_list.push(self.saved_model_row(index, model));
        }

        let saved_models_list: Element<'_, Message> =
            if self.settings_ui.form.saved_models.len() > 5 {
                scrollable(saved_models_list)
                    .class(cosmic::style::iced::Scrollable::Minimal)
                    .direction(thin_vertical_scrollbar())
                    .height(Length::Fixed(280.0))
                    .into()
            } else {
                saved_models_list.into()
            };

        let saved_models_section = widget::settings::section()
            .title("Saved models")
            .add(saved_models_list)
            .add(button::standard("Add model").on_press(Message::OpenAddModelModal));

        let default_model_section = if self.settings_ui.form.saved_models.is_empty() {
            widget::settings::section().title("Default model").add(
                widget::text::caption("Add at least one model").class(cosmic::theme::Text::Color(
                    Color::from_rgba(1.0, 1.0, 1.0, 0.56),
                )),
            )
        } else {
            let options = self.settings_ui.form.default_model_options();
            widget::settings::section()
                .title("Default model")
                .add(widget::settings::item(
                    "Default model",
                    widget::dropdown(
                        options,
                        self.settings_ui.form.default_model_index(),
                        Message::DefaultModelSelected,
                    )
                    .padding([8, 0, 8, 16]),
                ))
        };

        let mut context_section = widget::settings::section()
            .title("Context")
            .add(widget::settings::item(
                "Max messages in context",
                widget::text_input::text_input(
                    "0 = unlimited",
                    &self.settings_ui.form.context_message_limit,
                )
                .on_input(Message::ContextLimitChanged),
            ))
            .add(
                widget::text::caption("0 = unlimited").class(cosmic::theme::Text::Color(
                    Color::from_rgba(1.0, 1.0, 1.0, 0.56),
                )),
            );

        if context_limit_invalid {
            context_section =
                context_section.add(invalid_caption("Context limit must be a whole number"));
        }

        widget::settings::view_column(vec![
            provider_section.into(),
            network_section.into(),
            saved_models_section.into(),
            default_model_section.into(),
            context_section.into(),
        ])
        .into()
    }

    fn personalization_settings_content(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let prompt_actions = row![
            button::standard("Edit prompt").on_press(Message::OpenSystemPromptModal),
            button::text("Preview prompt").on_press(Message::OpenPromptPreviewModal),
        ]
        .spacing(spacing.space_s)
        .width(Length::Fill);

        let prompt_section = widget::settings::section()
            .title("System prompt")
            .add(widget::settings::item("Base system prompt", prompt_actions));

        let preference_labels = SettingsForm::preference_labels();
        let response_style_summary = compact_text_preview(&self.settings_ui.form.response_style);
        let more_about_you_summary = compact_text_preview(&self.settings_ui.form.more_about_you);

        let profile_section = widget::settings::section()
            .title("Profile")
            .add(widget::settings::item(
                "Name",
                full_width_text_input("Optional", &self.settings_ui.form.profile_name)
                    .on_input(Message::ProfileNameChanged),
            ))
            .add(widget::settings::item(
                "Preferred language",
                full_width_text_input("Optional", &self.settings_ui.form.profile_language)
                    .on_input(Message::ProfileLanguageChanged),
            ))
            .add(widget::settings::item(
                "Occupation",
                container(
                    widget::text_editor(&self.settings_ui.occupation_content)
                        .id(self.settings_ui.occupation_editor_id.clone())
                        .on_action(Message::ProfileOccupationEdited)
                        .padding([8, 0])
                        .height(Length::Fixed(88.0))
                        .wrapping(core_text::Wrapping::WordOrGlyph)
                        .class(composer_editor_class()),
                )
                .padding([spacing.space_s, spacing.space_m])
                .width(Length::Fill)
                .class(composer_container_class()),
            ))
            .add(widget::settings::item(
                "Response style",
                column![
                    button::standard("Edit response style")
                        .on_press(Message::OpenResponseStyleModal),
                    widget::text::caption(response_style_summary).class(
                        cosmic::theme::Text::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.56))
                    ),
                ]
                .spacing(spacing.space_xxs)
                .width(Length::Fill),
            ))
            .add(widget::settings::item(
                "More about you",
                column![
                    button::standard("Edit more about you")
                        .on_press(Message::OpenMoreAboutYouModal),
                    widget::text::caption(more_about_you_summary).class(
                        cosmic::theme::Text::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.56))
                    ),
                ]
                .spacing(spacing.space_xxs)
                .width(Length::Fill),
            ))
            .add(widget::settings::item(
                "Header & Lists",
                container(
                    widget::dropdown(
                        preference_labels,
                        Some(self.settings_ui.form.header_lists_index),
                        Message::HeaderListsSelected,
                    )
                    .padding([8, 0, 8, 16]),
                )
                .width(Length::Fill),
            ))
            .add(widget::settings::item(
                "Emoji",
                container(
                    widget::dropdown(
                        preference_labels,
                        Some(self.settings_ui.form.emoji_index),
                        Message::EmojiSelected,
                    )
                    .padding([8, 0, 8, 16]),
                )
                .width(Length::Fill),
            ));

        let mut memory_rows = widget::column().spacing(spacing.space_s);
        for (index, item) in self.settings_ui.form.memory_items.iter().enumerate() {
            memory_rows = memory_rows.push(self.memory_item_row(index, item));
        }

        let memory_content: Element<'_, Message> = if self.settings_ui.form.memory_items.is_empty()
        {
            widget::text::caption("No manual memory yet")
                .class(cosmic::theme::Text::Color(Color::from_rgba(
                    1.0, 1.0, 1.0, 0.56,
                )))
                .into()
        } else {
            memory_rows.into()
        };

        let memory_section = widget::settings::section()
            .title("Memory")
            .add(memory_content)
            .add(button::standard("Add memory item").on_press(Message::AddMemoryItem));

        let management_section = widget::settings::section()
            .title("Manage personalization")
            .add(
                row![
                    button::standard("Import profile")
                        .on_press(Message::OpenImportPersonalizationModal),
                    button::standard("Export profile")
                        .on_press(Message::OpenExportPersonalizationModal),
                    button::text("Reset personalization").on_press(Message::ResetPersonalization),
                ]
                .spacing(spacing.space_s)
                .width(Length::Fill),
            );

        widget::settings::view_column(vec![
            prompt_section.into(),
            profile_section.into(),
            memory_section.into(),
            management_section.into(),
        ])
        .into()
    }

    fn skills_settings_content(&self) -> Element<'_, Message> {
        let section = widget::settings::section()
            .title("Skills")
            .add(widget::settings::item(
                "datetime",
                widget::toggler(self.settings_ui.form.skill_datetime)
                    .on_toggle(Message::SkillDatetimeToggled),
            ))
            .add(widget::settings::item(
                "clipboard",
                widget::toggler(self.settings_ui.form.skill_clipboard)
                    .on_toggle(Message::SkillClipboardToggled),
            ))
            .add(widget::settings::item(
                "filesystem",
                widget::toggler(self.settings_ui.form.skill_filesystem)
                    .on_toggle(Message::SkillFilesystemToggled),
            ))
            .add(
                widget::text::caption("UI placeholder only. Backend execution is not enabled yet.")
                    .class(cosmic::theme::Text::Color(Color::from_rgba(
                        1.0, 1.0, 1.0, 0.56,
                    ))),
            );

        widget::settings::view_column(vec![section.into()]).into()
    }
}

fn full_width_text_input<'a>(
    placeholder: &'a str,
    value: &'a str,
) -> widget::text_input::TextInput<'a, Message> {
    widget::text_input::text_input(placeholder, value).width(Length::Fill)
}

fn compact_text_preview(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "Not set".into()
    } else {
        let single_line = trimmed.lines().next().unwrap_or(trimmed).trim();
        let preview: String = single_line.chars().take(72).collect();
        if single_line.chars().count() > 72 || trimmed.lines().nth(1).is_some() {
            format!("{preview}...")
        } else {
            preview
        }
    }
}

fn invalid_caption(content: &'static str) -> Element<'static, Message> {
    widget::text::caption(content)
        .class(cosmic::theme::Text::Color(Color::from_rgb(1.0, 0.42, 0.42)))
        .into()
}
