// SPDX-License-Identifier: MPL-2.0

use crate::chat::{AppSettings, ChatMessage, ChatRole, ChatSession, ProviderKind, SavedModel};
use crate::config::Config;
use crate::fl;
use crate::provider::{self, ProviderRequest};
use crate::secrets;
use crate::storage::{self, PersistedState};
use cosmic::app::Core;
use cosmic::applet::{Size as AppletSize, cosmic_panel_config::PanelAnchor};
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::cosmic_theme::palette::WithAlpha;
use cosmic::iced::advanced::layout::Limits;
use cosmic::iced::{
    Alignment, Length, Subscription, alignment, event, keyboard, mouse,
    platform_specific::{
        runtime::wayland::layer_surface::{IcedMargin, SctkLayerSurfaceSettings},
        shell::commands::layer_surface::{
            Anchor, KeyboardInteractivity, Layer, destroy_layer_surface, get_layer_surface,
        },
    },
    time,
    window::{self, Id},
};
use cosmic::iced_core::text::{self as core_text};
use cosmic::iced_core::{Background, Color, Event, Font, Pixels, Size, Vector};
use cosmic::iced_futures::futures::{self, SinkExt};
use cosmic::iced_futures::stream;
use cosmic::iced_widget::{column, container, rich_text, row, scrollable, stack, text};
use cosmic::prelude::*;
use cosmic::widget::button::Catalog;
use cosmic::widget::{self, button, header_bar};
use reqwest::Client;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::task::JoinHandle;

const PANEL_WIDTH: f32 = 420.0;
const PANEL_MIN_HEIGHT: f32 = 560.0;
const CHAT_ACTIONS_WIDTH: f32 = 84.0;
const COMPOSER_LINE_HEIGHT: f32 = 28.0;
const COMPOSER_MAX_HEIGHT: f32 = PANEL_MIN_HEIGHT * 0.5;
const THIN_SCROLLBAR_WIDTH: f32 = 4.0;
const THIN_SCROLLER_WIDTH: f32 = 3.0;
const LOADING_TICK_MS: u64 = 120;
const MESSAGE_MAX_WIDTH: f32 = PANEL_WIDTH * 0.86;
const USER_MESSAGE_BUBBLE_WIDTH: f32 = PANEL_WIDTH * (2.0 / 3.0);
const USER_MESSAGE_HORIZONTAL_PADDING: f32 = 32.0;
const USER_MESSAGE_EDITOR_WIDTH_BUFFER: f32 = 12.0;
const MESSAGE_TEXT_SIZE_PX: f32 = 14.0;
const CHAT_AUTOSCROLL_THRESHOLD_PX: f32 = 36.0;
const SETTINGS_PROVIDER_OPTIONS: [&str; 2] = ["OpenRouter", "LM Studio"];
const CHAT_MODEL_DROPDOWN_WIDTH: f32 = 236.0;

static PROVIDER_EVENTS_RX: OnceLock<Arc<Mutex<UnboundedReceiver<provider::ProviderEvent>>>> =
    OnceLock::new();

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
enum PanelView {
    #[default]
    Chat,
    Chats,
    Settings,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum SettingsModal {
    AddModel,
    SystemPrompt,
}

#[derive(Debug, Clone, Default)]
enum ConnectionTestState {
    #[default]
    Idle,
    Testing,
    Success,
    Failed(String),
}

#[derive(Debug, Clone, Default)]
struct SettingsForm {
    provider_index: usize,
    openrouter_api_key: String,
    lmstudio_base_url: String,
    saved_models: Vec<SavedModel>,
    default_model: Option<SavedModel>,
    system_prompt: String,
    context_message_limit: String,
}

impl SettingsForm {
    fn from_settings(settings: &AppSettings) -> Self {
        Self {
            provider_index: settings.provider.index(),
            openrouter_api_key: settings.openrouter_api_key.clone(),
            lmstudio_base_url: settings.lmstudio_base_url.clone(),
            saved_models: settings.saved_models.clone(),
            default_model: settings.default_model.clone(),
            system_prompt: settings.system_prompt.clone(),
            context_message_limit: settings.context_message_limit.to_string(),
        }
    }

    fn provider(&self) -> ProviderKind {
        ProviderKind::from_index(self.provider_index)
    }

    fn provider_labels() -> &'static [&'static str; 2] {
        &SETTINGS_PROVIDER_OPTIONS
    }

    fn default_model_options(&self) -> Vec<String> {
        self.saved_models
            .iter()
            .map(SavedModel::dropdown_label)
            .collect()
    }

    fn default_model_index(&self) -> Option<usize> {
        let selected = self.default_model.as_ref()?;
        self.saved_models.iter().position(|model| model == selected)
    }

    fn select_provider(&mut self, provider: ProviderKind) {
        self.provider_index = provider.index();

        if self
            .default_model
            .as_ref()
            .is_none_or(|model| model.provider != provider)
        {
            self.default_model = self
                .saved_models
                .iter()
                .find(|model| model.provider == provider)
                .cloned();
        }
    }

    fn select_default_model(&mut self, index: usize) {
        if let Some(model) = self.saved_models.get(index).cloned() {
            self.provider_index = model.provider.index();
            self.default_model = Some(model);
        }
    }

    fn add_model(&mut self, provider: ProviderKind, name: &str) -> bool {
        let Some(model) = SavedModel::normalized(provider, name) else {
            return false;
        };

        if !self.saved_models.contains(&model) {
            self.saved_models.push(model.clone());
        }

        if self.default_model.is_none() {
            self.default_model = Some(model.clone());
            self.provider_index = model.provider.index();
        }

        true
    }

    fn remove_model(&mut self, index: usize) {
        if index >= self.saved_models.len() {
            return;
        }

        let removed = self.saved_models.remove(index);
        if self.default_model.as_ref() == Some(&removed) {
            self.default_model = self
                .saved_models
                .iter()
                .find(|model| model.provider == self.provider())
                .cloned()
                .or_else(|| self.saved_models.first().cloned());

            if let Some(model) = &self.default_model {
                self.provider_index = model.provider.index();
            }
        }
    }

    fn apply_to_settings(&self, settings: &mut AppSettings) -> bool {
        settings.provider = self.provider();
        settings.openrouter_api_key = self.openrouter_api_key.trim().to_string();
        settings.lmstudio_base_url = self.lmstudio_base_url.trim().to_string();
        settings.saved_models = self.saved_models.clone();
        settings.default_model = self.default_model.clone();
        settings.system_prompt = self.system_prompt.clone();

        let parsed_context_limit = self.context_message_limit.trim().parse::<usize>().ok();

        if let Some(limit) = parsed_context_limit {
            settings.context_message_limit = limit;
        }

        settings.normalize();
        parsed_context_limit.is_some()
    }
}

#[derive(Debug, Clone)]
struct InflightRequest {
    request_id: u64,
    chat_id: u64,
    request: ProviderRequest,
    assistant_message_id: Option<u64>,
}

#[derive(Debug, Clone)]
struct ChatErrorState {
    chat_id: u64,
    message: String,
    request: ProviderRequest,
    assistant_message_id: Option<u64>,
}

