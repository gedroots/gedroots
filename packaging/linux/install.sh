#!/bin/sh
# Installe GEDRoots pour l'utilisateur courant (menu des applications, icône), sans droits root.
set -eu
cd "$(dirname "$0")/../.."
CARGO=${CARGO:-cargo}
$CARGO build --release -p gedroots
TARGET=$($CARGO metadata --format-version 1 --no-deps | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
DATA=${XDG_DATA_HOME:-$HOME/.local/share}
install -Dm755 "$TARGET/release/gedroots" "$HOME/.local/bin/gedroots"
install -Dm644 packaging/icon.png "$DATA/icons/hicolor/1024x1024/apps/gedroots.png"
install -Dm644 packaging/linux/gedroots.desktop "$DATA/applications/gedroots.desktop"
update-desktop-database "$DATA/applications" 2>/dev/null || true
echo "→ GEDRoots installé (~/.local/bin/gedroots)"
