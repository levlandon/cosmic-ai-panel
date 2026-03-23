#!/usr/bin/env bash
set -euo pipefail

rm -f "${HOME}/.local/bin/cosmic-ai-panel"
rm -f "${HOME}/.local/share/applications/cosmic-ai-panel.desktop"
rm -f "${HOME}/.local/share/metainfo/com.levlandon.cosmic-ai-panel.metainfo.xml"
rm -f "${HOME}/.local/share/icons/hicolor/scalable/apps/cosmic-ai-panel.svg"

if command -v appstreamcli >/dev/null 2>&1; then
    appstreamcli refresh-cache --user >/dev/null 2>&1 \
        || appstreamcli refresh-cache >/dev/null 2>&1 \
        || true
fi

pkill cosmic-panel || true

echo "Uninstalled Cosmic AI Panel from ${HOME}/.local"
