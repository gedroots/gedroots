#!/bin/sh
# Construit GEDRoots.app (et, avec --dmg, une image disque) à partir des sources.
#
#   packaging/macos/bundle.sh             # → <target>/release/GEDRoots.app
#   packaging/macos/bundle.sh --install   # … puis copie dans /Applications
#   packaging/macos/bundle.sh --dmg       # … et <target>/release/GEDRoots.dmg
#
# Normalement lancé par `mise run bundle` / `mise run install`, qui fournit la toolchain du
# projet ; CARGO permet sinon de choisir la commande cargo.
# Le dossier de compilation suit CARGO_TARGET_DIR / .cargo/config comme `cargo build`.
set -eu

cd "$(dirname "$0")/../.."
CARGO=${CARGO:-cargo}
INSTALL=0
DMG=0
for arg in "$@"; do
	case "$arg" in
	--install) INSTALL=1 ;;
	--dmg) DMG=1 ;;
	*) echo "option inconnue : $arg" >&2; exit 2 ;;
	esac
done

$CARGO build --release -p gedroots

TARGET=$($CARGO metadata --format-version 1 --no-deps | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' crates/gedroots/Cargo.toml | head -n 1)
OUT="$TARGET/release"
APP="$OUT/GEDRoots.app"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$OUT/gedroots" "$APP/Contents/MacOS/gedroots"
sed "s/@VERSION@/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
printf 'APPL????' > "$APP/Contents/PkgInfo"

# Icône : toutes les tailles attendues par macOS, à partir de l'image 1024 px.
ICONSET=$(mktemp -d)/GEDRoots.iconset
mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
	sips -z $s $s packaging/icon.png --out "$ICONSET/icon_${s}x${s}.png" > /dev/null
	d=$((s * 2))
	sips -z $d $d packaging/icon.png --out "$ICONSET/icon_${s}x${s}@2x.png" > /dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/GEDRoots.icns"
rm -rf "$(dirname "$ICONSET")"

# Signature « ad hoc » (sans compte développeur) : suffisante pour lancer l'app sur ce Mac.
codesign --force --sign - "$APP"
echo "→ $APP"

if [ "$INSTALL" = 1 ]; then
	rm -rf /Applications/GEDRoots.app
	cp -R "$APP" /Applications/
	# Le Finder et le Dock relisent l'icône et les types de fichiers gérés.
	/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f /Applications/GEDRoots.app || true
	echo "→ /Applications/GEDRoots.app"
fi

if [ "$DMG" = 1 ]; then
	STAGE=$(mktemp -d)
	cp -R "$APP" "$STAGE/"
	ln -s /Applications "$STAGE/Applications"
	rm -f "$OUT/GEDRoots.dmg"
	hdiutil create -volname GEDRoots -srcfolder "$STAGE" -ov -format UDZO "$OUT/GEDRoots.dmg" > /dev/null
	rm -rf "$STAGE"
	echo "→ $OUT/GEDRoots.dmg"
fi
