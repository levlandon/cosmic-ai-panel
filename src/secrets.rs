// SPDX-License-Identifier: MPL-2.0

use keyring::{Entry, Error};

const SERVICE_NAME: &str = "com.levlon.aipanel";
const OPENROUTER_USERNAME: &str = "openrouter_api_key";

fn openrouter_entry() -> Result<Entry, String> {
    Entry::new(SERVICE_NAME, OPENROUTER_USERNAME)
        .map_err(|error| format!("Failed to initialize secure storage: {error}"))
}

pub fn load_openrouter_api_key() -> Result<Option<String>, String> {
    let entry = openrouter_entry()?;

    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(Error::NoEntry) => Ok(None),
        Err(error) => Err(format!("Failed to load OpenRouter API key: {error}")),
    }
}

pub fn save_openrouter_api_key(api_key: &str) -> Result<(), String> {
    let entry = openrouter_entry()?;
    let api_key = api_key.trim();

    if api_key.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(error) => Err(format!("Failed to clear OpenRouter API key: {error}")),
        }
    } else {
        entry
            .set_password(api_key)
            .map_err(|error| format!("Failed to save OpenRouter API key: {error}"))
    }
}
