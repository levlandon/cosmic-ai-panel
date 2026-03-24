//! Build the final provider context from prompt layers and chat history.

use crate::chat::{ChatMessage, ChatRole, PreferenceLevel, UserProfile};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ContextMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct BuiltContext {
    pub system_prompt: Option<String>,
    pub history: Vec<ContextMessage>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct PromptPreview {
    pub code: String,
    pub text: String,
}

pub fn build_context(
    base_prompt: &str,
    profile: &UserProfile,
    memory: &[String],
    history: &[ChatMessage],
) -> BuiltContext {
    let system_prompt = build_system_prompt(base_prompt, profile, memory);
    let history = history
        .iter()
        .filter_map(|message| {
            let content = message.content.trim();
            if content.is_empty() {
                None
            } else {
                Some(ContextMessage {
                    role: message.role,
                    content: message.content.clone(),
                })
            }
        })
        .collect();

    BuiltContext {
        system_prompt,
        history,
    }
}

pub fn build_prompt_preview(
    base_prompt: &str,
    profile: &UserProfile,
    memory: &[String],
) -> PromptPreview {
    let text = build_system_prompt(base_prompt, profile, memory)
        .unwrap_or_else(|| "(The system prompt is empty.)".into());
    let code = build_code_preview(base_prompt, profile, memory);

    PromptPreview { code, text }
}

fn build_system_prompt(
    base_prompt: &str,
    profile: &UserProfile,
    memory: &[String],
) -> Option<String> {
    let mut sections = Vec::new();
    let base_prompt = base_prompt.trim();
    if !base_prompt.is_empty() {
        sections.push(base_prompt.to_string());
    }

    if let Some(profile_prompt) = build_profile_prompt(profile) {
        sections.push(profile_prompt);
    }

    if let Some(memory_prompt) = build_memory_prompt(memory) {
        sections.push(memory_prompt);
    }

    if sections.is_empty() {
        None
    } else {
        Some(sections.join("\n\n"))
    }
}

fn build_profile_prompt(profile: &UserProfile) -> Option<String> {
    if profile.is_empty() {
        return None;
    }

    let mut lines = Vec::new();
    if let Some(name) = &profile.name {
        lines.push(format!("- Name: {name}"));
    }
    if let Some(language) = &profile.language {
        lines.push(format!("- Preferred language: {language}"));
    }
    if let Some(occupation) = &profile.occupation {
        push_multiline_profile_entry(&mut lines, "Occupation", occupation);
    }
    if let Some(style) = &profile.response_style {
        push_multiline_profile_entry(&mut lines, "Response style", style);
    }
    if let Some(details) = &profile.more_about_you {
        push_multiline_profile_entry(&mut lines, "More about you", details);
    }
    if let Some(header_lists) = header_lists_instruction(profile.header_lists) {
        lines.push(format!("- Header & lists: {header_lists}"));
    }
    if let Some(emoji) = emoji_instruction(profile.emoji) {
        lines.push(format!("- Emoji: {emoji}"));
    }

    Some(format!("User profile:\n{}", lines.join("\n")))
}

fn push_multiline_profile_entry(lines: &mut Vec<String>, label: &str, value: &str) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }

    if trimmed.contains('\n') {
        let body = trimmed
            .lines()
            .map(str::trim_end)
            .filter(|line| !line.is_empty())
            .map(|line| format!("  {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        lines.push(format!("- {label}:\n{body}"));
    } else {
        lines.push(format!("- {label}: {trimmed}"));
    }
}

fn header_lists_instruction(preference: PreferenceLevel) -> Option<&'static str> {
    match preference {
        PreferenceLevel::More => {
            Some("Use more headings and bullet lists when that improves clarity.")
        }
        PreferenceLevel::Default => None,
        PreferenceLevel::Less => Some(
            "Use fewer headings and bullet lists; prefer plain paragraphs unless structure is needed.",
        ),
    }
}

fn emoji_instruction(preference: PreferenceLevel) -> Option<&'static str> {
    match preference {
        PreferenceLevel::More => Some("Using emoji is welcome when it adds tone or clarity."),
        PreferenceLevel::Default => None,
        PreferenceLevel::Less => Some("Avoid emoji unless the user explicitly asks for them."),
    }
}

