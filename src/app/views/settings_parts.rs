// SPDX-License-Identifier: MPL-2.0
//! Settings-specific reusable rows, tab controls, and modal overlays.

use super::super::style::*;
use super::super::*;
use crate::runtime::{
    context::builder::build_prompt_preview,
    personalization::{ai_migration_helper_prompt_markdown, parse_ai_migration_response},
};
use cosmic::iced_widget::{column, container, row, scrollable};

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
            "Default model".to_string()
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
            Some(SettingsModal::ManageModels) => self.manage_models_modal(),
            Some(SettingsModal::ManageMemory) => self.manage_memory_modal(),
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
                                .width(Length::Fill)
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
                            button::text("Back").on_press(Message::OpenManageModelsModal),
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
            Some(SettingsModal::AiMigration) => self.ai_migration_modal(),
            Some(SettingsModal::ConfirmResetPersonalization) => {
                self.confirm_reset_personalization_modal()
            }
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
            .on_action(Message::SettingsModalEdited)
            .padding([8, 0])
            .height(Length::Fixed(240.0))
            .wrapping(core_text::Wrapping::WordOrGlyph)
            .class(composer_editor_class());

        let mut actions = row![].spacing(spacing.space_s);
        if let Some(label) = kind.action_label() {
            actions = actions.push(button::standard(label).on_press(Message::SaveSettingsModal));
        }
        actions = actions.push(button::text("Cancel").on_press(Message::CloseSettingsModal));

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

    fn manage_models_modal(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let provider = self.settings_ui.form.provider();
        let model_filter_control = widget::dropdown(
            SettingsForm::model_filter_labels(),
            Some(self.settings_ui.form.model_filter_index()),
            Message::ModelFilterSelected,
        )
        .width(Length::Fill)
        .padding([8, 0, 8, 16]);

        let default_model_block: Element<'_, Message> =
            if self.settings_ui.form.saved_models.is_empty() {
                column![
                    widget::text::caption("Default model").class(cosmic::theme::Text::Color(
                        Color::from_rgba(1.0, 1.0, 1.0, 0.62),
                    )),
                    widget::text::caption("Add at least one model").class(
                        cosmic::theme::Text::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.56)),
                    ),
                ]
                .spacing(spacing.space_xxs)
                .width(Length::Fill)
                .into()
            } else {
                let options = self.settings_ui.form.default_model_options();
                if options.is_empty() {
                    column![
                        widget::text::caption("Default model").class(cosmic::theme::Text::Color(
                            Color::from_rgba(1.0, 1.0, 1.0, 0.62),
                        )),
                        widget::text::caption(format!("No saved {} models yet.", provider.label()))
                            .class(cosmic::theme::Text::Color(Color::from_rgba(
                                1.0, 1.0, 1.0, 0.56,
                            ))),
                    ]
                    .spacing(spacing.space_xxs)
                    .width(Length::Fill)
                    .into()
                } else {
                    column![
                        widget::text::caption("Default model").class(cosmic::theme::Text::Color(
                            Color::from_rgba(1.0, 1.0, 1.0, 0.62),
                        )),
                        container(
                            widget::dropdown(
                                options,
                                self.settings_ui.form.default_model_index(),
                                Message::DefaultModelSelected,
                            )
                            .width(Length::Fill)
                            .padding([8, 0, 8, 16]),
                        )
                        .width(Length::Fill),
                    ]
                    .spacing(spacing.space_xxs)
                    .width(Length::Fill)
                    .into()
                }
            };

        let filtered_model_indices = self.settings_ui.form.filtered_saved_model_indices();
        let mut filtered_rows = column![].spacing(spacing.space_s).width(Length::Fill);
        for index in &filtered_model_indices {
            if let Some(model) = self.settings_ui.form.saved_models.get(*index) {
                filtered_rows = filtered_rows.push(self.saved_model_row(*index, model));
            }
        }

        let models_list: Element<'_, Message> = if self.settings_ui.form.saved_models.is_empty() {
            widget::settings::section()
                .add(widget::text::caption("No saved models added yet.").class(
                    cosmic::theme::Text::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.56)),
                ))
                .into()
        } else if filtered_model_indices.is_empty() {
            widget::settings::section()
                .add(
                    widget::text::caption(format!(
                        "No models match the current {} filter.",
                        self.settings_ui.form.model_filter_summary()
                    ))
                    .class(cosmic::theme::Text::Color(Color::from_rgba(
                        1.0, 1.0, 1.0, 0.56,
                    ))),
                )
                .into()
        } else {
            let rows: Element<'_, Message> = if filtered_model_indices.len() > 3 {
                scrollable(filtered_rows)
                    .height(Length::Fixed(204.0))
                    .class(cosmic::style::iced::Scrollable::Minimal)
                    .direction(thin_vertical_scrollbar())
                    .into()
            } else {
                filtered_rows.into()
            };

            widget::settings::section().add(rows).into()
        };

        container(
            column![
                widget::text::heading("Manage models"),
                default_model_block,
                row![
                    container(widget::text::heading("Models")).width(Length::Fill),
                    button::standard("Add model").on_press(Message::OpenAddModelModal),
                ]
                .spacing(spacing.space_s)
                .align_y(Alignment::Center)
                .width(Length::Fill),
                column![
                    widget::text::caption("Model filter").class(cosmic::theme::Text::Color(
                        Color::from_rgba(1.0, 1.0, 1.0, 0.62),
                    )),
                    container(model_filter_control)
                        .width(Length::Fill)
                        .align_x(alignment::Horizontal::Center),
                ]
                .spacing(spacing.space_xxs)
                .width(Length::Fill),
                models_list,
                container(button::text("Close").on_press(Message::CloseSettingsModal))
                    .width(Length::Fill)
                    .align_x(alignment::Horizontal::Right),
            ]
            .spacing(spacing.space_m),
        )
        .padding(spacing.space_m)
        .width(Length::Fixed(PANEL_WIDTH - 16.0))
        .class(chat_list_card_class())
        .into()
    }

    fn manage_memory_modal(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let filtered_indices = self.settings_ui.filtered_memory_indices();

        let mut filtered_rows = column![].spacing(spacing.space_s).width(Length::Fill);
        for index in &filtered_indices {
            if let Some(item) = self.settings_ui.form.memory_items.get(*index) {
                filtered_rows = filtered_rows.push(self.memory_item_row(*index, item));
            }
        }

        let memory_list: Element<'_, Message> = if self.settings_ui.form.memory_items.is_empty() {
            widget::settings::section()
                .add(widget::text::caption("No manual memory yet.").class(
                    cosmic::theme::Text::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.56)),
                ))
                .into()
        } else if filtered_indices.is_empty() {
            widget::settings::section()
                .add(
                    widget::text::caption("No memory items match the current search.").class(
                        cosmic::theme::Text::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.56)),
                    ),
                )
                .into()
        } else {
            let rows: Element<'_, Message> = if filtered_indices.len() > 3 {
                scrollable(filtered_rows)
                    .height(Length::Fixed(204.0))
                    .class(cosmic::style::iced::Scrollable::Minimal)
                    .direction(thin_vertical_scrollbar())
                    .into()
            } else {
                filtered_rows.into()
            };

            widget::settings::section().add(rows).into()
        };

        container(
            column![
                widget::text::heading("Manage memory"),
                column![
                    widget::text::caption("Search").class(cosmic::theme::Text::Color(
                        Color::from_rgba(1.0, 1.0, 1.0, 0.62),
                    )),
                    widget::text_input::text_input(
                        "Search by text, symbols, or keywords",
                        &self.settings_ui.memory_search_query,
                    )
                    .on_input(Message::MemorySearchChanged)
                    .width(Length::Fill),
                ]
                .spacing(spacing.space_xxs)
                .width(Length::Fill),
                row![
                    container(widget::text::heading("Memory")).width(Length::Fill),
                    button::standard("Add memory item").on_press(Message::AddMemoryItem),
                ]
                .spacing(spacing.space_s)
                .align_y(Alignment::Center)
                .width(Length::Fill),
                memory_list,
                container(button::text("Close").on_press(Message::CloseSettingsModal))
                    .width(Length::Fill)
                    .align_x(alignment::Horizontal::Right),
            ]
            .spacing(spacing.space_m),
        )
        .padding(spacing.space_m)
        .width(Length::Fixed(PANEL_WIDTH - 16.0))
        .class(chat_list_card_class())
        .into()
    }

    fn confirm_reset_personalization_modal(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();

        container(
            column![
                widget::text::heading("Reset personalization"),
                widget::text::body("Delete personalization data?")
                    .size(18)
                    .class(cosmic::theme::Text::Color(Color::from_rgba(
                        1.0, 1.0, 1.0, 0.92
                    ))),
                widget::text::caption(
                    "Profile fields, base prompt overrides, and manual memory will be removed.",
                )
                .class(cosmic::theme::Text::Color(Color::from_rgba(
                    1.0, 1.0, 1.0, 0.56
                ))),
                row![
                    button::standard("Delete data").on_press(Message::ConfirmResetPersonalization),
                    button::text("Cancel").on_press(Message::CloseSettingsModal),
                ]
                .spacing(spacing.space_s)
                .align_y(Alignment::Center),
            ]
            .spacing(spacing.space_m)
            .width(Length::Fill),
        )
        .padding(spacing.space_m)
        .width(Length::Fixed(PANEL_WIDTH - 48.0))
        .class(chat_list_card_class())
        .into()
    }

    fn ai_migration_modal(&self) -> Element<'_, Message> {
        match &self.settings_ui.ai_migration_state {
            AiMigrationState::Editor => self.ai_migration_editor_modal(),
            AiMigrationState::Processing { frame, .. } => {
                self.ai_migration_processing_modal(*frame)
            }
            AiMigrationState::Failed { error } => self.ai_migration_failed_modal(error),
            AiMigrationState::Success {
                completion_ratio, ..
            } => self.ai_migration_success_modal(*completion_ratio),
        }
    }

    fn ai_migration_editor_modal(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let model_options = self.settings_ui.form.default_model_options();
        let has_models = !model_options.is_empty();
        let raw_input = self.settings_ui.ai_migration_input_content.text();
        let helper_prompt = ai_migration_helper_prompt_markdown();
        let direct_payload_ready = parse_ai_migration_response(&raw_input).is_ok();
        let helper_prompt_copy_label =
            if self.copied_target == Some(CopiedTarget::AiMigrationPrompt) {
                "Copied"
            } else {
                "Copy prompt"
            };
        let can_process = (direct_payload_ready
            || (has_models
                && self
                    .settings_ui
                    .ai_migration_visible_model_index()
                    .is_some()))
            && !raw_input.trim().is_empty()
            && !self.settings_ui.ai_migration_state.is_processing();

        let model_control: Element<'_, Message> = if has_models {
            container(
                widget::dropdown(
                    model_options,
                    self.settings_ui.ai_migration_visible_model_index(),
                    Message::AiMigrationModelSelected,
                )
                .width(Length::Fill)
                .padding([8, 0, 8, 16]),
            )
            .width(Length::Fill)
            .into()
        } else if self.settings_ui.form.saved_models.is_empty() {
            widget::text::caption("Add at least one saved model before using AI migration.")
                .class(cosmic::theme::Text::Color(Color::from_rgb(1.0, 0.42, 0.42)))
                .into()
        } else {
            widget::text::caption(format!(
                "No saved {} models yet. Change Provider in settings or paste ready JSON.",
                self.settings_ui.form.provider().label()
            ))
            .class(cosmic::theme::Text::Color(Color::from_rgb(1.0, 0.42, 0.42)))
            .into()
        };

        let mut actions = row![].spacing(spacing.space_s);
        let process_button = if can_process {
            button::standard("Process").on_press(Message::StartAiMigration)
        } else {
            button::standard("Process")
        };
        actions = actions.push(process_button);
        actions = actions.push(button::text("Cancel").on_press(Message::CloseSettingsModal));

        let content = column![
            widget::text::heading("AI migration"),
            widget::text::caption(
                "Paste raw notes and let the selected model convert them, or copy the helper prompt below, run it in any external model, then paste the returned JSON here. If the pasted content is already valid, Process applies it immediately."
            )
            .class(cosmic::theme::Text::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.62))),
            column![
                row![
                    widget::text::caption("Prompt for another model").class(
                        cosmic::theme::Text::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.62))
                    ),
                    button::standard(helper_prompt_copy_label)
                        .on_press(Message::CopyAiMigrationPrompt),
                ]
                .spacing(spacing.space_s)
                .align_y(Alignment::Center),
                container(
                    cosmic::widget::scrollable(
                        widget::text::body(helper_prompt)
                            .font(cosmic::font::mono())
                            .width(Length::Fill)
                    )
                    .height(Length::Fixed(180.0))
                    .class(cosmic::style::iced::Scrollable::Minimal)
                    .direction(thin_vertical_scrollbar())
                )
                .padding([spacing.space_s, spacing.space_m])
                .width(Length::Fill)
                .class(composer_container_class()),
                widget::text::caption(
                    "Send that prompt to any model with your source notes. Paste its JSON code block below. You can also paste rough notes directly and let the selected model structure them for you."
                )
                .class(cosmic::theme::Text::Color(Color::from_rgba(
                    1.0, 1.0, 1.0, 0.56
                ))),
            ]
            .spacing(spacing.space_xxs)
            .width(Length::Fill),
            column![
                widget::text::caption("Model (optional for ready JSON)").class(
                    cosmic::theme::Text::Color(
                    Color::from_rgba(1.0, 1.0, 1.0, 0.62)
                )),
                model_control,
            ]
            .spacing(spacing.space_xxs)
            .width(Length::Fill),
            column![
                widget::text::caption("Notes or JSON payload").class(
                    cosmic::theme::Text::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.62))
                ),
                container(
                    widget::text_editor(&self.settings_ui.ai_migration_input_content)
                        .id(self.settings_ui.ai_migration_input_editor_id.clone())
                        .on_action(Message::AiMigrationEdited)
                        .padding([8, 0])
                        .height(Length::Fixed(300.0))
                        .wrapping(core_text::Wrapping::WordOrGlyph)
                        .class(composer_editor_class()),
                )
                .padding([spacing.space_s, spacing.space_m])
                .width(Length::Fill)
                .class(composer_container_class()),
            ]
            .spacing(spacing.space_xxs)
            .width(Length::Fill),
        ]
        .spacing(spacing.space_m);

        container(content.push(actions))
            .padding(spacing.space_m)
            .width(Length::Fixed(PANEL_WIDTH - 16.0))
            .class(chat_list_card_class())
            .into()
    }

    fn ai_migration_processing_modal(&self, frame: u16) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();

        container(
            column![
                widget::text::heading("AI migration"),
                ai_migration_shimmer_text(frame, "Personalizing your profile..."),
                container(button::text("Cancel").on_press(Message::ReturnToAiMigrationEditor))
                    .width(Length::Fill)
                    .align_x(alignment::Horizontal::Center),
            ]
            .spacing(spacing.space_l)
            .width(Length::Fill),
        )
        .padding(spacing.space_m)
        .width(Length::Fixed(PANEL_WIDTH - 16.0))
        .class(chat_list_card_class())
        .into()
    }

    fn ai_migration_failed_modal(&self, error: &str) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let error = error.to_string();

        container(
            column![
                widget::text::heading("AI migration"),
                ai_migration_status_badge(
                    "dialog-warning-symbolic",
                    Color::from_rgb(1.0, 0.42, 0.42),
                    Color::from_rgba(0.36, 0.08, 0.08, 0.44),
                    Color::from_rgba(1.0, 0.42, 0.42, 0.28),
                ),
                widget::text::body("Processing did not complete")
                    .size(18)
                    .class(cosmic::theme::Text::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.92))),
                widget::text::caption(error)
                    .class(cosmic::theme::Text::Color(Color::from_rgb(1.0, 0.42, 0.42))),
                widget::text::caption(
                    "Retry can help. The previous error will be sent back to the model so it can correct the JSON.",
                )
                .class(cosmic::theme::Text::Color(Color::from_rgba(
                    1.0, 1.0, 1.0, 0.56
                ))),
                row![
                    button::standard("Retry").on_press(Message::RetryAiMigration),
                    button::text("Back").on_press(Message::ReturnToAiMigrationEditor),
                ]
                .spacing(spacing.space_s)
                .align_y(Alignment::Center),
            ]
            .spacing(spacing.space_m)
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Center),
        )
        .padding(spacing.space_m)
        .width(Length::Fixed(PANEL_WIDTH - 16.0))
        .class(chat_list_card_class())
        .into()
    }

    fn ai_migration_success_modal(&self, completion_ratio: f32) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let percent = (completion_ratio * 100.0).round() as u8;

        container(
            column![
                widget::text::heading("AI migration"),
                ai_migration_status_badge(
                    "object-select-symbolic",
                    Color::from_rgb(0.48, 0.9, 0.62),
                    Color::from_rgba(0.08, 0.24, 0.14, 0.44),
                    Color::from_rgba(0.48, 0.9, 0.62, 0.28),
                ),
                widget::text::body("Personalization updated")
                    .size(18)
                    .class(cosmic::theme::Text::Color(Color::from_rgba(
                        1.0, 1.0, 1.0, 0.92
                    ))),
                widget::text::caption(format!("{percent}% of the profile was filled.")).class(
                    cosmic::theme::Text::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.62))
                ),
            ]
            .spacing(spacing.space_m)
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Center),
        )
        .padding(spacing.space_m)
        .width(Length::Fixed(PANEL_WIDTH - 16.0))
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

