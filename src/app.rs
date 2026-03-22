// SPDX-License-Identifier: MPL-2.0

use crate::chat::{AppSettings, ChatMessage, ChatRole, ChatSession, ProviderKind};
use crate::config::Config;
use crate::fl;
use crate::provider::{self, ProviderRequest};
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
use cosmic::iced_core::{Background, Color, Event, Size, Vector};
use cosmic::iced_futures::futures::{self, SinkExt};
use cosmic::iced_futures::stream;
use cosmic::iced_widget::{column, container, row, scrollable, text};
use cosmic::prelude::*;
use cosmic::widget::button::Catalog;
use cosmic::widget::{self, button, header_bar};
use reqwest::Client;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

const PANEL_WIDTH: f32 = 420.0;
const PANEL_MIN_HEIGHT: f32 = 560.0;
const CHAT_ACTIONS_WIDTH: f32 = 84.0;
const COMPOSER_LINE_HEIGHT: f32 = 28.0;
const COMPOSER_MAX_HEIGHT: f32 = PANEL_MIN_HEIGHT * 0.5;
const COMPOSER_SOFT_WRAP_LIMIT: f32 = 300.0;
const THIN_SCROLLBAR_WIDTH: f32 = 4.0;
const THIN_SCROLLER_WIDTH: f32 = 3.0;
const LOADING_TICK_MS: u64 = 120;

static PROVIDER_EVENTS_RX: OnceLock<Arc<Mutex<UnboundedReceiver<provider::ProviderEvent>>>> =
    OnceLock::new();

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
enum PanelView {
    #[default]
    Chat,
    Chats,
    Settings,
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
enum ComposerBreak {
    #[default]
    Hard,
    SoftSpace,
    SoftNone,
}

#[derive(Debug, Clone, Default)]
struct SettingsForm {
    provider_index: usize,
    openrouter_api_key: String,
    openrouter_model: String,
    lmstudio_model: String,
    lmstudio_base_url: String,
    system_prompt: String,
    context_message_limit: String,
}

impl SettingsForm {
    fn from_settings(settings: &AppSettings) -> Self {
        Self {
            provider_index: settings.provider.index(),
            openrouter_api_key: String::new(),
            openrouter_model: settings.openrouter_model.clone(),
            lmstudio_model: settings.lmstudio_model.clone(),
            lmstudio_base_url: settings.lmstudio_base_url.clone(),
            system_prompt: settings.system_prompt.clone(),
            context_message_limit: settings.context_message_limit.to_string(),
        }
    }

    fn provider(&self) -> ProviderKind {
        ProviderKind::from_index(self.provider_index)
    }

    fn apply_to_settings(&self, settings: &mut AppSettings) {
        settings.provider = self.provider();
        settings.openrouter_model = self.openrouter_model.trim().to_string();
        settings.lmstudio_model = self.lmstudio_model.trim().to_string();
        settings.lmstudio_base_url = self.lmstudio_base_url.trim().to_string();
        settings.system_prompt = self.system_prompt.trim().to_string();
        settings.context_message_limit = self
            .context_message_limit
            .trim()
            .parse::<usize>()
            .unwrap_or(0);
    }
}

#[derive(Debug, Clone)]
struct InflightRequest {
    chat_id: u64,
    request: ProviderRequest,
    assistant_message_id: Option<u64>,
}

#[derive(Debug, Clone)]
struct ChatErrorState {
    chat_id: u64,
    message: String,
    request: ProviderRequest,
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
    config: Config,
    panel_view: PanelView,
    state: PersistedState,
    composer_lines: Vec<String>,
    composer_breaks: Vec<ComposerBreak>,
    composer_input_ids: Vec<widget::Id>,
    composer_focused_line: Option<usize>,
    rename_chat_id: Option<u64>,
    rename_input: String,
    hovered_chat_id: Option<u64>,
    settings_form: SettingsForm,
    provider_client: Client,
    provider_events_tx: UnboundedSender<provider::ProviderEvent>,
    inflight_request: Option<InflightRequest>,
    chat_error: Option<ChatErrorState>,
    loading_chat_id: Option<u64>,
    loading_phase: u8,
    status: Option<String>,
}

/// Messages emitted by the application and its widgets.
#[derive(Debug, Clone)]
pub enum Message {
    TogglePanel,
    PanelClosed(Id),
    EscapePressed(Id),
    UpdateConfig(Config),
    NewChat,
    ToggleChatList,
    SelectChat(u64),
    ChatHovered(u64),
    ChatUnhovered(u64),
    BeginRenameChat(u64),
    RenameInputChanged(String),
    CommitRenameChat(String),
    CancelRenameChat,
    DeleteChat(u64),
    OpenSettings,
    CloseSettings,
    ComposerLineChanged(usize, String),
    ComposerLineFocused(usize),
    ComposerLineUnfocused(usize),
    ComposerEnter(Id, bool),
    ComposerBackspace(Id),
    ComposerSelectAll(Id),
    SubmitComposer,
    LoadingTick,
    ProviderEvent(provider::ProviderEvent),
    RetryRequest(u64),
    ProviderSelected(usize),
    OpenRouterKeyChanged(String),
    OpenRouterModelChanged(String),
    LmStudioModelChanged(String),
    LmStudioUrlChanged(String),
    SystemPromptChanged(String),
    ContextLimitChanged(String),
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
        let state = storage::load_state();
        let settings_form = SettingsForm::from_settings(&state.settings);