#[derive(Debug, Clone)]
struct TransientChatNotice {
    chat_id: u64,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CopiedTarget {
    Message(u64),
    CodeBlock { message_id: u64, block_index: usize },
}

fn panel_reserved_margin(core: &Core) -> IcedMargin {
    let panel_thickness = match &core.applet.size {
        AppletSize::PanelSize(size) => size.get_applet_icon_size_with_padding(false) as i32,
        AppletSize::Hardcoded((width, height)) => match core.applet.anchor {
            PanelAnchor::Top | PanelAnchor::Bottom => i32::from(*height),
            PanelAnchor::Left | PanelAnchor::Right => i32::from(*width),
        },
    };

    match core.applet.anchor {
        PanelAnchor::Top => IcedMargin {
            top: panel_thickness,
            ..Default::default()
        },
        PanelAnchor::Bottom => IcedMargin {
            bottom: panel_thickness,
            ..Default::default()
        },
        PanelAnchor::Left => IcedMargin {
            left: panel_thickness,
            ..Default::default()
        },
        PanelAnchor::Right => IcedMargin {
            right: panel_thickness,
            ..Default::default()
        },
    }
}

/// The application model stores app-specific state used to describe its interface and
/// drive its logic.
pub struct AppModel {
    core: Core,
    panel_window: Option<Id>,
    panel_requested_open: bool,
    config: Config,
    panel_view: PanelView,
    state: PersistedState,
    composer_content: widget::text_editor::Content,
    composer_editor_id: widget::Id,
    editing_message_id: Option<u64>,
    editing_content: widget::text_editor::Content,
    editing_editor_id: widget::Id,
    messages_scroll_id: widget::Id,
    messages_bottom_distance: f32,
    assistant_markdown: HashMap<u64, widget::markdown::Content>,
    message_view_content: HashMap<u64, widget::text_editor::Content>,
    message_view_text: HashMap<u64, String>,
    rename_chat_id: Option<u64>,
    rename_input: String,
    hovered_chat_id: Option<u64>,
    hovered_message_id: Option<u64>,
    copied_target: Option<CopiedTarget>,
    settings_form: SettingsForm,
    settings_modal: Option<SettingsModal>,
    add_model_provider_index: usize,
    add_model_name: String,
    system_prompt_content: widget::text_editor::Content,
    system_prompt_editor_id: widget::Id,
    connection_test_state: ConnectionTestState,
    provider_client: Client,
    provider_events_tx: UnboundedSender<provider::ProviderEvent>,
    provider_task: Option<JoinHandle<()>>,
    inflight_request: Option<InflightRequest>,
    chat_error: Option<ChatErrorState>,
    loading_chat_id: Option<u64>,
    loading_phase: u8,
    next_request_id: u64,
    status: Option<String>,
    transient_chat_notice: Option<TransientChatNotice>,
}

/// Messages emitted by the application and its widgets.
#[derive(Debug, Clone)]
pub enum Message {
    AppletPressed,
    TogglePanel,
    PanelClosed(Id),
    EscapePressed(Id),
    UpdateConfig(Config),
    NewChat,
    ToggleChatList,
    SelectChat(u64),
    ChatHovered(u64),
    ChatUnhovered(u64),
    MessageHovered(u64),
    MessageUnhovered(u64),
    BeginRenameChat(u64),
    RenameInputChanged(String),
    CommitRenameChat(String),
    CancelRenameChat,
    DeleteChat(u64),
    OpenSettings,
    CloseSettings,
    ChatScrolled(cosmic::iced::widget::scrollable::Viewport),
    ComposerEdited(widget::text_editor::Action),
    InlineEditEdited(widget::text_editor::Action),
    MessageViewerEdited(u64, widget::text_editor::Action),
    MarkdownLink(widget::markdown::Uri),
    SubmitComposer,
    SaveEditedMessage,
    CancelEditedMessage,
    StopGeneration,
    LoadingTick,
    ProviderEvent(provider::ProviderEvent),
    CopyMessage(u64),
    CopyCodeBlock {
        message_id: u64,
        block_index: usize,
        content: String,
    },
    ResetCopiedTarget(CopiedTarget),
    RetryRequest(u64),
    RegenerateLastAssistant(u64),
    EditUserMessage(u64),
    DeleteLastAssistant(u64),
    BranchConversation(u64),
    ProviderSelected(usize),
    OpenRouterKeyChanged(String),
    LmStudioUrlChanged(String),
    ContextLimitChanged(String),
    DefaultModelSelected(usize),
    ActiveModelSelected(usize),
    OpenAddModelModal,
    CloseSettingsModal,
    AddModelProviderSelected(usize),
    AddModelNameChanged(String),
    SaveAddedModel,
    RemoveSavedModel(usize),
    OpenSystemPromptModal,
    SystemPromptEdited(widget::text_editor::Action),
    SaveSystemPrompt,
    TestConnection,
    ConnectionTestFinished(Result<(), String>),
    SaveSettings,
}

/// Create a COSMIC application from the app model
impl cosmic::Application for AppModel {
    /// The async executor that will be used to run your application's commands.
    type Executor = cosmic::executor::Default;

    /// Data that your application receives to its init method.
    type Flags = ();

    /// Messages which the application and its widgets will emit.
    type Message = Message;

    /// Unique identifier in RDNN (reverse domain name notation) format.
    const APP_ID: &'static str = "com.levlon.aipanel";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    /// Initializes the application with any given flags and startup commands.
    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<cosmic::Action<Self::Message>>) {
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
            config: cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
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

    fn on_close_requested(&self, _id: Id) -> Option<Message> {
        None
    }

    /// Describes the interface based on the current state of the application model.
    ///
    fn view(&self) -> Element<'_, Self::Message> {
        self.core
            .applet
            .icon_button("sparkleshare-symbolic")
            .on_press(Message::AppletPressed)
            .into()
    }

    fn view_window(&self, id: Id) -> Element<'_, Self::Message> {
        if self.panel_window != Some(id) {
            return container(text("")).into();
        }

        let header_title = match self.panel_view {
            PanelView::Settings => fl!("settings-title"),
            PanelView::Chats => String::new(),
            PanelView::Chat => String::new(),
        };
        let focused = self
            .core()
            .focused_window()
            .map(|focused_id| focused_id == id)
            .unwrap_or_default();
        let mut header = header_bar()
            .title(header_title)
            .focused(focused)
            .start(
                button::icon(widget::icon::from_name("document-new-symbolic").size(16))
                    .on_press(Message::NewChat),
            )
            .end(
                button::icon(widget::icon::from_name("open-menu-symbolic").size(16))
                    .on_press(Message::ToggleChatList),
            )
            .end(
                button::icon(widget::icon::from_name("preferences-system-symbolic").size(16))
                    .on_press(Message::OpenSettings),
            )
            .end(
                button::icon(widget::icon::from_name("window-close-symbolic").size(16))
                    .on_press(Message::TogglePanel),
            );

        if self.panel_view == PanelView::Chat {
            header = header.center(self.chat_model_header());
        }

        let main_view = match self.panel_view {
            PanelView::Chat => self.chat_screen(),
            PanelView::Chats => self.chats_screen(),
            PanelView::Settings => self.settings_screen(),
        };

        let body = container(main_view)
            .width(Length::Fill)
            .height(Length::Fill)
            .class(cosmic::style::Container::Background);

