#!/usr/bin/env bash
# Build and install Priogator for the current user:
#   ~/.local/bin/priogator, plus an app-launcher entry and icon.
set -euo pipefail
cd "$(dirname "$0")/.."
# rustup puts cargo here; not every shell has it on PATH.
command -v cargo >/dev/null || source "$HOME/.cargo/env"

cargo build --release

bin="$HOME/.local/bin"
apps="$HOME/.local/share/applications"
icons="$HOME/.local/share/icons/hicolor/1024x1024/apps"
mkdir -p "$bin" "$apps" "$icons"

install -m 755 target/release/priogator "$bin/priogator"
install -m 644 assets/icon.png "$icons/priogator.png"
cat > "$apps/priogator.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Priogator
Comment=To-do list with columns and easy re-prioritizing
Exec=$bin/priogator
Icon=priogator
Categories=Office;Utility;
StartupWMClass=priogator
DESKTOP

update-desktop-database "$apps" 2>/dev/null || true
echo "Installed $bin/priogator and the launcher entry."