        let app = AppModel {
            core,
            state,
            settings_form,
            config: cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
                .map(|context| match Config::get_entry(&context) {
                    Ok(config) => config,
                    Err((_errors, config)) => config,
                })
                .unwrap_or_default(),
            ..Self::default_state()
        };

        (app, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PanelClosed(id))
    }

    /// Describes the interface based on the current state of the application model.
    ///
    fn view(&self) -> Element<'_, Self::Message> {
        self.core
            .applet
            .icon_button("display-symbolic")
            .on_press(Message::TogglePanel)
            .into()
    }

    fn view_window(&self, id: Id) -> Element<'_, Self::Message> {
        if self.panel_window != Some(id) {
            return container(text("")).into();
        }

        let header_title = match self.panel_view {
            PanelView::Settings => fl!("settings-title"),
            PanelView::Chats => String::new(),
            PanelView::Chat => self
                .active_chat()
                .map(|chat| chat.title.clone())
                .unwrap_or_else(|| fl!("new-chat")),
        };
        let focused = self
            .core()
            .focused_window()
            .map(|focused_id| focused_id == id)
            .unwrap_or_default();
        let header = header_bar()
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
            .end(button::text("-").on_press(Message::TogglePanel));

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
            .width(Length::Fill)
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
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Character(c),
                    modifiers,
                    ..
                }) if c.eq_ignore_ascii_case("a") && modifiers.control() => {
                    Some(Message::ComposerSelectAll(id))
                }
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(keyboard::key::Named::Enter),
                    modifiers,
                    ..
                }) => Some(Message::ComposerEnter(id, modifiers.shift())),
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(keyboard::key::Named::Backspace),
                    ..
                }) => Some(Message::ComposerBackspace(id)),
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
            Message::UpdateConfig(config) => {
                self.config = config;
            }
            Message::NewChat => {
                let new_chat_id = self.create_chat();
                self.state.active_chat_id = new_chat_id;
                self.panel_view = PanelView::Chat;
                self.reset_composer();
                self.rename_chat_id = None;
                self.persist_state();
            }
            Message::ToggleChatList => {
                self.rename_chat_id = None;
                self.rename_input.clear();
                self.composer_focused_line = None;
                self.panel_view = if self.panel_view == PanelView::Chats {
                    PanelView::Chat
                } else {
                    PanelView::Chats
                };
            }
            Message::SelectChat(chat_id) => {
                self.state.active_chat_id = chat_id;
                self.panel_view = PanelView::Chat;
                self.rename_chat_id = None;
                self.hovered_chat_id = None;
            }
            Message::ChatHovered(chat_id) => {
                self.hovered_chat_id = Some(chat_id);
            }
            Message::ChatUnhovered(chat_id) => {
                if self.hovered_chat_id == Some(chat_id) {
                    self.hovered_chat_id = None;
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
                self.delete_chat(chat_id);
                self.hovered_chat_id = None;
                if self.chat_error.as_ref().map(|error| error.chat_id) == Some(chat_id) {
                    self.chat_error = None;
                }
                self.persist_state();
            }
            Message::OpenSettings => {
                self.panel_view = PanelView::Settings;
                self.composer_focused_line = None;
            }
            Message::CloseSettings => {
                self.panel_view = PanelView::Chat;
            }
            Message::ComposerLineChanged(index, value) => {
                return self.update_composer_line(index, value);
            }
            Message::ComposerLineFocused(index) => {
                self.composer_focused_line = Some(index);
            }
            Message::ComposerLineUnfocused(index) => {
                if self.composer_focused_line == Some(index) {
                    self.composer_focused_line = None;
                }
            }
            Message::ComposerEnter(id, shift) => {
                if self.panel_window == Some(id)
                    && self.panel_view == PanelView::Chat
                    && self.composer_focused_line.is_some()
                {
                    return if shift {
                        self.insert_composer_line_after_focus()
                    } else {
                        self.submit_message()
                    };
                }
            }
            Message::ComposerBackspace(id) => {
                if self.panel_window == Some(id)
                    && self.panel_view == PanelView::Chat
                    && self.composer_focused_line.is_some()
                {
                    return self.remove_empty_focused_composer_line();
                }
            }
            Message::ComposerSelectAll(id) => {
                if self.panel_window == Some(id)
                    && self.panel_view == PanelView::Chat
                    && self.composer_focused_line.is_some()
                {
                    if let Some(input_id) = self.focused_composer_input_id() {
                        return widget::text_input::select_all(input_id);
                    }
                }
            }
            Message::SubmitComposer => {
                return self.submit_message();
            }
            Message::LoadingTick => {
                self.loading_phase = (self.loading_phase + 1) % 6;
            }
            Message::ProviderEvent(event) => match event {
                provider::ProviderEvent::Delta { chat_id, delta } => {
                    self.handle_provider_delta(chat_id, delta);
                }
                provider::ProviderEvent::Finished { chat_id } => {
                    self.handle_provider_finished(chat_id);
                }
                provider::ProviderEvent::Failed { chat_id, error } => {
                    self.handle_provider_failed(chat_id, error);
                }
            },
            Message::RetryRequest(chat_id) => {
                return self.retry_request(chat_id);
            }
            Message::ProviderSelected(index) => {
                self.settings_form.provider_index = index;
            }
            Message::OpenRouterKeyChanged(value) => {
                self.settings_form.openrouter_api_key = value;
            }
            Message::OpenRouterModelChanged(value) => {
                self.settings_form.openrouter_model = value;
            }
            Message::LmStudioModelChanged(value) => {
                self.settings_form.lmstudio_model = value;
            }
            Message::LmStudioUrlChanged(value) => {
                self.settings_form.lmstudio_base_url = value;
            }
            Message::SystemPromptChanged(value) => {
                self.settings_form.system_prompt = value;
            }
            Message::ContextLimitChanged(value) => {
                self.settings_form.context_message_limit = value;
            }
            Message::SaveSettings => {
                self.settings_form
                    .apply_to_settings(&mut self.state.settings);
                self.panel_view = PanelView::Chat;
                self.persist_state();
            }
            Message::TogglePanel => {
                return if let Some(id) = self.panel_window.take() {
                    destroy_layer_surface(id)
                } else {
                    let id = Id::unique();
                    self.panel_window = Some(id);
                    get_layer_surface::<cosmic::Action<Self::Message>>(SctkLayerSurfaceSettings {
                        id,
                        layer: Layer::Top,
                        keyboard_interactivity: KeyboardInteractivity::OnDemand,
                        anchor: Anchor::RIGHT.union(Anchor::TOP).union(Anchor::BOTTOM),
                        namespace: Self::APP_ID.to_string(),
                        margin: panel_reserved_margin(&self.core),
                        size: Some((Some(PANEL_WIDTH as u32), None)),
                        // Ignore reserved panel/dock areas so the surface spans full output height.
                        exclusive_zone: -1,
                        size_limits: Limits::NONE
                            .min_width(PANEL_WIDTH)
                            .min_height(PANEL_MIN_HEIGHT),
                        ..Default::default()
                    })
                };
            }
            Message::EscapePressed(id) => {
                if self.panel_window == Some(id) {
                    if self.panel_view != PanelView::Chat {
                        self.panel_view = PanelView::Chat;
                        self.composer_focused_line = None;
                    } else {
                        self.panel_window = None;
                        return destroy_layer_surface(id);
                    }
                }
            }
            Message::PanelClosed(id) => {
                if self.panel_window == Some(id) {
                    self.panel_window = None;
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
            config: Config::default(),
            panel_view: PanelView::Chat,
            state: PersistedState::default(),
            composer_lines: vec![String::new()],
            composer_breaks: vec![ComposerBreak::Hard],
            composer_input_ids: vec![widget::Id::unique()],
            composer_focused_line: None,
            rename_chat_id: None,
            rename_input: String::new(),
            hovered_chat_id: None,
            settings_form: SettingsForm::default(),
            provider_client: Client::new(),
            provider_events_tx,
            inflight_request: None,
            chat_error: None,
            loading_chat_id: None,
            loading_phase: 0,
            status: None,
        }
    }

    fn reset_composer(&mut self) {
        self.composer_lines = vec![String::new()];
        self.composer_breaks = vec![ComposerBreak::Hard];
        self.composer_input_ids = vec![widget::Id::unique()];
        self.composer_focused_line = None;
    }

    fn composer_text(&self) -> String {
        let mut composed = String::new();
        for (index, line) in self.composer_lines.iter().enumerate() {
            if index > 0 {
                match self
                    .composer_breaks
                    .get(index)
                    .copied()
                    .unwrap_or(ComposerBreak::Hard)
                {
                    ComposerBreak::Hard => composed.push('\n'),
                    ComposerBreak::SoftSpace => composed.push(' '),
                    ComposerBreak::SoftNone => {}
                }
            }
            composed.push_str(line);
        }
        composed
    }

    fn update_composer_line(
        &mut self,
        index: usize,
        value: String,
    ) -> Task<cosmic::Action<Message>> {
        if index >= self.composer_lines.len() {
            return Task::none();
        }

        let parts: Vec<String> = value.split('\n').map(str::to_string).collect();
        self.composer_lines[index] = parts.first().cloned().unwrap_or_default();

        if parts.len() == 1 {
            return self.reflow_soft_paragraph(index);
        }

        let mut last_id = None;
        let mut insert_at = index + 1;
        for line in parts.into_iter().skip(1) {
            let id = widget::Id::unique();
            self.composer_lines.insert(insert_at, line);
            self.composer_breaks.insert(insert_at, ComposerBreak::Hard);
            self.composer_input_ids.insert(insert_at, id.clone());
            last_id = Some(id);
            insert_at += 1;
        }

        self.composer_focused_line = Some(insert_at.saturating_sub(1));

        if let Some(id) = last_id {
            widget::text_input::focus(id)
        } else {
            Task::none()
        }
    }

    fn insert_composer_line_after_focus(&mut self) -> Task<cosmic::Action<Message>> {
        let Some(index) = self.composer_focused_line else {
            return Task::none();
        };

        let new_index = (index + 1).min(self.composer_lines.len());
        let id = widget::Id::unique();
        self.composer_lines.insert(new_index, String::new());
        self.composer_breaks.insert(new_index, ComposerBreak::Hard);
        self.composer_input_ids.insert(new_index, id.clone());
        self.composer_focused_line = Some(new_index);

        widget::text_input::focus(id)
    }

    fn remove_empty_focused_composer_line(&mut self) -> Task<cosmic::Action<Message>> {
        let Some(index) = self.composer_focused_line else {
            return Task::none();
        };

        if index == 0 || self.composer_lines.len() == 1 || !self.composer_lines[index].is_empty() {
            return Task::none();
        }

        self.composer_lines.remove(index);
        self.composer_breaks.remove(index);
        self.composer_input_ids.remove(index);
        let focus_index = index - 1;
        self.composer_focused_line = Some(focus_index);

        widget::text_input::focus(self.composer_input_ids[focus_index].clone())
    }

    fn reflow_soft_paragraph(&mut self, index: usize) -> Task<cosmic::Action<Message>> {
        if index >= self.composer_lines.len() {
            return Task::none();
        }

        let mut start = index;
        while start > 0 && self.composer_breaks[start] != ComposerBreak::Hard {
            start -= 1;
        }

        let mut end = index;
        while end + 1 < self.composer_lines.len()
            && self.composer_breaks[end + 1] != ComposerBreak::Hard
        {
            end += 1;
        }

        let preserved_break = self.composer_breaks[start];
        let mut paragraph = self.composer_lines[start].clone();
        for line_index in (start + 1)..=end {
            match self.composer_breaks[line_index] {
                ComposerBreak::SoftSpace => paragraph.push(' '),
                ComposerBreak::SoftNone => {}
                ComposerBreak::Hard => {}
            }
            paragraph.push_str(&self.composer_lines[line_index]);
        }

        let mut anchor_offset = 0usize;
        for line_index in start..=index {
            if line_index > start && self.composer_breaks[line_index] == ComposerBreak::SoftSpace {
                anchor_offset += 1;
            }
            anchor_offset += self.composer_lines[line_index].chars().count();
        }

        let old_lines = self.composer_lines[start..=end].to_vec();
        let old_breaks = if start < end {
            self.composer_breaks[(start + 1)..=end].to_vec()
        } else {
            Vec::new()
        };
        let (new_lines, new_breaks) = wrap_soft_paragraph(&paragraph);

        if new_lines == old_lines && new_breaks == old_breaks {
            return Task::none();
        }

        let focus_relative = focus_line_for_offset(&new_lines, &new_breaks, anchor_offset);
        let new_focus_index = start + focus_relative;
        let old_ids = self.composer_input_ids[start..=end].to_vec();

        self.composer_lines.splice(start..=end, new_lines.clone());

        let mut breaks_to_insert = vec![preserved_break];
        breaks_to_insert.extend(new_breaks.iter().copied());
        self.composer_breaks.splice(start..=end, breaks_to_insert);

        let new_ids = (0..new_lines.len())
            .map(|line_index| {
                old_ids
                    .get(line_index)
                    .cloned()
                    .unwrap_or_else(widget::Id::unique)
            })
            .collect::<Vec<_>>();
        self.composer_input_ids.splice(start..=end, new_ids.clone());
        self.composer_focused_line = Some(new_focus_index);

        widget::text_input::focus(new_ids[focus_relative].clone())
    }

    fn focused_composer_input_id(&self) -> Option<widget::Id> {
        self.composer_focused_line
            .and_then(|index| self.composer_input_ids.get(index))
            .cloned()
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

    fn create_chat(&mut self) -> u64 {
        let chat_id = self.state.next_chat_id;
        self.state.next_chat_id += 1;

        let provider = self.settings_form.provider();
        let model = match provider {
            ProviderKind::OpenRouter => self.settings_form.openrouter_model.clone(),
            ProviderKind::LmStudio => self.settings_form.lmstudio_model.clone(),
        };

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

    fn submit_message(&mut self) -> Task<cosmic::Action<Message>> {
        if self.inflight_request.is_some() {
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

        let user_message = ChatMessage::new(self.next_message_id(), ChatRole::User, &prompt);
        let provider = self.state.settings.provider;
        let model = self.state.settings.active_model().to_string();
        let active_chat_id = self.state.active_chat_id;

        if let Some(chat) = self.active_chat_mut() {
            chat.provider = provider;
            chat.model = model;
            chat.messages.push(user_message);
            if chat.title.starts_with("New Chat") {
                chat.title = summarize_title(&prompt);
            }
            chat.touch();
        }

        self.reset_composer();
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
                });
                return if let Some(id) = self.composer_input_ids.first() {
                    widget::text_input::focus(id.clone())
                } else {
                    Task::none()
                };
            }
        };

        self.start_provider_request(active_chat_id, request);

        if let Some(id) = self.composer_input_ids.first() {
            widget::text_input::focus(id.clone())
        } else {
            Task::none()
        }
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
            Some(self.settings_form.openrouter_api_key.as_str()),
            chat,
        )
    }

    fn start_provider_request(&mut self, chat_id: u64, request: ProviderRequest) {
        self.loading_chat_id = Some(chat_id);
        self.loading_phase = 0;
        self.chat_error = None;
        self.inflight_request = Some(InflightRequest {
            chat_id,
            request: request.clone(),
            assistant_message_id: None,
        });

        let client = self.provider_client.clone();
        let tx = self.provider_events_tx.clone();
        tokio::spawn(async move {
            provider::stream_chat(client, chat_id, request, tx).await;
        });
    }

    fn retry_request(&mut self, chat_id: u64) -> Task<cosmic::Action<Message>> {
        if self.inflight_request.is_some() {
            return Task::none();
        }

        let Some(error) = self.chat_error.clone() else {
            return Task::none();
        };
        if error.chat_id != chat_id || error.request.endpoint.is_empty() {
            return Task::none();
        }

        self.start_provider_request(chat_id, error.request);
        Task::none()
    }

    fn handle_provider_delta(&mut self, chat_id: u64, delta: String) {
        if delta.is_empty() {
            return;
        }

        let Some(inflight_chat_id) = self
            .inflight_request
            .as_ref()
            .map(|request| request.chat_id)
        else {
            return;
        };
        if inflight_chat_id != chat_id {
            return;
        }

        self.loading_chat_id = None;

        if let Some(message_id) = self
            .inflight_request
            .as_ref()
            .and_then(|request| request.assistant_message_id)
        {
            if let Some(chat) = self.state.chats.iter_mut().find(|chat| chat.id == chat_id) {
                if let Some(message) = chat
                    .messages
                    .iter_mut()
                    .find(|message| message.id == message_id)
                {
                    message.content.push_str(&delta);
                    chat.touch();
                }
            }
            return;
        }

        let assistant_message_id = self.next_message_id();
        let assistant_message = ChatMessage::new(assistant_message_id, ChatRole::Assistant, delta);
        if let Some(chat) = self.state.chats.iter_mut().find(|chat| chat.id == chat_id) {
            chat.messages.push(assistant_message);
            chat.touch();
        }
        if let Some(inflight) = self.inflight_request.as_mut() {
            inflight.assistant_message_id = Some(assistant_message_id);
        }
    }

    fn handle_provider_finished(&mut self, chat_id: u64) {
        if self
            .inflight_request
            .as_ref()
            .map(|request| request.chat_id)
            != Some(chat_id)
        {
            return;
        }

        self.inflight_request = None;
        self.loading_chat_id = None;
        self.loading_phase = 0;
        self.persist_state();
    }

    fn handle_provider_failed(&mut self, chat_id: u64, error: String) {
        let Some(inflight) = self.inflight_request.take() else {
            return;
        };
        if inflight.chat_id != chat_id {
            return;
        }

        self.loading_chat_id = None;
        self.loading_phase = 0;
        self.chat_error = Some(ChatErrorState {
            chat_id,
            message: error,
            request: inflight.request,
        });
        self.persist_state();
    }

    fn chat_screen(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let messages = scrollable(self.message_column())
            .class(cosmic::style::iced::Scrollable::Minimal)
            .direction(thin_vertical_scrollbar())
            .height(Length::Fill)
            .width(Length::Fill);

        let mut composer_inputs = widget::column().spacing(0);
        for (index, line) in self.composer_lines.iter().enumerate() {
            let placeholder = if self.composer_lines.len() == 1 {
                fl!("composer-placeholder")
            } else {
                String::new()
            };

            let input = widget::text_input::text_input(placeholder, line)
                .id(self.composer_input_ids[index].clone())
                .padding([2, 0])
                .style(composer_input_class())
                .on_input(move |value| Message::ComposerLineChanged(index, value))
                .on_focus(Message::ComposerLineFocused(index))
                .on_unfocus(Message::ComposerLineUnfocused(index));

            composer_inputs = composer_inputs.push(input.width(Length::Fill));
        }

        let composer_height = (self.composer_lines.len() as f32 * COMPOSER_LINE_HEIGHT)
            .clamp(COMPOSER_LINE_HEIGHT, COMPOSER_MAX_HEIGHT);
        let composer_alignment = if self.composer_lines.len() == 1 {
            Alignment::Center
        } else {
            Alignment::End
        };
        let composer_editor = scrollable(composer_inputs)
            .class(cosmic::style::iced::Scrollable::Minimal)
            .direction(thin_vertical_scrollbar())
            .height(Length::Fixed(composer_height))
            .width(Length::Fill);

        let composer = container(
            row![
                composer_editor,
                button::custom(
                    container(widget::text::body("↑").size(20))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .center_x(Length::Fill)
                        .center_y(Length::Fill),
                )
                .width(Length::Fixed(36.0))
                .height(Length::Fixed(36.0))
                .padding(0)
                .class(send_button_class())
                .on_press(Message::SubmitComposer)
            ]
            .spacing(spacing.space_xs)
            .align_y(composer_alignment),
        )
        .padding(spacing.space_m)
        .class(composer_container_class());

        let mut content = widget::column().spacing(spacing.space_s);
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
        let provider = self.settings_form.provider();
        let provider_row = row![
            widget::radio(
                widget::text::body("OpenRouter"),
                ProviderKind::OpenRouter,
                Some(provider),
                |value| Message::ProviderSelected(value.index()),
            ),
            widget::radio(
                widget::text::body("LM Studio"),
                ProviderKind::LmStudio,
                Some(provider),
                |value| Message::ProviderSelected(value.index()),
            ),
        ]
        .spacing(spacing.space_m)
        .align_y(Alignment::Center);

        let mut content = column![
            button::icon(widget::icon::from_name("go-previous-symbolic").size(16))
                .on_press(Message::CloseSettings),
            widget::divider::horizontal::default(),
            widget::text::caption(fl!("provider-label")),
            provider_row,
        ]
        .spacing(spacing.space_s)
        .padding(spacing.space_m)
        .width(Length::Fill);

        match self.settings_form.provider() {
            ProviderKind::OpenRouter => {
                content = content
                    .push(widget::text::caption(fl!("openrouter-key")))
                    .push(
                        widget::text_input::secure_input(
                            "sk-or-...",
                            &self.settings_form.openrouter_api_key,
                            None,
                            true,
                        )
                        .on_input(Message::OpenRouterKeyChanged),
                    )
                    .push(widget::text::caption(fl!("openrouter-model")))
                    .push(
                        widget::text_input::text_input(
                            "openrouter model id",
                            &self.settings_form.openrouter_model,
                        )
                        .on_input(Message::OpenRouterModelChanged),
                    )
                    .push(widget::text::caption(fl!("session-note")));
            }
            ProviderKind::LmStudio => {
                content = content
                    .push(widget::text::caption(fl!("lmstudio-url")))
                    .push(
                        widget::text_input::text_input(
                            "http://127.0.0.1:1234/v1",
                            &self.settings_form.lmstudio_base_url,
                        )
                        .on_input(Message::LmStudioUrlChanged),
                    )
                    .push(widget::text::caption(fl!("lmstudio-model")))
                    .push(
                        widget::text_input::text_input(
                            "local model id",
                            &self.settings_form.lmstudio_model,
                        )
                        .on_input(Message::LmStudioModelChanged),
                    );
            }
        }

        let content = content
            .push(widget::text::caption(fl!("system-prompt")))
            .push(
                widget::text_input::text_input("System prompt", &self.settings_form.system_prompt)
                    .on_input(Message::SystemPromptChanged),
            )
            .push(widget::text::caption("Context message limit"))
            .push(
                widget::text_input::text_input(
                    "0 = unlimited, 10 = keep last 10",
                    &self.settings_form.context_message_limit,
                )
                .on_input(Message::ContextLimitChanged),
            )
            .push(
                row![
                    button::standard(fl!("save")).on_press(Message::SaveSettings),
                    button::text(fl!("cancel")).on_press(Message::CloseSettings),
                ]
                .spacing(spacing.space_s),
            );

        scrollable(content).height(Length::Fill).into()
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
            .spacing(spacing.space_s)
            .width(Length::Fill);
        for message in &chat.messages {
            messages = messages.push(self.message_card(message));
        }

        if self.chat_error.as_ref().map(|error| error.chat_id) == Some(chat.id) {
            messages = messages.push(self.error_notice());
        }

        if self.loading_chat_id == Some(chat.id) {
            messages = messages.push(self.loading_indicator());
        }

        messages.into()
    }

    fn message_card<'a>(&self, message: &'a ChatMessage) -> Element<'a, Message> {
        let spacing = cosmic::theme::spacing();
        match message.role {
            ChatRole::User => {
                let text_block = container(
                    widget::text::body(&message.content)
                        .class(cosmic::theme::Text::Color(Color::WHITE))
                        .wrapping(cosmic::iced::widget::text::Wrapping::Word),
                )
                .max_width(PANEL_WIDTH * 0.72);

                container(text_block)
                    .width(Length::Fill)
                    .align_x(alignment::Horizontal::Right)
                    .into()
            }
            ChatRole::Assistant | ChatRole::System => {
                let bubble = container(widget::text::body(&message.content))
                    .padding([spacing.space_s, spacing.space_m])
                    .max_width(PANEL_WIDTH * 0.72)
                    .class(chat_bubble_class(false));

                container(bubble)
                    .width(Length::Fill)
                    .align_x(alignment::Horizontal::Left)
                    .into()
            }
        }
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

        let retry_button: Element<'_, Message> = if error.request.endpoint.is_empty() {
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

