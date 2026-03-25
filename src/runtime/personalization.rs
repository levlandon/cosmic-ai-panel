//! Helpers for migrating raw notes into structured personalization settings.

use crate::chat::{AppSettings, ProviderKind, SavedModel, default_base_system_prompt};
use crate::personalization::{self, PersonalizationSettings};
use crate::provider::{self, ProviderMessage, ProviderRequest};

const AI_MIGRATION_SYSTEM_PROMPT_REMOTE: &str = r#"You convert raw user notes into strict JSON for an assistant personalization profile.

Return JSON only. Do not wrap it in markdown. Do not add explanations.

Use this exact shape:
{
  "schema_version": 1,
  "base_system_prompt": "string",
  "profile": {
    "name": "string or null",
    "language": "string or null",
    "occupation": "string or null",
    "response_style": "string or null",
    "more_about_you": "string or null",
    "header_lists": "more | default | less",
    "emoji": "more | default | less"
  },
  "memory": ["short factual memory item"]
}

Rules:
- Keep memory items short, factual, and reusable.
- Use null for unknown optional text fields.
- Use "default" when the raw notes do not clearly request more or less.
- If no better base system prompt is implied, use the default concise COSMIC assistant prompt.
- Preserve meaning from the raw notes, but normalize wording for assistant instructions.
"#;

const AI_MIGRATION_SYSTEM_PROMPT_LOCAL: &str = r#"You are a strict JSON transformer for assistant personalization.

Reply with exactly one valid JSON object.
- The first character must be {
- The last character must be }
- No markdown
- No explanations
- No prose before or after the JSON

Required JSON shape:
{
  "schema_version": 1,
  "base_system_prompt": "string",
  "profile": {
    "name": "string or null",
    "language": "string or null",
    "occupation": "string or null",
    "response_style": "string or null",
    "more_about_you": "string or null",
    "header_lists": "more | default | less",
    "emoji": "more | default | less"
  },
  "memory": ["short factual memory item"]
}

Rules:
- Every key above must exist.
- Unknown text fields must be null.
- header_lists and emoji must always be one of: more, default, less.
- memory must contain at most 8 short factual reusable items.
- If no better base_system_prompt is implied, use the provided default prompt exactly.
- Before replying, self-check that the output is valid JSON and matches the schema exactly.
"#;

pub fn ai_migration_helper_prompt() -> String {
    format!(
        r#"You extract assistant personalization settings from source notes and return exactly one fenced JSON code block. Do not add explanations, lists, headings, or any text before or after the code block.

Use this exact schema:
{{
  "schema_version": 1,
  "base_system_prompt": "string",
  "profile": {{
    "name": "string or null",
    "language": "string or null",
    "occupation": "string or null",
    "response_style": "string or null",
    "more_about_you": "string or null",
    "header_lists": "more | default | less",
    "emoji": "more | default | less"
  }},
  "memory": ["short factual memory item"]
}}

Rules:
- Do not invent facts that are missing from the source notes.
- Keep text concise. `name`, `language`, and `occupation` should stay short.
- `response_style` and `more_about_you` should be at most 2 short sentences each.
- `memory` must contain at most 8 items. Each item must be factual, reusable, and under 120 characters.
- Use null for unknown text fields.
- `header_lists` and `emoji` must always be one of: more, default, less.
- If the source notes do not imply a better system prompt, use this default value exactly: {default_prompt:?}
- Output only one ```json``` code block.

Source notes:
<paste source notes here>"#,
        default_prompt = default_base_system_prompt(),
    )
}

pub fn ai_migration_helper_prompt_markdown() -> String {
    format!("```text\n{}\n```", ai_migration_helper_prompt())
}

pub fn build_ai_migration_request(
    settings: &AppSettings,
    session_openrouter_key: Option<&str>,
    model: &SavedModel,
    raw_input: &str,
    previous_error: Option<&str>,
) -> Result<ProviderRequest, String> {
    let raw_input = raw_input.trim();
    if raw_input.is_empty() {
        return Err("Paste raw personalization notes first.".into());
    }

    let mut user_message = match model.provider {
        ProviderKind::LmStudio => format!(
            "Default base prompt (use exactly when no better prompt is implied):\n{}\n\nSource notes:\n{}\n\nReturn exactly one valid JSON object that matches the required schema. Keep response_style and more_about_you concise. Do not wrap the JSON in markdown.",
            default_base_system_prompt(),
            raw_input
        ),
        ProviderKind::OpenRouter => format!(
            "Default base prompt:\n{}\n\nRaw personalization notes:\n{}",
            default_base_system_prompt(),
            raw_input
        ),
    };

    if let Some(previous_error) = previous_error
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        user_message.push_str("\n\nPrevious attempt failed with this error:\n");
        user_message.push_str(previous_error);
        user_message.push_str("\n\nReturn corrected JSON only and fix the problem above.");
    }

    provider::build_custom_request(
        settings,
        session_openrouter_key,
        model.provider,
        &model.name,
        vec![
            ProviderMessage {
                role: "system".into(),
                content: match model.provider {
                    ProviderKind::LmStudio => AI_MIGRATION_SYSTEM_PROMPT_LOCAL,
                    ProviderKind::OpenRouter => AI_MIGRATION_SYSTEM_PROMPT_REMOTE,
                }
                .into(),
            },
            ProviderMessage {
                role: "user".into(),
                content: user_message,
            },
        ],
    )
}

