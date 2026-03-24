//! Build the final provider context from prompt layers and chat history.

use crate::chat::{ChatMessage, ChatRole, UserProfile};

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
    if let Some(style) = &profile.response_style {
        lines.push(format!("- Response style: {style}"));
    }

    Some(format!("User profile:\n{}", lines.join("\n")))
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

#[cfg(test)]
mod tests {
    use super::build_context;
    use crate::chat::{ChatMessage, ChatRole, UserProfile};

    #[test]
    fn build_context_assembles_system_layers() {
        let profile = UserProfile {
            name: Some("Lev".into()),
            language: Some("English".into()),
            response_style: Some("Concise".into()),
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
                "Base prompt\n\nUser profile:\n- Name: Lev\n- Preferred language: English\n- Response style: Concise\n\nKnown user memory:\n- User uses COSMIC desktop\n- User works with Rust"
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
}