        column![header, body]
            .width(Length::Fixed(PANEL_WIDTH))
            .height(Length::Fill)
            .into()
    }

    /// Register subscriptions for this application.
    ///
    /// Subscriptions are long-lived async tasks running in the background which
    /// emit messages to the application through a channel. They may be conditionally
    /// activated by selectively appending to the subscription batch, and will
    /// continue to execute for the duration that they remain in the batch.
    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::batch(vec![
            self.core()
                .watch_config::<Config>(Self::APP_ID)
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

    /// Handles messages emitted by the application and its widgets.
    ///
    /// Tasks may be returned for asynchronous execution of code in the background
    /// on the application's async runtime. The application will not exit until all
    /// tasks are finished.
    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::AppletPressed => {
                return self.handle_applet_pressed();
            }
            Message::UpdateConfig(config) => {
                self.config = config;
            }
            Message::NewChat => {
                let new_chat_id = self.create_chat();
                self.state.active_chat_id = new_chat_id;
                self.panel_view = PanelView::Chat;
                self.reset_composer();
                self.reset_inline_edit();
                self.rename_chat_id = None;
                self.hovered_message_id = None;
                self.persist_state();
                return self.scroll_messages_to_end(true);
            }
            Message::ToggleChatList => {
                self.rename_chat_id = None;
                self.rename_input.clear();
                self.panel_view = if self.panel_view == PanelView::Chats {
                    PanelView::Chat
                } else {
                    PanelView::Chats
                };
            }
            Message::SelectChat(chat_id) => {
                self.state.active_chat_id = chat_id;
                self.panel_view = PanelView::Chat;
                self.reset_inline_edit();
                self.rename_chat_id = None;
                self.hovered_chat_id = None;
                self.hovered_message_id = None;
                return self.scroll_messages_to_end(true);
            }
            Message::ChatHovered(chat_id) => {
                self.hovered_chat_id = Some(chat_id);
            }
            Message::ChatUnhovered(chat_id) => {
                if self.hovered_chat_id == Some(chat_id) {
                    self.hovered_chat_id = None;
                }
            }
            Message::MessageHovered(message_id) => {
                self.hovered_message_id = Some(message_id);
            }
            Message::MessageUnhovered(message_id) => {
                if self.hovered_message_id == Some(message_id) {
                    self.hovered_message_id = None;
                }
            }
            Message::BeginRenameChat(chat_id) => {
                self.rename_chat_id = Some(chat_id);
                self.rename_input = self
                    .state
                    .chats
                    .iter()
                    .find(|chat| chat.id == chat_id)
                    .map(|chat| chat.title.clone())
                    .unwrap_or_default();
            }
            Message::RenameInputChanged(value) => {
                self.rename_input = value;
            }
            Message::CommitRenameChat(value) => {
                if let Some(chat_id) = self.rename_chat_id {
                    let next_title = value.trim();
                    if !next_title.is_empty() {
                        if let Some(chat) =
                            self.state.chats.iter_mut().find(|chat| chat.id == chat_id)
                        {
                            chat.title = next_title.to_string();
                            chat.touch();
                            self.persist_state();
                        }
                    }
                }
                self.rename_chat_id = None;
                self.rename_input.clear();
                self.hovered_chat_id = None;
            }
            Message::CancelRenameChat => {
                self.rename_chat_id = None;
                self.rename_input.clear();
            }
            Message::DeleteChat(chat_id) => {
                if self
                    .inflight_request
                    .as_ref()
                    .map(|request| request.chat_id)
                    == Some(chat_id)
                {
                    self.abort_inflight_request();
                    self.inflight_request = None;
                    self.loading_chat_id = None;
                    self.loading_phase = 0;
                }
                self.delete_chat(chat_id);
                self.hovered_chat_id = None;
                if self.chat_error.as_ref().map(|error| error.chat_id) == Some(chat_id) {
                    self.chat_error = None;
                }
                self.persist_state();
            }
            Message::OpenSettings => {
                self.settings_form = SettingsForm::from_settings(&self.state.settings);
                self.settings_modal = None;
                self.add_model_provider_index = self.state.settings.provider.index();
                self.add_model_name.clear();
                self.system_prompt_content =
                    widget::text_editor::Content::with_text(&self.state.settings.system_prompt);
                self.connection_test_state = ConnectionTestState::Idle;
                self.panel_view = PanelView::Settings;
            }
            Message::CloseSettings => {
                self.settings_modal = None;
                self.panel_view = PanelView::Chat;
                return self.scroll_messages_to_end(true);
            }
            Message::ChatScrolled(viewport) => {
                self.messages_bottom_distance = viewport.absolute_offset_reversed().y;
            }
            Message::ComposerEdited(action) => {
                self.composer_content.perform(action);
            }
            Message::InlineEditEdited(action) => {
                self.editing_content.perform(action);
            }
            Message::MessageViewerEdited(message_id, action) => {
                self.perform_message_view_action(message_id, action);
            }
            Message::MarkdownLink(_uri) => {}
            Message::SubmitComposer => {
                return self.submit_message();
            }
            Message::SaveEditedMessage => {
                return self.save_edited_message();
            }
            Message::CancelEditedMessage => {
                self.reset_inline_edit();
            }
            Message::StopGeneration => {
                return self.stop_generation();
            }
            Message::LoadingTick => {
                self.loading_phase = (self.loading_phase + 1) % 6;
            }
            Message::ProviderEvent(event) => match event {
                provider::ProviderEvent::Delta {
                    request_id,
                    chat_id,
                    delta,
                } => {
                    self.handle_provider_delta(request_id, chat_id, delta);
                    return self.scroll_messages_to_end(false);
                }
                provider::ProviderEvent::Finished {
                    request_id,
                    chat_id,
                } => {
                    self.handle_provider_finished(request_id, chat_id);
                    return self.scroll_messages_to_end(false);
                }
                provider::ProviderEvent::Failed {
                    request_id,
                    chat_id,
                    error,
                } => {
                    self.handle_provider_failed(request_id, chat_id, error);
                    return self.scroll_messages_to_end(false);
                }
            },
            Message::CopyMessage(message_id) => {
                if let Some(content) = self.message_content_by_id(message_id) {
                    return self.copy_to_clipboard(CopiedTarget::Message(message_id), content);
                }
            }
            Message::CopyCodeBlock {
                message_id,
                block_index,
                content,
            } => {
                return self.copy_to_clipboard(
                    CopiedTarget::CodeBlock {
                        message_id,
                        block_index,
                    },
                    content,
                );
            }
            Message::ResetCopiedTarget(target) => {
                if self.copied_target.as_ref() == Some(&target) {
                    self.copied_target = None;
                }
            }
            Message::RetryRequest(chat_id) => {
                return self.retry_request(chat_id);
            }
            Message::RegenerateLastAssistant(message_id) => {
                return self.regenerate_last_assistant(message_id);
            }
            Message::EditUserMessage(message_id) => {
                return self.edit_user_message(message_id);
            }
            Message::DeleteLastAssistant(message_id) => {
                return self.delete_last_assistant(message_id);
            }
            Message::BranchConversation(message_id) => {
                return self.branch_conversation(message_id);
            }
            Message::ProviderSelected(index) => {
                self.settings_form
                    .select_provider(ProviderKind::from_index(index));
                self.connection_test_state = ConnectionTestState::Idle;
            }
            Message::OpenRouterKeyChanged(value) => {
                self.settings_form.openrouter_api_key = value;
                self.connection_test_state = ConnectionTestState::Idle;
            }
            Message::LmStudioUrlChanged(value) => {
                self.settings_form.lmstudio_base_url = value;
                self.connection_test_state = ConnectionTestState::Idle;
            }
            Message::ContextLimitChanged(value) => {
                self.settings_form.context_message_limit = value;
            }
            Message::DefaultModelSelected(index) => {
                self.settings_form.select_default_model(index);
            }
            Message::ActiveModelSelected(index) => {
                if let Some(model) = self.state.settings.saved_models.get(index).cloned() {
                    if let Some(chat) = self.active_chat_mut() {
                        chat.provider = model.provider;
                        chat.model = model.name;
                        chat.touch();
                    }
                    self.clear_chat_error(self.state.active_chat_id);
                    self.persist_state();
                }
            }
            Message::OpenAddModelModal => {
                self.settings_modal = Some(SettingsModal::AddModel);
                self.add_model_provider_index = self.settings_form.provider().index();
                self.add_model_name.clear();
            }
            Message::CloseSettingsModal => {
                self.settings_modal = None;
            }
            Message::AddModelProviderSelected(index) => {
                self.add_model_provider_index = index;
            }
            Message::AddModelNameChanged(value) => {
                self.add_model_name = value;
            }
            Message::SaveAddedModel => {
                if self.settings_form.add_model(
                    ProviderKind::from_index(self.add_model_provider_index),
                    &self.add_model_name,
                ) {
                    self.settings_modal = None;
                    self.add_model_name.clear();
                }
            }
            Message::RemoveSavedModel(index) => {
                self.settings_form.remove_model(index);
            }
            Message::OpenSystemPromptModal => {
                self.system_prompt_content =
                    widget::text_editor::Content::with_text(&self.settings_form.system_prompt);
                self.settings_modal = Some(SettingsModal::SystemPrompt);
            }
            Message::SystemPromptEdited(action) => {
                self.system_prompt_content.perform(action);
            }
            Message::SaveSystemPrompt => {
                self.settings_form.system_prompt = self.system_prompt_content.text();
                self.settings_modal = None;
            }
            Message::TestConnection => {
                return self.test_connection();
            }
            Message::ConnectionTestFinished(result) => {
                self.connection_test_state = match result {
                    Ok(()) => ConnectionTestState::Success,
                    Err(error) => ConnectionTestState::Failed(error),
                };
            }
            Message::SaveSettings => {
                let context_limit_valid = self
                    .settings_form
                    .apply_to_settings(&mut self.state.settings);
                if !context_limit_valid {
                    self.settings_form.context_message_limit =
                        self.state.settings.context_message_limit.to_string();
                    self.status = Some("Context limit must be a whole number.".into());
                }
                if let Err(error) =
                    secrets::save_openrouter_api_key(&self.state.settings.openrouter_api_key)
                {
                    self.status = Some(error);
                }
                self.settings_modal = None;
                self.connection_test_state = ConnectionTestState::Idle;
                self.clear_chat_error(self.state.active_chat_id);
                self.panel_view = PanelView::Chat;
                self.persist_state();
            }
            Message::TogglePanel => {
                return if let Some(id) = self.panel_window.take() {
                    self.panel_requested_open = false;
                    self.panel_view = PanelView::Chat;
                    self.reset_inline_edit();
                    self.hovered_message_id = None;
                    destroy_layer_surface(id)
                } else {
                    self.open_panel()
                };
            }
            Message::EscapePressed(id) => {
                if self.panel_window == Some(id) {
                    if self.panel_view != PanelView::Chat {
                        self.panel_view = PanelView::Chat;
                    } else {
                        self.panel_requested_open = false;
                        self.panel_window = None;
                        self.reset_inline_edit();
                        self.hovered_message_id = None;
                        return destroy_layer_surface(id);
                    }
                }
            }
            Message::PanelClosed(id) => {
                if self.panel_window == Some(id) {
                    self.panel_window = None;
                    if self.panel_requested_open {
                        return self.open_panel();
                    }
                }
            }
        }
        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

impl AppModel {
    fn default_state() -> Self {
        let (provider_events_tx, provider_events_rx) = unbounded_channel();
        let provider_events_rx = Arc::new(Mutex::new(provider_events_rx));
        let _ = PROVIDER_EVENTS_RX.get_or_init(|| provider_events_rx.clone());

        Self {
            core: Core::default(),
            panel_window: None,
            panel_requested_open: false,
            config: Config::default(),
            panel_view: PanelView::Chat,
            state: PersistedState::default(),
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

    fn reset_composer(&mut self) {
        self.composer_content = widget::text_editor::Content::new();
    }

    fn reset_inline_edit(&mut self) {
        self.editing_message_id = None;
        self.editing_content = widget::text_editor::Content::new();
    }

    fn set_transient_chat_notice(&mut self, chat_id: u64, message: impl Into<String>) {
        self.transient_chat_notice = Some(TransientChatNotice {
            chat_id,
            message: message.into(),
        });
    }

    fn clear_transient_chat_notice(&mut self, chat_id: u64) {
        if self
            .transient_chat_notice
            .as_ref()
            .map(|notice| notice.chat_id)
            == Some(chat_id)
        {
            self.transient_chat_notice = None;
        }
    }

    fn active_transient_chat_notice(&self) -> Option<&str> {
        self.transient_chat_notice
            .as_ref()
            .filter(|notice| notice.chat_id == self.state.active_chat_id)
            .map(|notice| notice.message.as_str())
    }

    fn sync_message_view_content(&mut self, message_id: u64, content: &str) {
        let needs_refresh = self
            .message_view_text
            .get(&message_id)
            .map(|current| current != content)
            .unwrap_or(true);

        if needs_refresh {
            self.message_view_text
                .insert(message_id, content.to_string());
            self.message_view_content
                .insert(message_id, widget::text_editor::Content::with_text(content));
        }
    }

    fn rebuild_message_view_cache(&mut self) {
        let mut live_ids = HashSet::new();

        let messages: Vec<(u64, String)> = self
            .state
            .chats
            .iter()
            .flat_map(|chat| {
                chat.messages
                    .iter()
                    .map(|message| (message.id, message.content.clone()))
            })
            .collect();

        for (message_id, content) in messages {
            live_ids.insert(message_id);
            self.sync_message_view_content(message_id, &content);
        }

        self.message_view_content
            .retain(|message_id, _| live_ids.contains(message_id));
        self.message_view_text
            .retain(|message_id, _| live_ids.contains(message_id));
    }

    fn perform_message_view_action(
        &mut self,
        message_id: u64,
        action: widget::text_editor::Action,
    ) {
        use widget::text_editor::Action;

        let Some(content) = self.message_view_content.get_mut(&message_id) else {
            return;
        };

        match action {
            Action::Move(_)
            | Action::Select(_)
            | Action::SelectWord
            | Action::SelectLine
            | Action::SelectAll
            | Action::Click(_)
            | Action::Drag(_)
            | Action::Scroll { .. } => content.perform(action),
            Action::Edit(_) => {}
        }
    }

    fn composer_text(&self) -> String {
        self.composer_content.text()
    }

    fn open_panel(&mut self) -> Task<cosmic::Action<Message>> {
        let id = Id::unique();
        self.panel_window = Some(id);
        self.panel_requested_open = true;
        self.messages_bottom_distance = 0.0;

        Task::batch([
            get_layer_surface::<cosmic::Action<Message>>(SctkLayerSurfaceSettings {
                id,
                layer: Layer::Top,
                keyboard_interactivity: KeyboardInteractivity::OnDemand,
                anchor: Anchor::RIGHT.union(Anchor::TOP).union(Anchor::BOTTOM),
                namespace: <Self as cosmic::Application>::APP_ID.to_string(),
                margin: panel_reserved_margin(&self.core),
                size: Some((Some(PANEL_WIDTH as u32), None)),
                exclusive_zone: -1,
                size_limits: Limits::NONE
                    .min_width(PANEL_WIDTH)
                    .min_height(PANEL_MIN_HEIGHT),
                ..Default::default()
            }),
            self.scroll_messages_to_end(true),
        ])
    }

    fn handle_applet_pressed(&mut self) -> Task<cosmic::Action<Message>> {
        match self.panel_window {
            Some(id) => {
                self.panel_requested_open = false;
                self.panel_window = None;
                self.panel_view = PanelView::Chat;
                self.hovered_message_id = None;
                destroy_layer_surface(id)
            }
            None => self.open_panel(),
        }
    }

    fn active_chat(&self) -> Option<&ChatSession> {
        self.state
            .chats
            .iter()
            .find(|chat| chat.id == self.state.active_chat_id)
    }

    fn active_chat_mut(&mut self) -> Option<&mut ChatSession> {
        self.state
            .chats
            .iter_mut()
            .find(|chat| chat.id == self.state.active_chat_id)
    }

    fn message_content_by_id(&self, message_id: u64) -> Option<String> {
        self.active_chat()?
            .messages
            .iter()
            .find(|message| message.id == message_id)
            .map(|message| message.content.clone())
    }

    fn last_assistant_message_id(&self, chat_id: u64) -> Option<u64> {
        self.state
            .chats
            .iter()
            .find(|chat| chat.id == chat_id)?
            .messages
            .iter()
            .rev()
            .find(|message| message.role == ChatRole::Assistant)
            .map(|message| message.id)
    }

    fn create_chat(&mut self) -> u64 {
        let chat_id = self.state.next_chat_id;
        self.state.next_chat_id += 1;

        let provider = self.state.settings.provider;
        let model = self.state.settings.active_model().to_string();

        self.state
            .chats
            .push(ChatSession::new(chat_id, provider, model));
        chat_id
    }

    fn delete_chat(&mut self, chat_id: u64) {
        self.state.chats.retain(|chat| chat.id != chat_id);

        if self.state.chats.is_empty() {
            let new_chat_id = self.create_chat();
            self.state.active_chat_id = new_chat_id;
        } else if self.state.active_chat_id == chat_id {
            self.state.active_chat_id = self.state.chats[0].id;
        }

        if self.rename_chat_id == Some(chat_id) {
            self.rename_chat_id = None;
            self.rename_input.clear();
        }

        self.rebuild_markdown_cache();
        self.rebuild_message_view_cache();
    }

    fn persist_state(&mut self) {
        match storage::save_state(&self.state) {
            Ok(()) => {}
            Err(error) => {
                self.status = Some(format!("{}: {error}", fl!("status-save-failed")));
            }
        }
    }

    fn next_message_id(&mut self) -> u64 {
        let message_id = self.state.next_message_id;
        self.state.next_message_id += 1;
        message_id
    }

    fn rebuild_markdown_cache(&mut self) {
        self.assistant_markdown.clear();

        for chat in &self.state.chats {
            for message in &chat.messages {
                if message.role == ChatRole::Assistant {
                    self.assistant_markdown.insert(
                        message.id,
                        widget::markdown::Content::parse(&message.content),
                    );
                }
            }
        }
    }

    fn message_viewer<'a>(&'a self, message: &'a ChatMessage) -> Option<Element<'a, Message>> {
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

    fn user_message_text_width(&self, content: &str) -> f32 {
        type PlainParagraph = cosmic::iced_core::text::paragraph::Plain<
            <cosmic::Renderer as cosmic::iced_core::text::Renderer>::Paragraph,
        >;

        let paragraph = PlainParagraph::new(core_text::Text {
            content: content.to_string(),
            bounds: Size::new(f32::INFINITY, f32::INFINITY),
            size: Pixels(MESSAGE_TEXT_SIZE_PX),
            line_height: core_text::LineHeight::default(),
            font: Font::default(),
            align_x: core_text::Alignment::Left,
            align_y: alignment::Vertical::Top,
            shaping: core_text::Shaping::Advanced,
            wrapping: core_text::Wrapping::None,
            ellipsize: core_text::Ellipsize::None,
        });

        (paragraph.min_width() + USER_MESSAGE_EDITOR_WIDTH_BUFFER).clamp(
            1.0,
            USER_MESSAGE_BUBBLE_WIDTH - USER_MESSAGE_HORIZONTAL_PADDING,
        )
    }

    fn has_selected_model(&self) -> bool {
        self.active_chat()
            .map(|chat| !chat.model.trim().is_empty())
            .unwrap_or(false)
    }

    fn active_model_label(&self) -> String {
        let Some(chat) = self.active_chat() else {
            return "No model selected".into();
        };
        let model = chat.model.trim();
        if model.is_empty() {
            "No model selected".into()
        } else {
            let trimmed = model
                .rsplit('/')
                .next()
                .filter(|segment| !segment.is_empty())
                .unwrap_or(model);
            match chat.provider {
                ProviderKind::LmStudio => format!("{trimmed} (local)"),
                ProviderKind::OpenRouter => trimmed.to_string(),
            }
        }
    }

    fn active_model_options(&self) -> Vec<String> {
        self.state
            .settings
            .saved_models
            .iter()
            .map(SavedModel::chat_dropdown_label)
            .collect()
    }

    fn active_model_index(&self) -> Option<usize> {
        let chat = self.active_chat()?;
        let selected = SavedModel::normalized(chat.provider, &chat.model)?;
        self.state
            .settings
            .saved_models
            .iter()
            .position(|model| model == &selected)
    }

    fn test_connection(&mut self) -> Task<cosmic::Action<Message>> {
        self.connection_test_state = ConnectionTestState::Testing;

        let client = self.provider_client.clone();
        let provider = self.settings_form.provider();
        let endpoint = self.settings_form.lmstudio_base_url.clone();
        let api_key = Some(self.settings_form.openrouter_api_key.clone());

        cosmic::task::future(async move {
            Message::ConnectionTestFinished(
                provider::test_connection(client, provider, endpoint, api_key).await,
            )
        })
    }

    fn settings_connection_status(&self) -> Option<Element<'_, Message>> {
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

    fn saved_model_row(&self, index: usize, model: &SavedModel) -> Element<'_, Message> {
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

    fn settings_modal_overlay(&self) -> Option<Element<'_, Message>> {
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

    fn can_follow_chat(&self) -> bool {
        self.messages_bottom_distance <= CHAT_AUTOSCROLL_THRESHOLD_PX
    }

    fn scroll_messages_to_end(&mut self, force: bool) -> Task<cosmic::Action<Message>> {
        if force {
            self.messages_bottom_distance = 0.0;
        }

        if force || self.can_follow_chat() {
            cosmic::iced::widget::operation::snap_to_end(self.messages_scroll_id.clone())
        } else {
            Task::none()
        }
    }

    fn copy_to_clipboard(
        &mut self,
        target: CopiedTarget,
        content: String,
    ) -> Task<cosmic::Action<Message>> {
        self.copied_target = Some(target.clone());

        Task::batch([
            cosmic::iced::clipboard::write::<cosmic::Action<Message>>(content),
            cosmic::task::future(async move {
                tokio::time::sleep(Duration::from_secs(5)).await;
                Message::ResetCopiedTarget(target)
            }),
        ])
    }

    fn submit_message(&mut self) -> Task<cosmic::Action<Message>> {
        if self.inflight_request.is_some() {
            return Task::none();
        }

        if !self.has_selected_model() {
            return Task::none();
        }

        let prompt = self.composer_text().trim().to_string();
        if prompt.is_empty() {
            return Task::none();
        }

        if self.active_chat().is_none() {
            let chat_id = self.create_chat();
            self.state.active_chat_id = chat_id;
        }

        let active_chat_id = self.state.active_chat_id;
        self.clear_chat_error(active_chat_id);

        let user_message = ChatMessage::new(self.next_message_id(), ChatRole::User, &prompt);
        let user_message_id = user_message.id;

        if let Some(chat) = self.active_chat_mut() {
            chat.messages.push(user_message);
            if chat.title.starts_with("New Chat") {
                chat.title = summarize_title(&prompt);
            }
            chat.touch();
        }

        self.clear_transient_chat_notice(active_chat_id);
        self.reset_composer();
        self.sync_message_view_content(user_message_id, &prompt);
        self.persist_state();

        let request = match self.build_provider_request(active_chat_id) {
            Ok(request) => request,
            Err(error) => {
                self.chat_error = Some(ChatErrorState {
                    chat_id: active_chat_id,
                    message: error,
                    request: ProviderRequest {
                        provider: self.state.settings.provider,
                        endpoint: String::new(),
                        api_key: None,
                        model: String::new(),
                        messages: Vec::new(),
                    },
                    assistant_message_id: None,
                });
                return cosmic::iced::widget::operation::focus(self.composer_editor_id.clone());
            }
        };

        self.start_provider_request(active_chat_id, request);

        Task::batch([
            cosmic::iced::widget::operation::focus(self.composer_editor_id.clone()),
            self.scroll_messages_to_end(true),
        ])
    }

    fn build_provider_request(&self, chat_id: u64) -> Result<ProviderRequest, String> {
        let chat = self
            .state
            .chats
            .iter()
            .find(|chat| chat.id == chat_id)
            .ok_or_else(|| "Chat not found.".to_string())?;

        provider::build_request(
            &self.state.settings,
            Some(self.state.settings.openrouter_api_key.as_str()),
            chat,
        )
    }

    fn start_provider_request(&mut self, chat_id: u64, request: ProviderRequest) {
        self.abort_inflight_request();

        let request_id = self.next_request_id;
        self.next_request_id += 1;
        self.loading_chat_id = Some(chat_id);
        self.loading_phase = 0;
        self.chat_error = None;
        self.inflight_request = Some(InflightRequest {
            request_id,
            chat_id,
            request: request.clone(),
            assistant_message_id: None,
        });

        let client = self.provider_client.clone();
        let tx = self.provider_events_tx.clone();
        self.provider_task = Some(tokio::spawn(async move {
            provider::stream_chat(client, request_id, chat_id, request, tx).await;
        }));
    }

    fn retry_request(&mut self, chat_id: u64) -> Task<cosmic::Action<Message>> {
        if self.inflight_request.is_some() {
            return Task::none();
        }

        let Some(error) = self.chat_error.clone() else {
            return Task::none();
        };
        if error.chat_id != chat_id {
            return Task::none();
        }

        let request = match self.build_provider_request(chat_id) {
            Ok(request) => request,
            Err(message) => {
                self.chat_error = Some(ChatErrorState {
                    chat_id,
                    message,
                    request: ProviderRequest {
                        provider: self.state.settings.provider,
                        endpoint: String::new(),
                        api_key: None,
                        model: String::new(),
                        messages: Vec::new(),
                    },
                    assistant_message_id: error.assistant_message_id,
                });
                return Task::none();
            }
        };

        self.start_provider_request(chat_id, request);
        Task::none()
    }

    fn handle_provider_delta(&mut self, request_id: u64, chat_id: u64, delta: String) {
        if delta.is_empty() {
            return;
        }

        let Some((inflight_request_id, inflight_chat_id)) = self
            .inflight_request
            .as_ref()
            .map(|request| (request.request_id, request.chat_id))
        else {
            return;
        };
        if inflight_request_id != request_id || inflight_chat_id != chat_id {
            return;
        }

        self.loading_chat_id = None;
        self.clear_transient_chat_notice(chat_id);

        if let Some(message_id) = self
            .inflight_request
            .as_ref()
            .and_then(|request| request.assistant_message_id)
        {
            let mut next_content = None;
            if let Some(chat) = self.state.chats.iter_mut().find(|chat| chat.id == chat_id) {
                if let Some(message) = chat
                    .messages
                    .iter_mut()
                    .find(|message| message.id == message_id)
                {
                    message.content.push_str(&delta);
                    next_content = Some(message.content.clone());
                    self.assistant_markdown
                        .entry(message_id)
                        .or_insert_with(|| widget::markdown::Content::new())
                        .push_str(&delta);
                    chat.touch();
                }
            }
            if let Some(content) = next_content {
                self.sync_message_view_content(message_id, &content);
            }
            return;
        }

        let assistant_message_id = self.next_message_id();
        let assistant_message = ChatMessage::new(assistant_message_id, ChatRole::Assistant, delta);
        if let Some(chat) = self.state.chats.iter_mut().find(|chat| chat.id == chat_id) {
            chat.messages.push(assistant_message);
            chat.touch();
        }
        let assistant_content = self
            .state
            .chats
            .iter()
            .find(|chat| chat.id == chat_id)
            .and_then(|chat| chat.messages.last())
            .map(|message| message.content.clone())
            .unwrap_or_default();
        self.sync_message_view_content(assistant_message_id, &assistant_content);
        self.assistant_markdown.insert(
            assistant_message_id,
            widget::markdown::Content::parse(
                &self
                    .state
                    .chats
                    .iter()
                    .find(|chat| chat.id == chat_id)
                    .and_then(|chat| chat.messages.last())
                    .map(|message| message.content.as_str())
                    .unwrap_or_default(),
            ),
        );
        if let Some(inflight) = self.inflight_request.as_mut() {
            inflight.assistant_message_id = Some(assistant_message_id);
        }
    }

    fn handle_provider_finished(&mut self, request_id: u64, chat_id: u64) {
        if self
            .inflight_request
            .as_ref()
            .map(|request| (request.request_id, request.chat_id))
            != Some((request_id, chat_id))
        {
            return;
        }

        self.provider_task = None;
        self.inflight_request = None;
        self.loading_chat_id = None;
        self.loading_phase = 0;
        self.persist_state();
    }

    fn handle_provider_failed(&mut self, request_id: u64, chat_id: u64, error: String) {
        let Some(inflight) = self.inflight_request.take() else {
            return;
        };
        if inflight.request_id != request_id || inflight.chat_id != chat_id {
            self.inflight_request = Some(inflight);
            return;
        }

        self.provider_task = None;
        self.loading_chat_id = None;
        self.loading_phase = 0;
        self.chat_error = Some(ChatErrorState {
            chat_id,
            message: error,
            request: inflight.request,
            assistant_message_id: inflight.assistant_message_id,
        });
        self.persist_state();
    }

    fn clear_chat_error(&mut self, chat_id: u64) {
        if self.chat_error.as_ref().map(|error| error.chat_id) == Some(chat_id) {
            self.chat_error = None;
        }
    }

    fn remove_message_ids(&mut self, message_ids: &[u64]) {
        for message_id in message_ids {
            self.assistant_markdown.remove(message_id);
            self.message_view_content.remove(message_id);
            self.message_view_text.remove(message_id);
        }
        if let Some(copied) = &self.copied_target {
            let should_clear = match copied {
                CopiedTarget::Message(message_id) => message_ids.contains(message_id),
                CopiedTarget::CodeBlock { message_id, .. } => message_ids.contains(message_id),
            };
            if should_clear {
                self.copied_target = None;
            }
        }
        if let Some(hovered) = self.hovered_message_id {
            if message_ids.contains(&hovered) {
                self.hovered_message_id = None;
            }
        }
    }

    fn regenerate_last_assistant(&mut self, message_id: u64) -> Task<cosmic::Action<Message>> {
        if self.inflight_request.is_some() {
            return Task::none();
        }

        let chat_id = self.state.active_chat_id;
        if self.last_assistant_message_id(chat_id) != Some(message_id) {
            return Task::none();
        }

        let mut removed = false;
        if let Some(chat) = self.state.chats.iter_mut().find(|chat| chat.id == chat_id)
            && let Some(last) = chat.messages.last()
            && last.id == message_id
            && last.role == ChatRole::Assistant
        {
            chat.messages.pop();
            chat.touch();
            removed = true;
        }

        if !removed {
            return Task::none();
        }

        self.remove_message_ids(&[message_id]);
        self.clear_chat_error(chat_id);

        let request = match self.build_provider_request(chat_id) {
            Ok(request) => request,
            Err(error) => {
                self.chat_error = Some(ChatErrorState {
                    chat_id,
                    message: error,
                    request: ProviderRequest {
                        provider: self.state.settings.provider,
                        endpoint: String::new(),
                        api_key: None,
                        model: String::new(),
                        messages: Vec::new(),
                    },
                    assistant_message_id: None,
                });
                self.persist_state();
                return Task::none();
            }
        };

        self.persist_state();
        self.start_provider_request(chat_id, request);
        self.scroll_messages_to_end(true)
    }

    fn edit_user_message(&mut self, message_id: u64) -> Task<cosmic::Action<Message>> {
        if self.inflight_request.is_some() {
            return Task::none();
        }

        let chat_id = self.state.active_chat_id;
        let Some(content) = self
            .state
            .chats
            .iter()
            .find(|chat| chat.id == chat_id)
            .and_then(|chat| {
                chat.messages
                    .iter()
                    .find(|message| message.id == message_id && message.role == ChatRole::User)
            })
            .map(|message| message.content.clone())
        else {
            return Task::none();
        };

        self.editing_message_id = Some(message_id);
        self.clear_chat_error(chat_id);
        self.editing_content = widget::text_editor::Content::with_text(&content);

        Task::none()
    }

    fn save_edited_message(&mut self) -> Task<cosmic::Action<Message>> {
        if self.inflight_request.is_some() {
            return Task::none();
        }

        let chat_id = self.state.active_chat_id;
        let Some(message_id) = self.editing_message_id else {
            return Task::none();
        };

        let edited_text = self.editing_content.text().trim().to_string();
        if edited_text.is_empty() {
            return cosmic::iced::widget::operation::focus(self.editing_editor_id.clone());
        }

        let mut removed_ids = Vec::new();
        let mut updated = false;
        if let Some(chat) = self.state.chats.iter_mut().find(|chat| chat.id == chat_id)
            && let Some(index) = chat
                .messages
                .iter()
                .position(|message| message.id == message_id && message.role == ChatRole::User)
        {
            removed_ids.extend(chat.messages[index + 1..].iter().map(|message| message.id));
            if let Some(message) = chat.messages.get_mut(index) {
                message.content = edited_text.clone();
            }
            chat.messages.truncate(index + 1);
            if index == 0 {
                chat.title = summarize_title(&edited_text);
            }
            chat.touch();
            updated = true;
        }

        if !updated {
            self.reset_inline_edit();
            return Task::none();
        }

        self.remove_message_ids(&removed_ids);
        self.sync_message_view_content(message_id, &edited_text);
        self.clear_chat_error(chat_id);
        self.clear_transient_chat_notice(chat_id);
        self.reset_inline_edit();
        self.persist_state();

        let request = match self.build_provider_request(chat_id) {
            Ok(request) => request,
            Err(error) => {
                self.chat_error = Some(ChatErrorState {
                    chat_id,
                    message: error,
                    request: ProviderRequest {
                        provider: self.state.settings.provider,
                        endpoint: String::new(),
                        api_key: None,
                        model: String::new(),
                        messages: Vec::new(),
                    },
                    assistant_message_id: None,
                });
                return Task::none();
            }
        };

        self.start_provider_request(chat_id, request);
        self.scroll_messages_to_end(true)
    }

    fn delete_last_assistant(&mut self, message_id: u64) -> Task<cosmic::Action<Message>> {
        if self.inflight_request.is_some() {
            return Task::none();
        }

        let chat_id = self.state.active_chat_id;
        if self.last_assistant_message_id(chat_id) != Some(message_id) {
            return Task::none();
        }

        let mut removed = false;
        if let Some(chat) = self.state.chats.iter_mut().find(|chat| chat.id == chat_id)
            && let Some(last) = chat.messages.last()
            && last.id == message_id
            && last.role == ChatRole::Assistant
        {
            chat.messages.pop();
            chat.touch();
            removed = true;
        }

        if !removed {
            return Task::none();
        }

        self.remove_message_ids(&[message_id]);
        self.clear_chat_error(chat_id);
        self.persist_state();

        self.scroll_messages_to_end(false)
    }

    fn branch_conversation(&mut self, message_id: u64) -> Task<cosmic::Action<Message>> {
        if self.inflight_request.is_some() {
            return Task::none();
        }

        let source_chat_id = self.state.active_chat_id;
        if self.last_assistant_message_id(source_chat_id) != Some(message_id) {
            return Task::none();
        }

        let Some(source_chat) = self
            .state
            .chats
            .iter()
            .find(|chat| chat.id == source_chat_id)
            .cloned()
        else {
            return Task::none();
        };

        let Some(branch_end) = source_chat
            .messages
            .iter()
            .position(|message| message.id == message_id)
        else {
            return Task::none();
        };

        let new_chat_id = self.state.next_chat_id;
        self.state.next_chat_id += 1;

        let mut branched_chat =
            ChatSession::new(new_chat_id, source_chat.provider, source_chat.model.clone());
        branched_chat.title = format!("{} (Branch)", source_chat.title);
        branched_chat.messages = source_chat.messages[..=branch_end]
            .iter()
            .cloned()
            .map(|mut message| {
                message.id = self.next_message_id();
                message
            })
            .collect();
        branched_chat.touch();

        self.state.chats.push(branched_chat);
        self.state.active_chat_id = new_chat_id;
        self.panel_view = PanelView::Chat;
        self.set_transient_chat_notice(
            new_chat_id,
            format!(
                "Branch created: {}",
                self.active_chat()
                    .map(|chat| chat.title.as_str())
                    .unwrap_or("New chat")
            ),
        );
        self.reset_inline_edit();
        self.reset_composer();
        self.rebuild_markdown_cache();
        self.rebuild_message_view_cache();
        self.persist_state();

        Task::batch([
            cosmic::iced::widget::operation::focus(self.composer_editor_id.clone()),
            self.scroll_messages_to_end(true),
        ])
    }

    fn abort_inflight_request(&mut self) {
        if let Some(task) = self.provider_task.take() {
            task.abort();
        }
    }

    fn stop_generation(&mut self) -> Task<cosmic::Action<Message>> {
        if self.inflight_request.is_none() {
            return Task::none();
        }

        let stopped_chat_id = self
            .inflight_request
            .as_ref()
            .map(|request| request.chat_id);
        self.abort_inflight_request();
        self.inflight_request = None;
        self.loading_chat_id = None;
        self.loading_phase = 0;
        if let Some(chat_id) = stopped_chat_id {
            self.clear_chat_error(chat_id);
        }
        self.persist_state();

        Task::batch([
            cosmic::iced::widget::operation::focus(self.composer_editor_id.clone()),
            self.scroll_messages_to_end(false),
        ])
    }

    fn chat_screen(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let model_selected = self.has_selected_model();
        let is_generating = self
            .inflight_request
            .as_ref()
            .map(|request| request.chat_id)
            == Some(self.state.active_chat_id);
        let composer_has_text = !self.composer_text().trim().is_empty();
        let composer_is_multiline = self.composer_content.line_count() > 1;
        let placeholder = if model_selected {
            fl!("composer-placeholder")
        } else {
            "Choose a model in settings".into()
        };

        let composer_editor_width =
            (PANEL_WIDTH - (spacing.space_m as f32 * 4.0) - (spacing.space_xs as f32) - 36.0)
                .max(160.0);

        let messages = scrollable(self.message_column())
            .anchor_bottom()
            .id(self.messages_scroll_id.clone())
            .on_scroll(Message::ChatScrolled)
            .class(cosmic::style::iced::Scrollable::Minimal)
            .direction(thin_vertical_scrollbar())
            .height(Length::Fill)
            .width(Length::Fill);

        let composer_editor = widget::text_editor(&self.composer_content)
            .id(self.composer_editor_id.clone())
            .placeholder(placeholder)
            .on_action(Message::ComposerEdited)
            .key_binding(composer_key_binding)
            .padding([11, 0, 0, 0])
            .height(Length::Shrink)
            .min_height(COMPOSER_LINE_HEIGHT + 6.0)
            .max_height(COMPOSER_MAX_HEIGHT)
            .wrapping(core_text::Wrapping::WordOrGlyph)
            .class(composer_editor_class())
            .width(composer_editor_width);

        let mut send_button =
            button::custom(
                container(
                    widget::text::body(if is_generating { "■" } else { "↑" })
                        .size(if is_generating { 14 } else { 20 }),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill),
            )
            .width(Length::Fixed(36.0))
            .height(Length::Fixed(36.0))
            .padding(0)
            .class(send_button_class());

        if is_generating {
            send_button = send_button.on_press(Message::StopGeneration);
        } else if model_selected && composer_has_text {
            send_button = send_button.on_press(Message::SubmitComposer);
        }

        let composer = container(
            row![container(composer_editor).width(Length::Fill), send_button]
                .spacing(spacing.space_xs)
                .width(Length::Fill)
                .align_y(if composer_is_multiline {
                    Alignment::End
                } else {
                    Alignment::Center
                }),
        )
        .padding([spacing.space_s, spacing.space_m])
        .class(composer_container_class());

        let mut content = widget::column().spacing(spacing.space_m);
        content = content.push(messages).push(composer);

        if let Some(status) = &self.status {
            content = content.push(widget::text::caption(status));
        }

        content
            .padding(spacing.space_m)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn chat_model_header(&self) -> Element<'_, Message> {
        let tone = if self.has_selected_model() {
            Color::from_rgba(1.0, 1.0, 1.0, 0.82)
        } else {
            Color::from_rgb(1.0, 0.42, 0.42)
        };

        if self.state.settings.saved_models.is_empty() {
            return widget::text::body(self.active_model_label())
                .class(cosmic::theme::Text::Color(tone))
                .width(Length::Fixed(CHAT_MODEL_DROPDOWN_WIDTH))
                .align_x(alignment::Horizontal::Center)
                .into();
        }

        widget::dropdown(
            self.active_model_options(),
            self.active_model_index(),
            Message::ActiveModelSelected,
        )
        .width(Length::Fixed(CHAT_MODEL_DROPDOWN_WIDTH))
        .padding([8, 18, 8, 16])
        .into()
    }

    fn chats_screen(&self) -> Element<'_, Message> {
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

    fn settings_screen(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let provider_options = SettingsForm::provider_labels();
        let mut test_button = button::standard("Test connection");
        if !matches!(self.connection_test_state, ConnectionTestState::Testing) {
            test_button = test_button.on_press(Message::TestConnection);
        }

        let provider_control = widget::dropdown(
            provider_options,
            Some(self.settings_form.provider_index),
            Message::ProviderSelected,
        )
        .padding([8, 0, 8, 16]);

        let provider_section = match self.settings_form.provider() {
            ProviderKind::OpenRouter => widget::settings::section()
                .title("Provider")
                .add(widget::settings::item("Provider", provider_control))
                .add(widget::settings::item(
                    "API key",
                    column![
                        container(
                            widget::text_input::secure_input(
                                "sk-or-...",
                                &self.settings_form.openrouter_api_key,
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
                                &self.settings_form.lmstudio_base_url,
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
        for (index, model) in self.settings_form.saved_models.iter().enumerate() {
            saved_models_list = saved_models_list.push(self.saved_model_row(index, model));
        }

        let saved_models_list: Element<'_, Message> = if self.settings_form.saved_models.len() > 5 {
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

        let default_model_section = if self.settings_form.saved_models.is_empty() {
            widget::settings::section().title("Default model").add(
                widget::text::caption("Add at least one model").class(cosmic::theme::Text::Color(
                    Color::from_rgba(1.0, 1.0, 1.0, 0.56),
                )),
            )
        } else {
            let options = self.settings_form.default_model_options();
            widget::settings::section()
                .title("Default model")
                .add(widget::settings::item(
                    "Default model",
                    widget::dropdown(
                        options,
                        self.settings_form.default_model_index(),
                        Message::DefaultModelSelected,
                    )
                    .padding([8, 0, 8, 16]),
                ))
        };

        let context_limit_invalid = self
            .settings_form
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
                    &self.settings_form.context_message_limit,
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

    fn message_column(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();

        let Some(chat) = self.active_chat() else {
            return container(text(""))
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        };

        if chat.messages.is_empty() {
            if self.loading_chat_id != Some(chat.id) {
                return container(text(""))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into();
            }
        }

        let mut messages = widget::column()
            .spacing(spacing.space_m)
            .width(Length::Fill);
        for message in &chat.messages {
            messages = messages.push(self.message_card(message));
        }

        if let Some(notice) = self.active_transient_chat_notice() {
            messages = messages.push(self.transient_chat_notice_card(notice));
        }

        if self.chat_error.as_ref().map(|error| error.chat_id) == Some(chat.id) {
            messages = messages.push(self.error_notice());
        }

        if self.loading_chat_id == Some(chat.id) {
            messages = messages.push(self.loading_indicator());
        }

        messages.into()
    }

    fn message_card<'a>(&'a self, message: &'a ChatMessage) -> Element<'a, Message> {
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
                        .into()
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

    fn message_actions_row<'a>(&'a self, message: &'a ChatMessage) -> Option<Element<'a, Message>> {
        let spacing = cosmic::theme::spacing();
        let side_gutter = spacing.space_s;
        let Some(chat) = self.active_chat() else {
            return None;
        };

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

                if is_last_assistant && !actions_locked {
                    buttons.push(self.message_action_button(
                        "view-refresh-symbolic",
                        Some(Message::RegenerateLastAssistant(message.id)),
                        true,
                        false,
                    ));
                    buttons.push(self.message_action_button(
                        "object-merge-symbolic",
                        Some(Message::BranchConversation(message.id)),
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
            _ => return None,
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

    fn message_action_button(
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

        if visible {
            if let Some(on_press) = on_press {
                button = button.on_press(on_press);
            }
        }

        container(button)
            .width(Length::Fixed(frame_size))
            .center_x(Length::Fixed(frame_size))
            .into()
    }

    fn chat_action_buttons(&self, chat_id: u64) -> Element<'_, Message> {
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

    fn loading_indicator(&self) -> Element<'_, Message> {
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

    fn error_notice(&self) -> Element<'_, Message> {
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

    fn transient_chat_notice_card(&self, notice: &str) -> Element<'_, Message> {
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

fn summarize_title(prompt: &str) -> String {
    let trimmed = prompt.trim();
    let mut summary = trimmed.chars().take(36).collect::<String>();
    if trimmed.chars().count() > 36 {
        summary.push_str("...");
    }
    if summary.is_empty() {
        "New chat".into()
    } else {
        summary
    }
}

fn composer_key_binding(
    key_press: widget::text_editor::KeyPress,
) -> Option<widget::text_editor::Binding<Message>> {
    editor_key_binding(key_press, Message::SubmitComposer, None)
}

fn message_edit_key_binding(
    key_press: widget::text_editor::KeyPress,
) -> Option<widget::text_editor::Binding<Message>> {
    editor_key_binding(
        key_press,
        Message::SaveEditedMessage,
        Some(Message::CancelEditedMessage),
    )
}

fn editor_key_binding(
    key_press: widget::text_editor::KeyPress,
    submit_message: Message,
    escape_message: Option<Message>,
) -> Option<widget::text_editor::Binding<Message>> {
    use widget::text_editor::{Binding, Motion};

    let modifiers = key_press.modifiers;

    match key_press.key.to_latin(key_press.physical_key) {
        Some('c') | Some('C') if modifiers.command() => return Some(Binding::Copy),
        Some('x') | Some('X') if modifiers.command() => return Some(Binding::Cut),
        Some('v') | Some('V') if modifiers.command() && !modifiers.alt() => {
            return Some(Binding::Paste);
        }
        Some('a') | Some('A') if modifiers.command() => {
            return Some(Binding::SelectAll);
        }
        _ => {}
    }

    let jump_modifier = modifiers.control() || modifiers.alt();

    if let keyboard::Key::Named(named) = key_press.key.as_ref() {
        let motion = match named {
            keyboard::key::Named::ArrowLeft => Some(if jump_modifier {
                Motion::WordLeft
            } else {
                Motion::Left
            }),
            keyboard::key::Named::ArrowRight => Some(if jump_modifier {
                Motion::WordRight
            } else {
                Motion::Right
            }),
            keyboard::key::Named::ArrowUp => Some(Motion::Up),
            keyboard::key::Named::ArrowDown => Some(Motion::Down),
            keyboard::key::Named::Home => Some(if jump_modifier {
                Motion::DocumentStart
            } else {
                Motion::Home
            }),
            keyboard::key::Named::End => Some(if jump_modifier {
                Motion::DocumentEnd
            } else {
                Motion::End
            }),
            keyboard::key::Named::PageUp => Some(Motion::PageUp),
            keyboard::key::Named::PageDown => Some(Motion::PageDown),
            _ => None,
        };

        if let Some(motion) = motion {
            return Some(if modifiers.shift() {
                Binding::Select(motion)
            } else {
                Binding::Move(motion)
            });
        }

        match named {
            keyboard::key::Named::Enter if !modifiers.shift() => {
                return Some(Binding::Custom(submit_message));
            }
            keyboard::key::Named::Backspace if jump_modifier => {
                return Some(Binding::Sequence(vec![
                    Binding::Select(Motion::WordLeft),
                    Binding::Delete,
                ]));
            }
            keyboard::key::Named::Delete if jump_modifier => {
                return Some(Binding::Sequence(vec![
                    Binding::Select(Motion::WordRight),
                    Binding::Delete,
                ]));
            }
            keyboard::key::Named::Backspace => return Some(Binding::Backspace),
            keyboard::key::Named::Delete => return Some(Binding::Delete),
            keyboard::key::Named::Escape => {
                return Some(if let Some(message) = escape_message {
                    Binding::Custom(message)
                } else {
                    Binding::Unfocus
                });
            }
            _ => {}
        }
    }

    widget::text_editor::Binding::from_key_press(key_press)
}

fn send_button_class() -> cosmic::theme::Button {
    cosmic::theme::Button::Custom {
        active: Box::new(|focused, theme| send_button_style(focused, theme, 1.0)),
        disabled: Box::new(|theme| send_button_style(false, theme, 0.7)),
        hovered: Box::new(|focused, theme| send_button_style(focused, theme, 0.94)),
        pressed: Box::new(|focused, theme| send_button_style(focused, theme, 0.88)),
    }
}

fn composer_container_class() -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::Custom(Box::new(|theme| {
        let cosmic = theme.cosmic();
        cosmic::iced::widget::container::Style {
            icon_color: Some(Color::from(theme.current_container().on)),
            text_color: Some(Color::from(theme.current_container().on)),
            background: Some(Background::Color(
                theme.current_container().component.base.into(),
            )),
            border: cosmic::iced_core::Border {
                radius: cosmic.corner_radii.radius_l.into(),
                width: 1.0,
                color: theme.current_container().component.divider.into(),
            },
            shadow: Default::default(),
            snap: true,
        }
    }))
}

fn composer_editor_class() -> cosmic::theme::iced::TextEditor<'static> {
    cosmic::theme::iced::TextEditor::Custom(Box::new(|theme, _status| {
        let cosmic = theme.cosmic();

        cosmic::iced::widget::text_editor::Style {
            background: Background::Color(Color::TRANSPARENT),
            border: cosmic::iced_core::Border {
                radius: cosmic.corner_radii.radius_0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            placeholder: cosmic.palette.neutral_9.with_alpha(0.7).into(),
            value: theme.current_container().on.into(),
            selection: cosmic.accent.base.with_alpha(0.3).into(),
        }
    }))
}

fn message_viewer_class(value_color: Color) -> cosmic::theme::iced::TextEditor<'static> {
    cosmic::theme::iced::TextEditor::Custom(Box::new(move |theme, _status| {
        let cosmic = theme.cosmic();

        cosmic::iced::widget::text_editor::Style {
            background: Background::Color(Color::TRANSPARENT),
            border: cosmic::iced_core::Border {
                radius: cosmic.corner_radii.radius_0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            placeholder: Color::TRANSPARENT,
            value: value_color,
            selection: cosmic.accent.base.with_alpha(0.32).into(),
        }
    }))
}

fn message_action_button_class(visible: bool) -> cosmic::theme::Button {
    if visible {
        cosmic::theme::Button::Icon
    } else {
        cosmic::theme::Button::Custom {
            active: Box::new(|_, theme| hidden_message_action_button_style(theme)),
            disabled: Box::new(hidden_message_action_button_style),
            hovered: Box::new(|_, theme| hidden_message_action_button_style(theme)),
            pressed: Box::new(|_, theme| hidden_message_action_button_style(theme)),
        }
    }
}

fn hidden_message_action_button_style(theme: &cosmic::Theme) -> cosmic::widget::button::Style {
    let cosmic = theme.cosmic();

    cosmic::widget::button::Style {
        shadow_offset: Vector::default(),
        background: None,
        overlay: None,
        border_radius: cosmic.corner_radii.radius_m.into(),
        border_width: 0.0,
        border_color: Color::TRANSPARENT,
        outline_width: 0.0,
        outline_color: Color::TRANSPARENT,
        icon_color: Some(Color::TRANSPARENT),
        text_color: Some(Color::TRANSPARENT),
    }
}

fn chat_list_card_class() -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::Custom(Box::new(|theme| {
        let cosmic = theme.cosmic();
        let background = theme.current_container().component.base;
        let border = theme.current_container().component.divider;

        cosmic::iced::widget::container::Style {
            icon_color: Some(Color::from(theme.current_container().on)),
            text_color: Some(Color::from(theme.current_container().on)),
            background: Some(Background::Color(background.into())),
            border: cosmic::iced_core::Border {
                radius: cosmic.corner_radii.radius_l.into(),
                width: 1.0,
                color: border.into(),
            },
            shadow: Default::default(),
            snap: true,
        }
    }))
}

fn settings_modal_backdrop_class() -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::Custom(Box::new(|_theme| cosmic::iced::widget::container::Style {
        icon_color: None,
        text_color: None,
        background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.42))),
        border: cosmic::iced_core::Border {
            radius: 0.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        shadow: Default::default(),
        snap: true,
    }))
}

fn loading_dot_class() -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::Custom(Box::new(|_theme| cosmic::iced::widget::container::Style {
        icon_color: Some(Color::WHITE),
        text_color: Some(Color::WHITE),
        background: Some(Background::Color(Color::WHITE)),
        border: cosmic::iced_core::Border {
            radius: 999.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        shadow: Default::default(),
        snap: true,
    }))
}

fn error_notice_class() -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::Custom(Box::new(|theme| {
        let cosmic = theme.cosmic();
        let background = Color::from_rgba(0.36, 0.08, 0.08, 0.32);
        let border = Color::from_rgba(1.0, 0.42, 0.42, 0.28);

        cosmic::iced::widget::container::Style {
            icon_color: Some(Color::from_rgb(1.0, 0.42, 0.42)),
            text_color: Some(Color::from(theme.current_container().on)),
            background: Some(Background::Color(background)),
            border: cosmic::iced_core::Border {
                radius: cosmic.corner_radii.radius_xl.into(),
                width: 1.0,
                color: border,
            },
            shadow: Default::default(),
            snap: true,
        }
    }))
}

fn transient_chat_notice_class() -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::Custom(Box::new(|theme| {
        let cosmic = theme.cosmic();

        cosmic::iced::widget::container::Style {
            icon_color: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.82)),
            text_color: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.82)),
            background: Some(Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.08))),
            border: cosmic::iced_core::Border {
                radius: cosmic.corner_radii.radius_xl.into(),
                width: 1.0,
                color: Color::from_rgba(1.0, 1.0, 1.0, 0.12),
            },
            shadow: Default::default(),
            snap: true,
        }
    }))
}

fn composer_input_class() -> cosmic::theme::TextInput {
    cosmic::theme::TextInput::Custom {
        active: Box::new(|theme: &cosmic::Theme| composer_input_appearance(theme)),
        error: Box::new(|theme: &cosmic::Theme| composer_input_appearance(theme)),
        hovered: Box::new(|theme: &cosmic::Theme| composer_input_appearance(theme)),
        focused: Box::new(|theme: &cosmic::Theme| composer_input_appearance(theme)),
        disabled: Box::new(|theme: &cosmic::Theme| composer_input_appearance(theme)),
    }
}

fn composer_input_appearance(theme: &cosmic::Theme) -> cosmic::widget::text_input::Appearance {
    let cosmic = theme.cosmic();

    cosmic::widget::text_input::Appearance {
        background: Color::TRANSPARENT.into(),
        border_radius: cosmic.corner_radii.radius_0.into(),
        border_width: 0.0,
        border_offset: None,
        border_color: Color::TRANSPARENT,
        icon_color: None,
        text_color: Some(theme.current_container().on.into()),
        placeholder_color: cosmic.palette.neutral_9.with_alpha(0.7).into(),
        selected_text_color: cosmic.on_accent_color().into(),
        selected_fill: cosmic.accent_color().into(),
        label_color: cosmic.palette.neutral_9.into(),
    }
}

fn chat_bubble_class(_user: bool) -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::Custom(Box::new(move |theme| {
        let cosmic = theme.cosmic();
        let background: Color = theme.current_container().component.base.into();
        let text_color: Color = theme.current_container().on.into();
        let border_color: Color = theme.current_container().component.divider.into();

        cosmic::iced::widget::container::Style {
            icon_color: Some(Color::from(text_color)),
            text_color: Some(Color::from(text_color)),
            background: Some(Background::Color(background)),
            border: cosmic::iced_core::Border {
                radius: cosmic.corner_radii.radius_l.into(),
                width: 1.0,
                color: border_color,
            },
            shadow: Default::default(),
            snap: true,
        }
    }))
}

fn chat_row_button_class(selected: bool) -> cosmic::theme::Button {
    cosmic::theme::Button::Custom {
        active: Box::new(move |focused, theme| chat_row_button_style(focused, theme, selected, 0)),
        disabled: Box::new(|theme| {
            let base = theme.active(false, false, &cosmic::theme::Button::Text);
            cosmic::widget::button::Style {
                shadow_offset: Vector::default(),
                background: None,
                overlay: None,
                border_radius: theme.cosmic().corner_radii.radius_l.into(),
                border_width: 0.0,
                border_color: Color::TRANSPARENT,
                outline_width: 0.0,
                outline_color: Color::TRANSPARENT,
                icon_color: base.icon_color,
                text_color: base.text_color,
            }
        }),
        hovered: Box::new(move |focused, theme| chat_row_button_style(focused, theme, selected, 1)),
        pressed: Box::new(move |focused, theme| chat_row_button_style(focused, theme, selected, 2)),
    }
}

fn chat_row_button_style(
    focused: bool,
    theme: &cosmic::Theme,
    selected: bool,
    state: u8,
) -> cosmic::widget::button::Style {
    let cosmic = theme.cosmic();
    let base = theme.active(focused, selected, &cosmic::theme::Button::Text);
    let hover_bg = theme.current_container().component.hover;
    let pressed_bg = theme.current_container().component.pressed;
    let selected_bg = cosmic.accent.base.with_alpha(0.18);
    let background = if selected {
        selected_bg.into()
    } else {
        match state {
            1 => hover_bg.into(),
            2 => pressed_bg.into(),
            _ => Color::TRANSPARENT,
        }
    };
    let (outline_width, outline_color) = if focused {
        (1.0, cosmic.accent_color().into())
    } else {
        (0.0, Color::TRANSPARENT)
    };

    cosmic::widget::button::Style {
        shadow_offset: Vector::default(),
        background: Some(Background::Color(background)),
        overlay: None,
        border_radius: cosmic.corner_radii.radius_l.into(),
        border_width: 0.0,
        border_color: Color::TRANSPARENT,
        outline_width,
        outline_color,
        icon_color: base.icon_color,
        text_color: if selected {
            Some(cosmic.accent_text_color().into())
        } else {
            base.text_color
        },
    }
}

fn send_button_style(
    focused: bool,
    theme: &cosmic::Theme,
    shade: f32,
) -> cosmic::widget::button::Style {
    let cosmic = theme.cosmic();
    let (outline_width, outline_color) = if focused {
        (1.0, cosmic.accent_color().into())
    } else {
        (0.0, Color::TRANSPARENT)
    };

    cosmic::widget::button::Style {
        shadow_offset: Vector::default(),
        background: Some(Background::Color(Color::from_rgb(shade, shade, shade))),
        overlay: None,
        border_radius: cosmic.corner_radii.radius_xl.into(),
        border_width: 0.0,
        border_color: Color::TRANSPARENT,
        outline_width,
        outline_color,
        icon_color: Some(Color::BLACK),
        text_color: Some(Color::BLACK),
    }
}

fn thin_vertical_scrollbar() -> cosmic::iced::widget::scrollable::Direction {
    cosmic::iced::widget::scrollable::Direction::Vertical(
        cosmic::iced::widget::scrollable::Scrollbar::new()
            .width(THIN_SCROLLBAR_WIDTH)
            .scroller_width(THIN_SCROLLER_WIDTH)
            .spacing(2),
    )
}