fn wrap_soft_paragraph(text: &str) -> (Vec<String>, Vec<ComposerBreak>) {
    if text.is_empty() {
        return (vec![String::new()], vec![]);
    }

    let mut remaining = text.to_string();
    let mut lines = Vec::new();
    let mut breaks = Vec::new();

    loop {
        let Some((head, tail, break_kind)) = split_soft_wrap(&remaining, COMPOSER_SOFT_WRAP_LIMIT)
        else {
            lines.push(remaining);
            break;
        };

        lines.push(head);
        breaks.push(break_kind);
        remaining = tail;
    }

    (lines, breaks)
}

fn focus_line_for_offset(
    lines: &[String],
    breaks: &[ComposerBreak],
    anchor_offset: usize,
) -> usize {
    let mut traversed = 0usize;

    for (index, line) in lines.iter().enumerate() {
        traversed += line.chars().count();
        if anchor_offset <= traversed {
            return index;
        }

        if let Some(break_kind) = breaks.get(index) {
            if *break_kind == ComposerBreak::SoftSpace {
                traversed += 1;
            }
        }
    }

    lines.len().saturating_sub(1)
}

fn split_soft_wrap(text: &str, limit: f32) -> Option<(String, String, ComposerBreak)> {
    let mut last_space = None;
    let mut candidate = String::new();

    for (char_index, ch) in text.chars().enumerate() {
        candidate.push(ch);
        if ch.is_whitespace() {
            last_space = Some(char_index);
        }

        if measure_text_width(&candidate) > limit {
            if let Some(space_index) = last_space.filter(|index| *index > 0) {
                let split_byte = char_to_byte_index(text, space_index);
                let head = text[..split_byte].trim_end().to_string();
                let tail = text[split_byte..].trim_start().to_string();
                if !head.is_empty() && !tail.is_empty() {
                    return Some((head, tail, ComposerBreak::SoftSpace));
                }
            }

            let split_byte = char_to_byte_index(text, char_index);
            let head = text[..split_byte].to_string();
            let tail = text[split_byte..].to_string();
            if !head.is_empty() && !tail.is_empty() {
                return Some((head, tail, ComposerBreak::SoftNone));
            }
            return None;
        }
    }

    None
}

fn measure_text_width(text: &str) -> f32 {
    type WrapParagraph = cosmic::iced_core::text::paragraph::Plain<
        <cosmic::Renderer as cosmic::iced_core::text::Renderer>::Paragraph,
    >;

    let mut paragraph = WrapParagraph::default();
    paragraph.update(core_text::Text {
        content: text,
        bounds: Size::new(f32::MAX, f32::MAX),
        size: cosmic::iced::Pixels(14.0),
        line_height: core_text::LineHeight::default(),
        font: cosmic::font::default(),
        align_x: core_text::Alignment::Left,
        align_y: alignment::Vertical::Top,
        shaping: core_text::Shaping::Advanced,
        wrapping: core_text::Wrapping::None,
        ellipsize: core_text::Ellipsize::None,
    });
    paragraph.min_width()
}

fn char_to_byte_index(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(byte_index, _)| byte_index)
        .unwrap_or(text.len())
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
