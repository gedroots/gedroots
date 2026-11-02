#!/bin/sh
# Builds GEDRoots.app (and, with --dmg, a disk image) from the sources.
#
#   packaging/macos/bundle.sh             # → <target>/release/GEDRoots.app
#   packaging/macos/bundle.sh --install   # … then copies it to /Applications
#   packaging/macos/bundle.sh --dmg       # … and <target>/release/GEDRoots.dmg
#
# Normally run by `mise run bundle` / `mise run install`, which provide the project's
# toolchain; otherwise CARGO chooses the cargo command.
# The build folder follows CARGO_TARGET_DIR / .cargo/config like `cargo build`.
set -eu

cd "$(dirname "$0")/../.."
CARGO=${CARGO:-cargo}
INSTALL=0
DMG=0
for arg in "$@"; do
	case "$arg" in
	--install) INSTALL=1 ;;
	--dmg) DMG=1 ;;
	*) echo "unknown option: $arg" >&2; exit 2 ;;
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
# Translations of the Info.plist strings (file type name…).
cp -R packaging/macos/*.lproj "$APP/Contents/Resources/"

# Icon: every size macOS expects, from the 1024 px image.
ICONSET=$(mktemp -d)/GEDRoots.iconset
mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
	sips -z $s $s packaging/icon.png --out "$ICONSET/icon_${s}x${s}.png" > /dev/null
	d=$((s * 2))
	sips -z $d $d packaging/icon.png --out "$ICONSET/icon_${s}x${s}@2x.png" > /dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/GEDRoots.icns"
rm -rf "$(dirname "$ICONSET")"

# “Ad hoc” signature (no developer account): enough to run the app on this Mac.
codesign --force --sign - "$APP"
echo "→ $APP"

if [ "$INSTALL" = 1 ]; then
	rm -rf /Applications/GEDRoots.app
	cp -R "$APP" /Applications/
	# The Finder and the Dock read the icon and the supported file types again.
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