fn build_memory_prompt(memory: &[String]) -> Option<String> {
    let items: Vec<String> = memory
        .iter()
        .filter_map(|item| {
            let trimmed = item.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(format!("- {trimmed}"))
            }
        })
        .collect();

    if items.is_empty() {
        None
    } else {
        Some(format!("Known user memory:\n{}", items.join("\n")))
    }
}

fn build_code_preview(base_prompt: &str, profile: &UserProfile, memory: &[String]) -> String {
    let mut profile_map = Map::new();
    profile_map.insert("name".into(), optional_json_value(&profile.name));
    profile_map.insert(
        "preferred_language".into(),
        optional_json_value(&profile.language),
    );
    profile_map.insert(
        "occupation".into(),
        optional_json_value(&profile.occupation),
    );
    profile_map.insert(
        "response_style".into(),
        optional_json_value(&profile.response_style),
    );
    profile_map.insert(
        "more_about_you".into(),
        optional_json_value(&profile.more_about_you),
    );
    profile_map.insert(
        "header_lists".into(),
        Value::String(profile.header_lists.label().to_ascii_lowercase()),
    );
    profile_map.insert(
        "emoji".into(),
        Value::String(profile.emoji.label().to_ascii_lowercase()),
    );

    let profile_json =
        serde_json::to_string_pretty(&Value::Object(profile_map)).unwrap_or_else(|_| "{}".into());
    let memory_json = serde_json::to_string_pretty(memory).unwrap_or_else(|_| "[]".into());

    format!(
        "# Variables\nbase_system_prompt = {base_prompt:?}\n\nprofile = {profile_json}\n\nmemory = {memory_json}\n\n# Builder\nfinal_system_prompt = join_non_empty([\n  base_system_prompt,\n  build_profile_prompt(profile),\n  build_memory_prompt(memory),\n], \"\\n\\n\")\n\n# Request-time context\nchat_history = context_tail(history)\n"
    )
}

fn optional_json_value(value: &Option<String>) -> Value {
    value
        .as_ref()
        .map(|value| Value::String(value.clone()))
        .unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::{build_context, build_prompt_preview};
    use crate::chat::{ChatMessage, ChatRole, PreferenceLevel, UserProfile};

    #[test]
    fn build_context_assembles_system_layers() {
        let profile = UserProfile {
            name: Some("Lev".into()),
            language: Some("English".into()),
            occupation: Some("Rust desktop engineer".into()),
            response_style: Some("Concise".into()),
            more_about_you: Some("Builds for COSMIC".into()),
            header_lists: PreferenceLevel::More,
            emoji: PreferenceLevel::Less,
        };
        let memory = vec![
            "User uses COSMIC desktop".to_string(),
            "User works with Rust".to_string(),
        ];
        let history = vec![ChatMessage::new(1, ChatRole::User, "hello")];

        let built = build_context("Base prompt", &profile, &memory, &history);

        assert_eq!(built.history.len(), 1);
        assert_eq!(built.history[0].content, "hello");
        assert_eq!(
            built.system_prompt,
            Some(
                "Base prompt\n\nUser profile:\n- Name: Lev\n- Preferred language: English\n- Occupation: Rust desktop engineer\n- Response style: Concise\n- More about you: Builds for COSMIC\n- Header & lists: Use more headings and bullet lists when that improves clarity.\n- Emoji: Avoid emoji unless the user explicitly asks for them.\n\nKnown user memory:\n- User uses COSMIC desktop\n- User works with Rust"
                    .to_string()
            )
        );
    }

    #[test]
    fn build_context_skips_empty_history_and_sections() {
        let built = build_context(
            "",
            &UserProfile::default(),
            &["   ".to_string()],
            &[ChatMessage::new(1, ChatRole::User, "   ")],
        );

        assert_eq!(built.system_prompt, None);
        assert!(built.history.is_empty());
    }

    #[test]
    fn build_prompt_preview_shows_code_and_text_modes() {
        let profile = UserProfile {
            name: Some("Lev".into()),
            emoji: PreferenceLevel::Less,
            ..UserProfile::default()
        };

        let preview = build_prompt_preview("Base prompt", &profile, &["Rust".into()]);

        assert!(preview.code.contains("build_profile_prompt(profile)"));
        assert!(preview.code.contains("\"emoji\": \"less\""));
        assert!(preview.text.contains("Base prompt"));
        assert!(preview.text.contains("Known user memory"));
    }
}
