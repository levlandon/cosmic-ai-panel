// SPDX-License-Identifier: MPL-2.0
//! Versioned import and export helpers for personalization-only settings.

use crate::chat::{UserProfile, default_base_system_prompt};
use serde::{Deserialize, Serialize};

const CURRENT_PERSONALIZATION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PersonalizationSettings {
    pub base_system_prompt: String,
    pub profile: UserProfile,
    pub memory: Vec<String>,
}

impl PersonalizationSettings {
    pub fn normalize(&mut self) {
        self.profile.normalize();
        self.memory = self
            .memory
            .drain(..)
            .filter_map(|item| {
                let trimmed = item.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .collect();

        let prompt = self.base_system_prompt.trim();
        self.base_system_prompt = if prompt.is_empty() {
            default_base_system_prompt().to_string()
        } else {
            prompt.to_string()
        };
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalizationBundleV1 {
    pub schema_version: u32,
    pub base_system_prompt: String,
    pub profile: UserProfile,
    pub memory: Vec<String>,
}

impl PersonalizationBundleV1 {
    fn from_settings(settings: &PersonalizationSettings) -> Self {
        Self {
            schema_version: CURRENT_PERSONALIZATION_SCHEMA_VERSION,
            base_system_prompt: settings.base_system_prompt.clone(),
            profile: settings.profile.clone(),
            memory: settings.memory.clone(),
        }
    }

    fn into_settings(self) -> PersonalizationSettings {
        let mut settings = PersonalizationSettings {
            base_system_prompt: self.base_system_prompt,
            profile: self.profile,
            memory: self.memory,
        };
        settings.normalize();
        settings
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PersonalizationBundle {
    V1(PersonalizationBundleV1),
}

#[derive(Debug, Deserialize)]
struct PersonalizationHeader {
    schema_version: Option<u32>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct LegacyPersonalizationBundle {
    base_system_prompt: String,
    profile: UserProfile,
    memory: Vec<String>,
}

impl LegacyPersonalizationBundle {
    fn into_settings(self) -> PersonalizationSettings {
        let mut settings = PersonalizationSettings {
            base_system_prompt: self.base_system_prompt,
            profile: self.profile,
            memory: self.memory,
        };
        settings.normalize();
        settings
    }
}

pub fn export_personalization(settings: &PersonalizationSettings) -> Result<String, String> {
    serde_json::to_string_pretty(&PersonalizationBundle::V1(
        PersonalizationBundleV1::from_settings(settings),
    ))
    .map_err(|error| format!("Failed to serialize personalization export: {error}"))
}

pub fn import_personalization(serialized: &str) -> Result<PersonalizationSettings, String> {
    let header: PersonalizationHeader = serde_json::from_str(serialized)
        .map_err(|error| format!("Invalid personalization JSON: {error}"))?;

    match header.schema_version {
        Some(CURRENT_PERSONALIZATION_SCHEMA_VERSION) => {
            let bundle: PersonalizationBundle = serde_json::from_str(serialized)
                .map_err(|error| format!("Failed to parse personalization schema v1: {error}"))?;
            match bundle {
                PersonalizationBundle::V1(bundle) => Ok(bundle.into_settings()),
            }
        }
        Some(version) => Err(format!(
            "Unsupported personalization schema_version {version}"
        )),
        None => {
            let legacy: LegacyPersonalizationBundle = serde_json::from_str(serialized)
                .map_err(|error| format!("Failed to parse legacy personalization data: {error}"))?;
            Ok(legacy.into_settings())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::PreferenceLevel;

    #[test]
    fn export_roundtrip_preserves_personalization() {
        let mut settings = PersonalizationSettings {
            base_system_prompt: "Stay precise".into(),
            profile: UserProfile {
                name: Some("Lev".into()),
                language: Some("English".into()),
                occupation: Some("Rust desktop engineer".into()),
                response_style: Some("Short paragraphs".into()),
                more_about_you: Some("Focus on COSMIC and Rust.".into()),
                header_lists: PreferenceLevel::More,
                emoji: PreferenceLevel::Less,
            },
            memory: vec!["User uses COSMIC desktop".into()],
        };
        settings.normalize();

        let exported = export_personalization(&settings).unwrap();
        let imported = import_personalization(&exported).unwrap();

        assert_eq!(imported, settings);
    }

    #[test]
    fn import_legacy_personalization_without_version() {
        let imported = import_personalization(
            r#"{
                "base_system_prompt": "",
                "profile": { "name": "Lev" },
                "memory": ["Rust", "   "]
            }"#,
        )
        .unwrap();

        assert_eq!(imported.base_system_prompt, default_base_system_prompt());
        assert_eq!(imported.profile.name.as_deref(), Some("Lev"));
        assert_eq!(imported.memory, vec!["Rust"]);
    }
}