#[derive(Debug, Clone)]
pub struct AiMigrationOutcome {
    pub personalization: PersonalizationSettings,
    pub completion_ratio: f32,
}

pub fn process_ai_migration_response(response: &str) -> Result<AiMigrationOutcome, String> {
    let personalization = parse_ai_migration_response(response)?;
    let completion_ratio = personalization_completion_ratio(&personalization);

    Ok(AiMigrationOutcome {
        personalization,
        completion_ratio,
    })
}

pub fn parse_ai_migration_response(response: &str) -> Result<PersonalizationSettings, String> {
    let candidates = vec![
        response.trim().to_string(),
        strip_markdown_fences(response),
        extract_json_object(response),
    ];

    for candidate in candidates {
        if candidate.trim().is_empty() {
            continue;
        }

        if let Some(settings) = parse_personalization_candidate(&candidate) {
            return Ok(settings);
        }
    }

    Err("Model response was not valid personalization JSON.".into())
}

pub fn personalization_completion_ratio(settings: &PersonalizationSettings) -> f32 {
    let mut score = 0.0_f32;
    let total = 9.0_f32;

    if !settings.base_system_prompt.trim().is_empty() {
        score += 1.0;
    }
    if settings.profile.name.is_some() {
        score += 1.0;
    }
    if settings.profile.language.is_some() {
        score += 1.0;
    }
    if settings.profile.occupation.is_some() {
        score += 1.0;
    }
    if settings.profile.response_style.is_some() {
        score += 1.0;
    }
    if settings.profile.more_about_you.is_some() {
        score += 1.0;
    }
    if !settings.memory.is_empty() {
        score += 1.0;
    }
    if settings.profile.header_lists.label() != "Default" {
        score += 1.0;
    }
    if settings.profile.emoji.label() != "Default" {
        score += 1.0;
    }

    (score / total).clamp(0.0, 1.0)
}

fn parse_personalization_candidate(candidate: &str) -> Option<PersonalizationSettings> {
    let candidate = candidate.trim().trim_start_matches('\u{feff}');

    if let Ok(settings) = personalization::import_personalization(candidate) {
        return Some(settings);
    }

    if let Ok(decoded) = serde_json::from_str::<String>(candidate)
        && let Ok(settings) = personalization::import_personalization(decoded.trim())
    {
        return Some(settings);
    }

    let repaired = repair_json_candidate(candidate);
    if repaired != candidate {
        if let Ok(settings) = personalization::import_personalization(&repaired) {
            return Some(settings);
        }

        if let Ok(decoded) = serde_json::from_str::<String>(&repaired)
            && let Ok(settings) = personalization::import_personalization(decoded.trim())
        {
            return Some(settings);
        }
    }

    None
}

fn strip_markdown_fences(response: &str) -> String {
    let trimmed = response.trim();
    let Some(stripped) = trimmed.strip_prefix("```") else {
        return trimmed.to_string();
    };
    let Some(end) = stripped.rfind("```") else {
        return trimmed.to_string();
    };
    let inner = stripped[..end].trim();
    inner
        .split_once('\n')
        .map(|(_, body)| body.trim().to_string())
        .unwrap_or_else(|| inner.to_string())
}

fn extract_json_object(response: &str) -> String {
    let trimmed = response.trim();
    let mut start = None;
    let mut depth = 0_u32;
    let mut in_string = false;
    let mut escape = false;

    for (index, ch) in trimmed.char_indices() {
        if in_string {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' => {
                if depth == 0 {
                    start = Some(index);
                }
                depth += 1;
            }
            '}' => {
                if depth == 0 {
                    continue;
                }
                depth -= 1;
                if depth == 0
                    && let Some(start) = start
                {
                    return trimmed[start..=index].to_string();
                }
            }
            _ => {}
        }
    }

    String::new()
}

