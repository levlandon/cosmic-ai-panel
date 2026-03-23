// SPDX-License-Identifier: MPL-2.0

use crate::chat::{AppSettings, ChatMessage, ChatRole, ChatSession, ProviderKind};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{self, File};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};

const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct AppState {
    pub chats: Vec<ChatSession>,
    pub active_chat_id: u64,
    pub next_chat_id: u64,
    pub next_message_id: u64,
    pub settings: AppSettings,
}

impl AppState {
    fn from_parts(
        settings: AppSettings,
        chat_sessions: Vec<ChatSession>,
        active_chat_id: u64,
    ) -> Result<Self, String> {
        if chat_sessions.is_empty() {
            return Err("state contains no chat sessions".into());
        }

        let next_chat_id = chat_sessions
            .iter()
            .map(|chat| chat.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let next_message_id = chat_sessions
            .iter()
            .flat_map(|chat| chat.messages.iter().map(|message| message.id))
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let active_chat_id = if chat_sessions.iter().any(|chat| chat.id == active_chat_id) {
            active_chat_id
        } else {
            chat_sessions[0].id
        };

        Ok(Self {
            chats: chat_sessions,
            active_chat_id,
            next_chat_id,
            next_message_id,
            settings,
        })
    }
}

impl Default for AppState {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedStateV1 {
    pub schema_version: u32,
    pub settings: AppSettings,
    pub chat_sessions: Vec<ChatSession>,
    pub active_chat_id: u64,
}

impl PersistedStateV1 {
    fn from_runtime(state: &AppState) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            settings: state.settings.clone(),
            chat_sessions: state.chats.clone(),
            active_chat_id: state.active_chat_id,
        }
    }

