#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

BIN_DIR="${HOME}/.local/bin"
APPS_DIR="${HOME}/.local/share/applications"
METAINFO_DIR="${HOME}/.local/share/metainfo"
ICON_DIR="${HOME}/.local/share/icons/hicolor/scalable/apps"

mkdir -p "${BIN_DIR}" "${APPS_DIR}" "${METAINFO_DIR}" "${ICON_DIR}"

install -Dm755 "${SCRIPT_DIR}/cosmic-ai-panel" "${BIN_DIR}/cosmic-ai-panel"
install -Dm644 "${SCRIPT_DIR}/cosmic-ai-panel.desktop" "${APPS_DIR}/cosmic-ai-panel.desktop"
install -Dm644 "${SCRIPT_DIR}/app.metainfo.xml" "${METAINFO_DIR}/com.levlandon.cosmic-ai-panel.metainfo.xml"
install -Dm644 "${SCRIPT_DIR}/cosmic-ai-panel.svg" "${ICON_DIR}/cosmic-ai-panel.svg"

if command -v appstreamcli >/dev/null 2>&1; then
    appstreamcli refresh-cache --user >/dev/null 2>&1 \
        || appstreamcli refresh-cache >/dev/null 2>&1 \
        || true
fi

pkill cosmic-panel || true

echo "Installed Cosmic AI Panel to ${HOME}/.local"