fn repair_json_candidate(candidate: &str) -> String {
    let chars: Vec<char> = candidate.chars().collect();
    let mut repaired = String::with_capacity(candidate.len());
    let mut in_string = false;
    let mut escape = false;

    for (index, ch) in chars.iter().copied().enumerate() {
        if in_string {
            repaired.push(ch);
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => {
                in_string = true;
                repaired.push(ch);
            }
            ',' => {
                let mut lookahead = index + 1;
                while lookahead < chars.len() && chars[lookahead].is_whitespace() {
                    lookahead += 1;
                }

                if lookahead < chars.len() && matches!(chars[lookahead], '}' | ']') {
                    continue;
                }

                repaired.push(ch);
            }
            _ => repaired.push(ch),
        }
    }

    repaired
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::{PreferenceLevel, ProviderKind, UserProfile};

    #[test]
    fn parse_ai_migration_response_accepts_fenced_json() {
        let response = r#"```json
{
  "schema_version": 1,
  "base_system_prompt": "Stay concise",
  "profile": {
    "name": "Lev",
    "language": "English",
    "occupation": null,
    "response_style": null,
    "more_about_you": null,
    "header_lists": "default",
    "emoji": "less"
  },
  "memory": ["Uses COSMIC"]
}
```"#;

        let parsed = parse_ai_migration_response(response).unwrap();

        assert_eq!(parsed.profile.name.as_deref(), Some("Lev"));
        assert_eq!(parsed.profile.emoji, PreferenceLevel::Less);
    }

    #[test]
    fn parse_ai_migration_response_repairs_trailing_commas() {
        let response = r#"{
  "schema_version": 1,
  "base_system_prompt": "Stay concise",
  "profile": {
    "name": "Lev",
    "language": "English",
    "occupation": null,
    "response_style": null,
    "more_about_you": null,
    "header_lists": "default",
    "emoji": "less",
  },
  "memory": ["Uses COSMIC",],
}"#;

        let parsed = parse_ai_migration_response(response).unwrap();

        assert_eq!(parsed.profile.name.as_deref(), Some("Lev"));
        assert_eq!(parsed.memory, vec!["Uses COSMIC"]);
    }

    #[test]
    fn parse_ai_migration_response_accepts_json_string_payload() {
        let response = r#""{\"schema_version\":1,\"base_system_prompt\":\"Stay concise\",\"profile\":{\"name\":\"Lev\",\"language\":null,\"occupation\":null,\"response_style\":null,\"more_about_you\":null,\"header_lists\":\"default\",\"emoji\":\"default\"},\"memory\":[\"Uses COSMIC\"]}""#;

        let parsed = parse_ai_migration_response(response).unwrap();

        assert_eq!(parsed.profile.name.as_deref(), Some("Lev"));
        assert_eq!(parsed.memory, vec!["Uses COSMIC"]);
    }

    #[test]
    fn process_ai_migration_response_returns_completion_ratio() {
        let response = r#"{
  "schema_version": 1,
  "base_system_prompt": "Stay concise",
  "profile": {
    "name": "Lev",
    "language": null,
    "occupation": null,
    "response_style": "Short paragraphs",
    "more_about_you": null,
    "header_lists": "default",
    "emoji": "less"
  },
  "memory": ["Uses COSMIC"]
}"#;

        let outcome = process_ai_migration_response(response).unwrap();

        assert!(outcome.completion_ratio > 0.3);
        assert_eq!(outcome.personalization.profile.name.as_deref(), Some("Lev"));
    }

    #[test]
    fn completion_ratio_tracks_filled_personalization() {
        let settings = PersonalizationSettings {
            base_system_prompt: "Stay concise".into(),
            profile: UserProfile {
                name: Some("Lev".into()),
                language: Some("English".into()),
                occupation: None,
                response_style: None,
                more_about_you: None,
                header_lists: PreferenceLevel::More,
                emoji: PreferenceLevel::Default,
            },
            memory: vec!["Uses COSMIC".into()],
        };

        let ratio = personalization_completion_ratio(&settings);

        assert!(ratio > 0.4);
        assert!(ratio < 1.0);
    }

    #[test]
    fn build_ai_migration_request_uses_selected_model() {
        let settings = AppSettings::default();
        let model = SavedModel::new(ProviderKind::LmStudio, "local-model");

        let request =
            build_ai_migration_request(&settings, None, &model, "Works with Rust", None).unwrap();

        assert_eq!(request.provider, ProviderKind::LmStudio);
        assert_eq!(request.model, "local-model");
        assert_eq!(request.messages.len(), 2);
    }

    #[test]
    fn build_ai_migration_request_includes_previous_error_feedback() {
        let settings = AppSettings::default();
        let model = SavedModel::new(ProviderKind::OpenRouter, "openrouter/test");

        let request = build_ai_migration_request(
            &settings,
            Some("token"),
            &model,
            "Works with Rust",
            Some("Model response was not valid personalization JSON."),
        )
        .unwrap();

        assert!(
            request.messages[1]
                .content
                .contains("Previous attempt failed with this error")
        );
        assert!(request.messages[1].content.contains("corrected JSON only"));
    }

    #[test]
    fn local_ai_migration_request_uses_stricter_contract() {
        let settings = AppSettings::default();
        let model = SavedModel::new(ProviderKind::LmStudio, "local-model");

        let request =
            build_ai_migration_request(&settings, None, &model, "Works with Rust", None).unwrap();

        assert!(
            request.messages[0]
                .content
                .contains("The first character must be {")
        );
        assert!(
            request.messages[1]
                .content
                .contains("Return exactly one valid JSON object")
        );
    }

    #[test]
    fn helper_prompt_markdown_is_copy_ready() {
        let prompt = ai_migration_helper_prompt_markdown();

        assert!(prompt.starts_with("```text\n"));
        assert!(prompt.contains("\"schema_version\": 1"));
        assert!(prompt.contains("Output only one ```json``` code block."));
    }
}
