// SPDX-License-Identifier: MPL-2.0
//! Settings screen rendering and modal overlays.

use super::super::*;
use super::super::style::*;
use crate::fl;
use cosmic::iced_widget::{column, container, row, scrollable, stack};

impl AppModel {
    pub(in crate::app) fn settings_screen(&self) -> Element<'_, Message> {
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

        let mut saved_models_list = widget::column().spacing(spacing.space_s);
        for (index, model) in self.settings_ui.form.saved_models.iter().enumerate() {
            saved_models_list = saved_models_list.push(self.saved_model_row(index, model));
        }

        let saved_models_list: Element<'_, Message> = if self.settings_ui.form.saved_models.len() > 5 {
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

        let context_limit_invalid = self
            .settings_ui
            .form
            .context_message_limit
            .trim()
            .parse::<usize>()
            .is_err();

        let prompt_section =
            widget::settings::section()
                .title("Prompt")
                .add(widget::settings::item(
                    "System prompt",
                    button::standard("Edit system prompt").on_press(Message::OpenSystemPromptModal),
                ));

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
            context_section = context_section.add(
                widget::text::caption("Enter a whole number")
                    .class(cosmic::theme::Text::Color(Color::from_rgb(1.0, 0.42, 0.42))),
            );
        }

        let actions = row![
            button::standard(fl!("save")).on_press(Message::SaveSettings),
            button::text(fl!("cancel")).on_press(Message::CloseSettings),
        ]
        .spacing(spacing.space_s);

        let base = scrollable(
            column![
                button::icon(widget::icon::from_name("go-previous-symbolic").size(16))
                    .on_press(Message::CloseSettings),
                widget::settings::view_column(vec![
                    provider_section.into(),
                    saved_models_section.into(),
                    default_model_section.into(),
                    prompt_section.into(),
                    context_section.into(),
                    actions.into(),
                ]),
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
}
