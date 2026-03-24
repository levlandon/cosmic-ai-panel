//! Bootstrap and subscription helpers for the app trait entrypoints.

use super::*;

impl AppModel {
    pub(in crate::app) fn init_app(core: Core) -> (Self, Task<cosmic::Action<Message>>) {
        let mut state = storage::load_state();
        state.settings.normalize();
        if let Some(default_model) = state.settings.default_model.clone() {
            for chat in &mut state.chats {
                if chat.model.trim().is_empty() {
                    chat.provider = default_model.provider;
                    chat.model = default_model.name.clone();
                }
            }
        }
        let mut status = None;

        match secrets::load_openrouter_api_key() {
            Ok(Some(api_key)) => {
                state.settings.openrouter_api_key = api_key;
            }
            Ok(None) => {
                let legacy_key = state.settings.openrouter_api_key.trim().to_string();
                if !legacy_key.is_empty() {
                    match secrets::save_openrouter_api_key(&legacy_key) {
                        Ok(()) => {
                            state.settings.openrouter_api_key = legacy_key;
                            if let Err(error) = storage::save_state(&state) {
                                status = Some(format!("{}: {error}", fl!("status-save-failed")));
                            }
                        }
                        Err(error) => {
                            status = Some(error);
                        }
                    }
                }
            }
            Err(error) => {
                status = Some(error);
            }
        }

        let settings_form = SettingsForm::from_settings(&state.settings);

        let mut app = AppModel {
            core,
            state,
            settings_form,
            status,
            config: cosmic_config::Config::new(
                <Self as cosmic::Application>::APP_ID,
                Config::VERSION,
            )
                .map(|context| match Config::get_entry(&context) {
                    Ok(config) => config,
                    Err((_errors, config)) => config,
                })
                .unwrap_or_default(),
            ..Self::default_state()
        };
        app.rebuild_markdown_cache();
        app.rebuild_message_view_cache();

        (app, Task::none())
    }

    pub(in crate::app) fn app_subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            self.core
                .watch_config::<Config>(<Self as cosmic::Application>::APP_ID)
                .map(|update| {
                    // for why in update.errors {
                    //     tracing::error!(?why, "app config error");
                    // }

                    Message::UpdateConfig(update.config)
                }),
            event::listen_with(|event, _, id| match event {
                Event::Window(window::Event::Closed) => Some(Message::PanelClosed(id)),
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(keyboard::key::Named::Escape),
                    ..
                }) => Some(Message::EscapePressed(id)),
                _ => None,
            }),
            if self.loading_chat_id.is_some() {
                time::every(Duration::from_millis(LOADING_TICK_MS)).map(|_| Message::LoadingTick)
            } else {
                Subscription::none()
            },
            provider_events_subscription().map(Message::ProviderEvent),
        ])
    }

    pub(in crate::app) fn default_state() -> Self {
        let (provider_events_tx, provider_events_rx) = unbounded_channel();
        let provider_events_rx = Arc::new(Mutex::new(provider_events_rx));
        let _ = PROVIDER_EVENTS_RX.get_or_init(|| provider_events_rx.clone());

        Self {
            core: Core::default(),
            panel_window: None,
            panel_requested_open: false,
            config: Config::default(),
            panel_view: PanelView::Chat,
            state: AppState::default(),
            composer_content: widget::text_editor::Content::new(),
            composer_editor_id: widget::Id::unique(),
            editing_message_id: None,
            editing_content: widget::text_editor::Content::new(),
            editing_editor_id: widget::Id::unique(),
            messages_scroll_id: widget::Id::unique(),
            messages_bottom_distance: 0.0,
            assistant_markdown: HashMap::new(),
            rename_chat_id: None,
            rename_input: String::new(),
            hovered_chat_id: None,
            hovered_message_id: None,
            copied_target: None,
            message_view_content: HashMap::new(),
            message_view_text: HashMap::new(),
            settings_form: SettingsForm::default(),
            settings_modal: None,
            add_model_provider_index: ProviderKind::OpenRouter.index(),
            add_model_name: String::new(),
            system_prompt_content: widget::text_editor::Content::new(),
            system_prompt_editor_id: widget::Id::unique(),
            connection_test_state: ConnectionTestState::Idle,
            provider_client: Client::new(),
            provider_events_tx,
            provider_task: None,
            inflight_request: None,
            chat_error: None,
            loading_chat_id: None,
            loading_phase: 0,
            next_request_id: 1,
            status: None,
            transient_chat_notice: None,
        }
    }
}

fn provider_events_subscription() -> Subscription<provider::ProviderEvent> {
    Subscription::run_with("provider-events", |_| {
        stream::channel(
            100,
            |mut output: futures::channel::mpsc::Sender<provider::ProviderEvent>| async move {
                loop {
                    let Some(rx) = PROVIDER_EVENTS_RX.get().cloned() else {
                        futures::future::pending::<()>().await;
                        continue;
                    };

                    let next = {
                        let mut rx = rx.lock().await;
                        rx.recv().await
                    };

                    if let Some(event) = next {
                        let _ = output.send(event).await;
                    } else {
                        futures::future::pending::<()>().await;
                    }
                }
            },
        )
    })
}
