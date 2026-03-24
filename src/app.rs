// SPDX-License-Identifier: MPL-2.0

mod chat_actions;
mod chats_actions;
mod lifecycle;
mod model;
mod provider_events;
mod settings_actions;
mod style;
mod views;

use crate::chat::{AppSettings, ChatMessage, ChatRole, ChatSession, ProviderKind, SavedModel};
use crate::config::Config;
use crate::fl;
use crate::provider::{self, ProviderRequest};
use crate::secrets;
use crate::storage::{self, AppState};
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
use cosmic::iced_widget::{column, container, text};
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
    state: AppState,
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
            .icon_button_from_handle(widget::icon::from_svg_bytes(
                include_bytes!("../resources/cosmic-ai-panel.svg").as_slice(),
            ))
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
                return self.handle_new_chat();
            }
            Message::ToggleChatList => {
                self.toggle_chat_list();
            }
            Message::SelectChat(chat_id) => {
                return self.select_chat(chat_id);
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
                self.begin_rename_chat(chat_id);
            }
            Message::RenameInputChanged(value) => {
                self.rename_input = value;
            }
            Message::CommitRenameChat(value) => {
                self.commit_rename_chat(value);
            }
            Message::CancelRenameChat => {
                self.cancel_rename_chat();
            }
            Message::DeleteChat(chat_id) => {
                self.delete_chat_action(chat_id);
            }
            Message::OpenSettings => {
                self.open_settings();
            }
            Message::CloseSettings => {
                return self.close_settings();
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
                self.select_settings_provider(index);
            }
            Message::OpenRouterKeyChanged(value) => {
                self.set_openrouter_key(value);
            }
            Message::LmStudioUrlChanged(value) => {
                self.set_lmstudio_url(value);
            }
            Message::ContextLimitChanged(value) => {
                self.set_context_limit(value);
            }
            Message::DefaultModelSelected(index) => {
                self.select_default_model(index);
            }
            Message::ActiveModelSelected(index) => {
                self.select_active_model(index);
            }
            Message::OpenAddModelModal => {
                self.open_add_model_modal();
            }
            Message::CloseSettingsModal => {
                self.close_settings_modal();
            }
            Message::AddModelProviderSelected(index) => {
                self.select_add_model_provider(index);
            }
            Message::AddModelNameChanged(value) => {
                self.set_add_model_name(value);
            }
            Message::SaveAddedModel => {
                self.save_added_model();
            }
            Message::RemoveSavedModel(index) => {
                self.remove_saved_model(index);
            }
            Message::OpenSystemPromptModal => {
                self.open_system_prompt_modal();
            }
            Message::SystemPromptEdited(action) => {
                self.edit_system_prompt(action);
            }
            Message::SaveSystemPrompt => {
                self.save_system_prompt();
            }
            Message::TestConnection => {
                return self.test_connection();
            }
            Message::ConnectionTestFinished(result) => {
                self.finish_connection_test(result);
            }
            Message::SaveSettings => {
                self.save_settings_and_close();
            }
            Message::TogglePanel => {
                return self.toggle_panel();
            }
            Message::EscapePressed(id) => {
                return self.handle_escape_pressed(id);
            }
            Message::PanelClosed(id) => {
                return self.handle_panel_closed(id);
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
