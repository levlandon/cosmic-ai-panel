// SPDX-License-Identifier: MPL-2.0

use crate::chat::{
    AppSettings, ChatMessage, ChatRole, ChatSession, ModelFilterSettings, ProviderKind,
    ProviderSettings, SavedModel, SkillsSettings, UserProfile, default_base_system_prompt,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};

const CURRENT_CHAT_SCHEMA_VERSION: u32 = 1;
const CURRENT_SETTINGS_SCHEMA_VERSION: u32 = 1;

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
pub struct PersistedChatStateV1 {
    pub schema_version: u32,
    pub chat_sessions: Vec<ChatSession>,
    pub active_chat_id: u64,
}

impl PersistedChatStateV1 {
    fn from_runtime(state: &AppState) -> Self {
        Self {
            schema_version: CURRENT_CHAT_SCHEMA_VERSION,
            chat_sessions: state.chats.clone(),
            active_chat_id: state.active_chat_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PersistedChatState {
    PersistedChatStateV1(PersistedChatStateV1),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedSettingsV1 {
    pub schema_version: u32,
    pub provider: ProviderSettings,
    pub profile: UserProfile,
    pub memory: Vec<String>,
    pub skills: SkillsSettings,
    pub base_system_prompt: String,
    pub context_message_limit: usize,
}

impl PersistedSettingsV1 {
    fn from_runtime(settings: &AppSettings) -> Self {
        Self {
            schema_version: CURRENT_SETTINGS_SCHEMA_VERSION,
            provider: settings.provider.clone(),
            profile: settings.profile.clone(),
            memory: settings.memory.clone(),
            skills: settings.skills.clone(),
            base_system_prompt: settings.base_system_prompt.clone(),
            context_message_limit: settings.context_message_limit,
        }
    }

    fn into_runtime(self) -> AppSettings {
        let mut settings = AppSettings {
            provider: self.provider,
            profile: self.profile,
            memory: self.memory,
            skills: self.skills,
            base_system_prompt: self.base_system_prompt,
            context_message_limit: self.context_message_limit,
        };
        settings.normalize();
        settings
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PersistedSettings {
    PersistedSettingsV1(PersistedSettingsV1),
}

#[derive(Debug, Deserialize)]
struct PersistedFileHeader {
    schema_version: Option<u32>,
}

#[derive(Debug)]
enum LoadFileError {
    Read(io::Error),
    ParseHeader(serde_json::Error),
    ParseV1(serde_json::Error),
    ParseLegacy(serde_json::Error),
    UnsupportedSchemaVersion(u32),
    Migration(String),
}

impl fmt::Display for LoadFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => write!(f, "read failed: {error}"),
            Self::ParseHeader(error) => write!(f, "invalid JSON header: {error}"),
            Self::ParseV1(error) => write!(f, "failed to parse schema v1 data: {error}"),
            Self::ParseLegacy(error) => write!(f, "failed to parse legacy data: {error}"),
            Self::UnsupportedSchemaVersion(version) => {
                write!(f, "unsupported schema_version {version}")
            }
            Self::Migration(error) => write!(f, "migration failed: {error}"),
        }
    }
}

#[derive(Debug)]
struct LoadedChatState {
    chat_sessions: Vec<ChatSession>,
    active_chat_id: u64,
    legacy_settings: Option<AppSettings>,
}

impl LoadedChatState {
    fn into_runtime(self, settings: AppSettings) -> Result<AppState, String> {
        AppState::from_parts(settings, self.chat_sessions, self.active_chat_id)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct LegacyPersistedState {
    chats: Vec<ChatSession>,
    active_chat_id: u64,
    #[serde(default)]
    next_chat_id: Option<u64>,
    #[serde(default)]
    next_message_id: Option<u64>,
    settings: LegacyAppSettings,
}

#[derive(Debug, Serialize, Deserialize)]
struct LegacyVersionedStateV1 {
    schema_version: u32,
    settings: LegacyAppSettings,
    chat_sessions: Vec<ChatSession>,
    active_chat_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct LegacyAppSettings {
    pub provider: ProviderKind,
    #[serde(default, skip_serializing)]
    pub openrouter_api_key: String,
    pub lmstudio_base_url: String,
    pub saved_models: Vec<SavedModel>,
    pub default_model: Option<SavedModel>,
    #[serde(default, alias = "system_prompt")]
    pub base_system_prompt: String,
    pub context_message_limit: usize,
    pub profile: UserProfile,
    pub memory: Vec<String>,
    pub skills: SkillsSettings,
    #[serde(default, alias = "openrouter_model")]
    legacy_openrouter_model: String,
    #[serde(default, alias = "lmstudio_model")]
    legacy_lmstudio_model: String,
    #[serde(default)]
    timeout_seconds: Option<u64>,
    #[serde(default)]
    retry_attempts: Option<u8>,
    #[serde(default)]
    retry_delay_seconds: Option<u64>,
}

impl LegacyAppSettings {
    fn into_runtime(self) -> AppSettings {
        let mut settings = AppSettings {
            provider: ProviderSettings {
                active_provider: self.provider,
                openrouter_api_key: self.openrouter_api_key,
                lmstudio_base_url: self.lmstudio_base_url,
                saved_models: self.saved_models,
                default_model: self.default_model,
                model_filter: ModelFilterSettings::default(),
                follow_provider_selection: false,
                timeout_seconds: self.timeout_seconds.unwrap_or(20),
                retry_attempts: self.retry_attempts.unwrap_or(1),
                retry_delay_seconds: self.retry_delay_seconds.unwrap_or(2),
                legacy_openrouter_model: self.legacy_openrouter_model,
                legacy_lmstudio_model: self.legacy_lmstudio_model,
            },
            profile: self.profile,
            memory: self.memory,
            skills: self.skills,
            base_system_prompt: if self.base_system_prompt.trim().is_empty() {
                default_base_system_prompt().to_string()
            } else {
                self.base_system_prompt
            },
            context_message_limit: self.context_message_limit,
        };
        settings.normalize();
        settings
    }
}

impl Default for LegacyAppSettings {
    fn default() -> Self {
        Self {
            provider: ProviderKind::OpenRouter,
            openrouter_api_key: String::new(),
            lmstudio_base_url: "http://127.0.0.1:1234/v1".into(),
            saved_models: Vec::new(),
            default_model: None,
            base_system_prompt: default_base_system_prompt().into(),
            context_message_limit: 10,
            profile: UserProfile::default(),
            memory: Vec::new(),
            skills: SkillsSettings::default(),
            legacy_openrouter_model: String::new(),
            legacy_lmstudio_model: String::new(),
            timeout_seconds: None,
            retry_attempts: None,
            retry_delay_seconds: None,
        }
    }
}

pub fn state_file_path() -> PathBuf {
    storage_root_dir().join("state.json")
}

pub fn settings_file_path() -> PathBuf {
    storage_root_dir().join("settings.json")
}

pub fn load_state() -> AppState {
    load_state_from_paths(&state_file_path(), &settings_file_path())
}

pub fn save_state(state: &AppState) -> io::Result<()> {
    save_state_to_paths(&state_file_path(), &settings_file_path(), state)
}

fn storage_root_dir() -> PathBuf {
    let base_dir = dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .or_else(dirs::config_local_dir)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".local/state"))
        })
        .unwrap_or_else(|| PathBuf::from("."));

    base_dir.join("cosmic-ai-panel")
}

fn load_state_from_paths(state_path: &Path, settings_path: &Path) -> AppState {
    let chats = load_chat_state_from_path(state_path);
    let settings = load_settings_from_path(settings_path, chats.legacy_settings.clone());

    match chats.into_runtime(settings) {
        Ok(state) => state,
        Err(error) => {
            log_persistence_warning(&format!(
                "Failed to rebuild runtime state from persisted files: {error}"
            ));
            AppState {
                settings: load_settings_from_path(settings_path, None),
                ..AppState::default()
            }
        }
    }
}

fn load_chat_state_from_path(path: &Path) -> LoadedChatState {
    if !path.exists() {
        return default_loaded_chat_state();
    }

    match try_load_chat_state_file(path) {
        Ok(state) => state,
        Err(error) => {
            log_persistence_warning(&format!(
                "Failed to load chat state from {}: {error}",
                path.display()
            ));

            let backup_path = backup_file_path(path);
            if !backup_path.exists() {
                return default_loaded_chat_state();
            }

            match try_load_chat_state_file(&backup_path) {
                Ok(state) => {
                    log_persistence_warning(&format!(
                        "Recovered chat state from backup {} after primary load failure",
                        backup_path.display()
                    ));
                    state
                }
                Err(backup_error) => {
                    log_persistence_warning(&format!(
                        "Failed to load chat backup from {}: {backup_error}",
                        backup_path.display()
                    ));
                    default_loaded_chat_state()
                }
            }
        }
    }
}

fn load_settings_from_path(path: &Path, fallback_settings: Option<AppSettings>) -> AppSettings {
    if !path.exists() {
        return fallback_settings.unwrap_or_default();
    }

    match try_load_settings_file(path) {
        Ok(settings) => settings,
        Err(error) => {
            log_persistence_warning(&format!(
                "Failed to load settings from {}: {error}",
                path.display()
            ));

            let backup_path = backup_file_path(path);
            if backup_path.exists() {
                match try_load_settings_file(&backup_path) {
                    Ok(settings) => {
                        log_persistence_warning(&format!(
                            "Recovered settings from backup {} after primary load failure",
                            backup_path.display()
                        ));
                        return settings;
                    }
                    Err(backup_error) => {
                        log_persistence_warning(&format!(
                            "Failed to load settings backup from {}: {backup_error}",
                            backup_path.display()
                        ));
                    }
                }
            }

            fallback_settings.unwrap_or_default()
        }
    }
}

fn try_load_chat_state_file(path: &Path) -> Result<LoadedChatState, LoadFileError> {
    let contents = fs::read_to_string(path).map_err(LoadFileError::Read)?;
    let header: PersistedFileHeader =
        serde_json::from_str(&contents).map_err(LoadFileError::ParseHeader)?;

    match header.schema_version {
        Some(CURRENT_CHAT_SCHEMA_VERSION) => parse_versioned_chat_state(&contents),
        Some(version) => Err(LoadFileError::UnsupportedSchemaVersion(version)),
        None => migrate_legacy_chat_state(&contents),
    }
}

fn try_load_settings_file(path: &Path) -> Result<AppSettings, LoadFileError> {
    let contents = fs::read_to_string(path).map_err(LoadFileError::Read)?;
    let header: PersistedFileHeader =
        serde_json::from_str(&contents).map_err(LoadFileError::ParseHeader)?;

    match header.schema_version {
        Some(CURRENT_SETTINGS_SCHEMA_VERSION) => {
            let persisted: PersistedSettings =
                serde_json::from_str(&contents).map_err(LoadFileError::ParseV1)?;

            match persisted {
                PersistedSettings::PersistedSettingsV1(settings) => Ok(settings.into_runtime()),
            }
        }
        Some(version) => Err(LoadFileError::UnsupportedSchemaVersion(version)),
        None => {
            let legacy: LegacyAppSettings =
                serde_json::from_str(&contents).map_err(LoadFileError::ParseLegacy)?;
            Ok(legacy.into_runtime())
        }
    }
}

fn parse_versioned_chat_state(contents: &str) -> Result<LoadedChatState, LoadFileError> {
    let value: Value = serde_json::from_str(contents).map_err(LoadFileError::ParseV1)?;

    if value.get("settings").is_some() {
        let legacy: LegacyVersionedStateV1 =
            serde_json::from_value(value).map_err(LoadFileError::ParseV1)?;
        return Ok(LoadedChatState {
            chat_sessions: legacy.chat_sessions,
            active_chat_id: legacy.active_chat_id,
            legacy_settings: Some(legacy.settings.into_runtime()),
        });
    }

    let persisted: PersistedChatState =
        serde_json::from_value(value).map_err(LoadFileError::ParseV1)?;

    match persisted {
        PersistedChatState::PersistedChatStateV1(state) => Ok(LoadedChatState {
            chat_sessions: state.chat_sessions,
            active_chat_id: state.active_chat_id,
            legacy_settings: None,
        }),
    }
}

fn migrate_legacy_chat_state(contents: &str) -> Result<LoadedChatState, LoadFileError> {
    let legacy: LegacyPersistedState =
        serde_json::from_str(contents).map_err(LoadFileError::ParseLegacy)?;

    if legacy.chats.is_empty() {
        return Err(LoadFileError::Migration(
            "legacy state contains no chat sessions".into(),
        ));
    }

    Ok(LoadedChatState {
        chat_sessions: legacy.chats,
        active_chat_id: legacy.active_chat_id,
        legacy_settings: Some(legacy.settings.into_runtime()),
    })
}

fn save_state_to_paths(
    state_path: &Path,
    settings_path: &Path,
    state: &AppState,
) -> io::Result<()> {
    save_chat_state_to_path(state_path, state)?;
    save_settings_to_path(settings_path, &state.settings)
}

fn save_chat_state_to_path(path: &Path, state: &AppState) -> io::Result<()> {
    let persisted =
        PersistedChatState::PersistedChatStateV1(PersistedChatStateV1::from_runtime(state));
    let serialized = serde_json::to_vec_pretty(&persisted).map_err(|error| {
        let io_error = io::Error::new(ErrorKind::InvalidData, error);
        log_persistence_error("Failed to serialize chat state", &io_error);
        io_error
    })?;

    save_json_atomic(path, &serialized, "chat state")
}

fn save_settings_to_path(path: &Path, settings: &AppSettings) -> io::Result<()> {
    let persisted =
        PersistedSettings::PersistedSettingsV1(PersistedSettingsV1::from_runtime(settings));
    let serialized = serde_json::to_vec_pretty(&persisted).map_err(|error| {
        let io_error = io::Error::new(ErrorKind::InvalidData, error);
        log_persistence_error("Failed to serialize settings", &io_error);
        io_error
    })?;

    save_json_atomic(path, &serialized, "settings")
}

fn save_json_atomic(path: &Path, serialized: &[u8], label: &str) -> io::Result<()> {
    if let Some(parent) = path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        log_persistence_error(
            &format!(
                "Failed to create directory for {label}: {}",
                parent.display()
            ),
            &error,
        );
        return Err(error);
    }

    let tmp_path = temp_file_path(path);
    let backup_path = backup_file_path(path);
    let mut tmp_file = match File::create(&tmp_path) {
        Ok(file) => file,
        Err(error) => {
            log_persistence_error(
                &format!(
                    "Failed to create temporary {label} file {}",
                    tmp_path.display()
                ),
                &error,
            );
            return Err(error);
        }
    };

    if let Err(error) = tmp_file.write_all(serialized) {
        log_persistence_error(
            &format!(
                "Failed to write temporary {label} file {}",
                tmp_path.display()
            ),
            &error,
        );
        cleanup_tmp_file(&tmp_path);
        return Err(error);
    }

    if let Err(error) = tmp_file.flush() {
        log_persistence_error(
            &format!(
                "Failed to flush temporary {label} file {}",
                tmp_path.display()
            ),
            &error,
        );
        cleanup_tmp_file(&tmp_path);
        return Err(error);
    }

    if let Err(error) = tmp_file.sync_all() {
        log_persistence_error(
            &format!(
                "Failed to fsync temporary {label} file {}",
                tmp_path.display()
            ),
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
            &format!("Failed to create {label} backup {}", backup_path.display()),
            &error,
        );
        cleanup_tmp_file(&tmp_path);
        return Err(error);
    }

    if let Err(error) = fs::rename(&tmp_path, path) {
        log_persistence_error(
            &format!(
                "Failed to atomically rename {} to {} for {label}",
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

fn default_loaded_chat_state() -> LoadedChatState {
    let state = AppState::default();
    LoadedChatState {
        chat_sessions: state.chats,
        active_chat_id: state.active_chat_id,
        legacy_settings: Some(state.settings),
    }
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
            "Failed to remove temporary file {}: {error}",
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
    fn legacy_state_migrates_into_split_runtime_shape() {
        let root = unique_test_root("legacy-migration");
        let state_path = root.join("state.json");
        let settings_path = root.join("settings.json");
        let legacy = LegacyPersistedState {
            chats: vec![ChatSession::new(7, ProviderKind::LmStudio, "local-model")],
            active_chat_id: 7,
            next_chat_id: Some(99),
            next_message_id: Some(100),
            settings: LegacyAppSettings {
                provider: ProviderKind::LmStudio,
                base_system_prompt: "Legacy prompt".into(),
                memory: vec!["User works with Rust".into()],
                ..LegacyAppSettings::default()
            },
        };

        fs::create_dir_all(state_path.parent().unwrap()).unwrap();
        fs::write(&state_path, serde_json::to_vec(&legacy).unwrap()).unwrap();

        let loaded = load_state_from_paths(&state_path, &settings_path);

        assert_eq!(loaded.active_chat_id, 7);
        assert_eq!(loaded.chats.len(), 1);
        assert_eq!(loaded.next_chat_id, 8);
        assert_eq!(loaded.next_message_id, 1);
        assert_eq!(
            loaded.settings.provider.active_provider,
            ProviderKind::LmStudio
        );
        assert_eq!(loaded.settings.base_system_prompt, "Legacy prompt");
        assert_eq!(loaded.settings.memory, vec!["User works with Rust"]);

        cleanup_test_dir(&state_path);
    }

    #[test]
    fn invalid_settings_file_falls_back_to_backup() {
        let root = unique_test_root("settings-backup");
        let state_path = root.join("state.json");
        let settings_path = root.join("settings.json");
        let backup_path = backup_file_path(&settings_path);
        let backup = PersistedSettings::PersistedSettingsV1(PersistedSettingsV1 {
            schema_version: CURRENT_SETTINGS_SCHEMA_VERSION,
            provider: ProviderSettings::default(),
            profile: UserProfile {
                name: Some("Lev".into()),
                language: None,
                response_style: None,
                ..UserProfile::default()
            },
            memory: vec!["User uses COSMIC desktop".into()],
            skills: SkillsSettings::default(),
            base_system_prompt: default_base_system_prompt().into(),
            context_message_limit: 10,
        });

        fs::create_dir_all(state_path.parent().unwrap()).unwrap();
        save_chat_state_to_path(&state_path, &AppState::default()).unwrap();
        fs::write(&settings_path, b"{ not valid json").unwrap();
        fs::write(&backup_path, serde_json::to_vec(&backup).unwrap()).unwrap();

        let loaded = load_state_from_paths(&state_path, &settings_path);

        assert_eq!(loaded.settings.profile.name.as_deref(), Some("Lev"));
        assert_eq!(loaded.settings.memory, vec!["User uses COSMIC desktop"]);

        cleanup_test_dir(&state_path);
    }

    #[test]
    fn save_state_writes_split_state_and_settings_files() {
        let root = unique_test_root("split-save");
        let state_path = root.join("state.json");
        let settings_path = root.join("settings.json");
        let mut state = AppState::default();
        state.chats[0].title = "Updated title".into();
        state.settings.memory = vec!["User works with Rust".into()];

        fs::create_dir_all(state_path.parent().unwrap()).unwrap();
        fs::write(&state_path, br#"{"legacy":"value"}"#).unwrap();
        fs::write(&settings_path, br#"{"legacy":"settings"}"#).unwrap();

        save_state_to_paths(&state_path, &settings_path, &state).unwrap();

        let saved_state: PersistedChatState =
            serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
        let saved_settings: PersistedSettings =
            serde_json::from_slice(&fs::read(&settings_path).unwrap()).unwrap();
        let state_backup = fs::read_to_string(backup_file_path(&state_path)).unwrap();
        let settings_backup = fs::read_to_string(backup_file_path(&settings_path)).unwrap();

        match saved_state {
            PersistedChatState::PersistedChatStateV1(v1) => {
                assert_eq!(v1.schema_version, CURRENT_CHAT_SCHEMA_VERSION);
                assert_eq!(v1.chat_sessions[0].title, "Updated title");
            }
        }

        match saved_settings {
            PersistedSettings::PersistedSettingsV1(v1) => {
                assert_eq!(v1.schema_version, CURRENT_SETTINGS_SCHEMA_VERSION);
                assert_eq!(v1.memory, vec!["User works with Rust"]);
            }
        }

        assert_eq!(state_backup, r#"{"legacy":"value"}"#);
        assert_eq!(settings_backup, r#"{"legacy":"settings"}"#);

        cleanup_test_dir(&state_path);
    }

    fn unique_test_root(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        std::env::temp_dir()
            .join("cosmic-ai-panel-tests")
            .join(format!("{label}-{unique}"))
    }

    fn cleanup_test_dir(path: &Path) {
        if let Some(root) = path.parent() {
            let _ = fs::remove_dir_all(root);
        }
    }
}