    fn into_runtime(self) -> Result<AppState, String> {
        AppState::from_parts(self.settings, self.chat_sessions, self.active_chat_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PersistedState {
    PersistedStateV1(PersistedStateV1),
}

#[derive(Debug, Deserialize)]
struct PersistedStateHeader {
    schema_version: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LegacyPersistedState {
    chats: Vec<ChatSession>,
    active_chat_id: u64,
    #[serde(default)]
    next_chat_id: Option<u64>,
    #[serde(default)]
    next_message_id: Option<u64>,
    settings: AppSettings,
}

#[derive(Debug)]
enum LoadStateError {
    Read(io::Error),
    ParseHeader(serde_json::Error),
    ParseV1(serde_json::Error),
    ParseLegacy(serde_json::Error),
    UnsupportedSchemaVersion(u32),
    Migration(String),
}

impl fmt::Display for LoadStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => write!(f, "read failed: {error}"),
            Self::ParseHeader(error) => write!(f, "invalid JSON header: {error}"),
            Self::ParseV1(error) => write!(f, "failed to parse schema v1 state: {error}"),
            Self::ParseLegacy(error) => write!(f, "failed to parse legacy state: {error}"),
            Self::UnsupportedSchemaVersion(version) => {
                write!(f, "unsupported schema_version {version}")
            }
            Self::Migration(error) => write!(f, "migration failed: {error}"),
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

pub fn load_state() -> AppState {
    load_state_from_path(&state_file_path())
}

pub fn save_state(state: &AppState) -> io::Result<()> {
    save_state_to_path(&state_file_path(), state)
}

fn load_state_from_path(path: &Path) -> AppState {
    if !path.exists() {
        return AppState::default();
    }

    match try_load_state_file(path) {
        Ok(state) => state,
        Err(error) => {
            log_persistence_warning(&format!(
                "Failed to load state from {}: {error}",
                path.display()
            ));

            let backup_path = backup_file_path(path);
            if !backup_path.exists() {
                return AppState::default();
            }

            match try_load_state_file(&backup_path) {
                Ok(state) => {
                    log_persistence_warning(&format!(
                        "Recovered state from backup {} after primary load failure",
                        backup_path.display()
                    ));
                    state
                }
                Err(backup_error) => {
                    log_persistence_warning(&format!(
                        "Failed to load backup state from {}: {backup_error}",
                        backup_path.display()
                    ));
                    AppState::default()
                }
            }
        }
    }
}

fn try_load_state_file(path: &Path) -> Result<AppState, LoadStateError> {
    let contents = fs::read_to_string(path).map_err(LoadStateError::Read)?;
    let header: PersistedStateHeader =
        serde_json::from_str(&contents).map_err(LoadStateError::ParseHeader)?;

    match header.schema_version {
        Some(CURRENT_SCHEMA_VERSION) => {
            let persisted: PersistedState =
                serde_json::from_str(&contents).map_err(LoadStateError::ParseV1)?;

            match persisted {
                PersistedState::PersistedStateV1(state) => {
                    state.into_runtime().map_err(LoadStateError::Migration)
                }
            }
        }
        Some(version) => Err(LoadStateError::UnsupportedSchemaVersion(version)),
        None => migrate_legacy_state(&contents),
    }
}

fn migrate_legacy_state(contents: &str) -> Result<AppState, LoadStateError> {
    let legacy: LegacyPersistedState =
        serde_json::from_str(contents).map_err(LoadStateError::ParseLegacy)?;
    let state_v1 = PersistedStateV1 {
        schema_version: CURRENT_SCHEMA_VERSION,
        settings: legacy.settings,
        chat_sessions: legacy.chats,
        active_chat_id: legacy.active_chat_id,
    };

    state_v1.into_runtime().map_err(LoadStateError::Migration)
}

fn save_state_to_path(path: &Path, state: &AppState) -> io::Result<()> {
    if let Some(parent) = path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        log_persistence_error(
            &format!("Failed to create state directory {}", parent.display()),
            &error,
        );
        return Err(error);
    }

    let tmp_path = temp_file_path(path);
    let backup_path = backup_file_path(path);
    let persisted = PersistedState::PersistedStateV1(PersistedStateV1::from_runtime(state));
    let serialized = serde_json::to_vec_pretty(&persisted).map_err(|error| {
        let io_error = io::Error::new(ErrorKind::InvalidData, error);
        log_persistence_error("Failed to serialize state", &io_error);
        io_error
    })?;

    let mut tmp_file = match File::create(&tmp_path) {
        Ok(file) => file,
        Err(error) => {
            log_persistence_error(
                &format!("Failed to create temporary state file {}", tmp_path.display()),
                &error,
            );
            return Err(error);
        }
    };

    if let Err(error) = tmp_file.write_all(&serialized) {
        log_persistence_error(
            &format!("Failed to write temporary state file {}", tmp_path.display()),
            &error,
        );
        cleanup_tmp_file(&tmp_path);
        return Err(error);
    }

    if let Err(error) = tmp_file.flush() {
        log_persistence_error(
            &format!("Failed to flush temporary state file {}", tmp_path.display()),
            &error,
        );
        cleanup_tmp_file(&tmp_path);
        return Err(error);
    }

    if let Err(error) = tmp_file.sync_all() {
        log_persistence_error(
            &format!("Failed to fsync temporary state file {}", tmp_path.display()),
            &error,
        );
        cleanup_tmp_file(&tmp_path);
        return Err(error);
    }

    drop(tmp_file);

    if path.exists()
        && let Err(error) = fs::copy(path, &backup_path)
    {
        log_persistence_error(
            &format!("Failed to create state backup {}", backup_path.display()),
            &error,
        );
        cleanup_tmp_file(&tmp_path);
        return Err(error);
    }

    if let Err(error) = fs::rename(&tmp_path, path) {
        log_persistence_error(
            &format!(
                "Failed to atomically rename {} to {}",
                tmp_path.display(),
                path.display()
            ),
            &error,
        );
        cleanup_tmp_file(&tmp_path);
        return Err(error);
    }

    Ok(())
}

fn temp_file_path(path: &Path) -> PathBuf {
    sibling_path_with_suffix(path, ".tmp")
}

fn backup_file_path(path: &Path) -> PathBuf {
    sibling_path_with_suffix(path, ".bak")
}

fn sibling_path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| format!("{name}{suffix}"))
        .unwrap_or_else(|| format!("state{suffix}"));

    path.with_file_name(file_name)
}

fn cleanup_tmp_file(path: &Path) {
    if let Err(error) = fs::remove_file(path)
        && error.kind() != ErrorKind::NotFound
    {
        log_persistence_warning(&format!(
            "Failed to remove temporary state file {}: {error}",
            path.display()
        ));
    }
}

fn log_persistence_warning(message: &str) {
    eprintln!("[cosmic-ai-panel][storage][warn] {message}");
}

fn log_persistence_error(message: &str, error: &io::Error) {
    eprintln!("[cosmic-ai-panel][storage][error] {message}: {error}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn legacy_state_migrates_into_v1_runtime_shape() {
        let path = unique_test_path("legacy-migration");
        let legacy = LegacyPersistedState {
            chats: vec![ChatSession::new(7, ProviderKind::LmStudio, "local-model")],
            active_chat_id: 7,
            next_chat_id: Some(99),
            next_message_id: Some(100),
            settings: AppSettings::default(),
        };

        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();

        let loaded = load_state_from_path(&path);

        assert_eq!(loaded.active_chat_id, 7);
        assert_eq!(loaded.chats.len(), 1);
        assert_eq!(loaded.next_chat_id, 8);
        assert_eq!(loaded.next_message_id, 1);

        cleanup_test_dir(&path);
    }

    #[test]
    fn invalid_primary_file_falls_back_to_backup() {
        let path = unique_test_path("backup-fallback");
        let backup_path = backup_file_path(&path);
        let backup_state = PersistedState::PersistedStateV1(PersistedStateV1::from_runtime(
            &AppState::default(),
        ));

        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"{ not valid json").unwrap();
        fs::write(&backup_path, serde_json::to_vec(&backup_state).unwrap()).unwrap();

        let loaded = load_state_from_path(&path);

        assert_eq!(loaded.active_chat_id, 1);
        assert_eq!(loaded.chats[0].title, "Welcome");

        cleanup_test_dir(&path);
    }

    #[test]
    fn save_state_writes_versioned_state_and_backup() {
        let path = unique_test_path("versioned-save");
        let mut state = AppState::default();
        state.chats[0].title = "Updated title".into();

        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, br#"{"legacy":"value"}"#).unwrap();

        save_state_to_path(&path, &state).unwrap();

        let saved: PersistedState = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let backup_contents = fs::read_to_string(backup_file_path(&path)).unwrap();

        match saved {
            PersistedState::PersistedStateV1(v1) => {
                assert_eq!(v1.schema_version, CURRENT_SCHEMA_VERSION);
                assert_eq!(v1.chat_sessions[0].title, "Updated title");
            }
        }
        assert_eq!(backup_contents, r#"{"legacy":"value"}"#);

        cleanup_test_dir(&path);
    }

    fn unique_test_path(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        std::env::temp_dir()
            .join("cosmic-ai-panel-tests")
            .join(format!("{label}-{unique}"))
            .join("state.json")
    }

    fn cleanup_test_dir(path: &Path) {
        if let Some(root) = path.parent() {
            let _ = fs::remove_dir_all(root);
        }
    }
}
