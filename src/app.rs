// SPDX-License-Identifier: MPL-2.0

mod chat_actions;
mod chats_actions;
mod dispatch;
mod lifecycle;
mod message;
mod model;
mod provider_events;
mod runtime;
mod settings_actions;
mod settings_state;
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
use message::Message;
use reqwest::Client;
pub(in crate::app) use settings_state::{
    AiMigrationState, ConnectionTestState, PromptPreviewMode, SettingsForm, SettingsModal,
    SettingsNotice, SettingsTab, SettingsUiState, SettingsValidationError, TextEditorModal,
};
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
    AiMigrationPrompt,
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
    settings_ui: SettingsUiState,
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
        Self::init_app(core)
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
        self.app_subscription()
    }

    /// Handles messages emitted by the application and its widgets.
    ///
    /// Tasks may be returned for asynchronous execution of code in the background
    /// on the application's async runtime. The application will not exit until all
    /// tasks are finished.
    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        self.handle_message(message)
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
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
        Message::SaveInlineEdit,
        Some(Message::CancelInlineEdit),
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