fn ai_migration_status_badge<'a>(
    icon_name: &'a str,
    icon_color: Color,
    background: Color,
    border: Color,
) -> Element<'a, Message> {
    let frame_size = 72.0;

    container(
        container(widget::icon::from_name(icon_name).size(28))
            .width(Length::Fixed(frame_size))
            .height(Length::Fixed(frame_size))
            .center_x(Length::Fixed(frame_size))
            .center_y(Length::Fixed(frame_size))
            .class(cosmic::theme::Container::Custom(Box::new(move |_theme| {
                cosmic::iced::widget::container::Style {
                    icon_color: Some(icon_color),
                    text_color: Some(icon_color),
                    background: Some(Background::Color(background)),
                    border: cosmic::iced_core::Border {
                        radius: 999.0.into(),
                        width: 1.0,
                        color: border,
                    },
                    shadow: Default::default(),
                    snap: true,
                }
            }))),
    )
    .width(Length::Fill)
    .align_x(alignment::Horizontal::Center)
    .into()
}

fn ai_migration_shimmer_text<'a>(frame: u16, text: &'a str) -> Element<'a, Message> {
    let highlight = usize::from(frame / 2);
    let mut line = row![].spacing(0).align_y(Alignment::Center);

    for (index, ch) in text.chars().enumerate() {
        let distance = highlight.abs_diff(index) % 8;
        let alpha = match distance {
            0 => 1.0,
            1 => 0.86,
            2 => 0.72,
            3 => 0.58,
            _ => 0.36,
        };
        let glyph = if ch == ' ' { "\u{00A0}" } else { "" };
        let segment = if glyph.is_empty() {
            ch.to_string()
        } else {
            glyph.to_string()
        };

        line = line.push(
            widget::text::body(segment)
                .size(18)
                .class(cosmic::theme::Text::Color(Color::from_rgba(
                    1.0, 1.0, 1.0, alpha,
                ))),
        );
    }

    container(line)
        .width(Length::Fill)
        .align_x(alignment::Horizontal::Center)
        .into()
}
