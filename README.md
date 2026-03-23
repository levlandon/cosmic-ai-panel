# cosmic-ai-panel

Minimal AI assistant panel for COSMIC desktop.

cosmic-ai-panel adds a lightweight assistant directly into the COSMIC panel with support for OpenRouter and LM Studio providers.

## Features

* panel-integrated assistant UI
* streaming responses
* editable user messages
* regenerate responses
* branch conversations
* context limit control
* system prompt editor
* local-first chat storage
* placeholder onboarding UI
* safe persistence with backup fallback

## Supported providers

* OpenRouter
* LM Studio (local models)

## Installation

From COSMIC Store (recommended)

or build manually:

cargo build --release

## Configuration

Set provider and model inside Settings panel.

Example OpenRouter model:

openrouter/free

## Status

Alpha release. Stable core functionality implemented.

## License

MIT
