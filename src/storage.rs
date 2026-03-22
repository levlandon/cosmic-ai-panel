// SPDX-License-Identifier: MPL-2.0

use crate::chat::{AppSettings, ChatMessage, ChatRole, ChatSession, ProviderKind};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedState {
    pub chats: Vec<ChatSession>,
    pub active_chat_id: u64,
    pub next_chat_id: u64,
    pub next_message_id: u64,
    pub settings: AppSettings,
}

impl Default for PersistedState {
    fn default() -> Self {
        let settings = AppSettings::default();
        let mut welcome_chat = ChatSession::new(1, ProviderKind::OpenRouter, "");
        welcome_chat.title = "Welcome".into();
        welcome_chat.messages = vec![
            ChatMessage::new(
                1,
                ChatRole::System,
                "This panel stores chats locally and will later connect to OpenRouter or LM Studio.",
            ),
            ChatMessage::new(
                2,
                ChatRole::Assistant,
                "UI shell is ready for testing: create chats, switch them, rename them, delete them, and open settings.",
            ),
        ];

        Self {
            chats: vec![welcome_chat],
            active_chat_id: 1,
            next_chat_id: 2,
            next_message_id: 3,
            settings,
        }
    }
}

pub fn state_file_path() -> PathBuf {
    let base_dir = dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .or_else(dirs::config_local_dir)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".local/state"))
        })
        .unwrap_or_else(|| PathBuf::from("."));

    base_dir.join("cosmic-ai-panel").join("state.json")
}

pub fn load_state() -> PersistedState {
    let path = state_file_path();
    let Ok(contents) = fs::read_to_string(path) else {
        return PersistedState::default();
    };

    serde_json::from_str(&contents).unwrap_or_default()
}

pub fn save_state(state: &PersistedState) -> io::Result<()> {
    let path = state_file_path();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let file = File::create(path)?;
    serde_json::to_writer(file, state).map_err(io::Error::other)
}
