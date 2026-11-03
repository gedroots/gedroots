# GEDRoots mobile

Read-only iPhone and iPad app. It receives the tree from GEDRoots on the computer
(File › Mobile Devices…), peer to peer over iroh: end-to-end encrypted, direct when possible,
through a relay otherwise. Each save on the computer publishes a new version, and the app only
downloads what changed.

The Rust core is `crates/mobile` (UniFFI), on top of `crates/sync`.

## Build

Needs Xcode, XcodeGen (`brew install xcodegen`) and the iOS Rust targets.

```sh
cd apps/ios
./build-rust.sh                       # Rust core + Swift bindings → Generated/
DEVELOPMENT_TEAM=XXXXXXXXXX xcodegen generate
open GEDRoots.xcodeproj               # pick your iPhone, Run
```

A free Apple ID is enough to run it on your own device (the app expires after 7 days).

## Pairing

On the computer, open the tree, choose File › Mobile Devices…, turn sharing on and choose Add a
Device. In the app, scan the QR code (or copy the code on the computer and paste it). The code
works once and expires after 10 minutes; the device is then known by its own key, kept in the
app's folder.
