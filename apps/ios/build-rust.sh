#!/usr/bin/env bash
# Builds the Rust core for iOS (device and Apple silicon simulator), generates the Swift
# bindings and packs them into Generated/GedrootsMobileFFI.xcframework.
set -euo pipefail
cd "$(dirname "$0")"
root=../..
profile=${PROFILE:-release}
rustup target add aarch64-apple-ios aarch64-apple-ios-sim >/dev/null
for target in aarch64-apple-ios aarch64-apple-ios-sim; do
  cargo build --manifest-path $root/Cargo.toml -p gedroots-mobile --lib --profile "$profile" --target "$target"
done
rm -rf Generated && mkdir -p Generated/headers
cargo run --manifest-path $root/Cargo.toml -p gedroots-mobile --bin uniffi-bindgen -- generate \
  --library "$root/target/aarch64-apple-ios/$profile/libgedroots_mobile.a" \
  --language swift --out-dir Generated/bindings
mv Generated/bindings/gedroots_mobile.swift Generated/
cp Generated/bindings/*.h Generated/headers/
cat Generated/bindings/*.modulemap > Generated/headers/module.modulemap
xcodebuild -create-xcframework \
  -library "$root/target/aarch64-apple-ios/$profile/libgedroots_mobile.a" -headers Generated/headers \
  -library "$root/target/aarch64-apple-ios-sim/$profile/libgedroots_mobile.a" -headers Generated/headers \
  -output Generated/GedrootsMobileFFI.xcframework
rm -rf Generated/bindings
echo "OK: Generated/GedrootsMobileFFI.xcframework"
