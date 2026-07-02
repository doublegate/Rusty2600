#!/usr/bin/env bash
# Regenerates the iOS app's `rusty2600-mobile` glue: a real xcframework
# (cross-compiled for aarch64-apple-ios + aarch64-apple-ios-sim) and the
# UniFFI-generated Swift bindings. Mirrors `../android/regenerate-bindings.sh`
# for the Android build.
#
# REQUIRES A MAC WITH XCODE INSTALLED. Every step from "cargo build --target
# aarch64-apple-ios" onward needs Apple's iOS SDK, which only ships inside
# Xcode — there is no way to cross-compile for iOS from Linux/Windows. This
# script was authored (but never run) in a Linux sandbox with no `xcodebuild`/
# `xcrun`/`swift` present; only the final `uniffi-bindgen generate` step was
# actually exercised there, against a Linux-native build of the same crate,
# to prove the generated Swift source is correct (see docs/mobile.md).
set -euo pipefail
cd "$(dirname "$0")/.."

rustup target add aarch64-apple-ios aarch64-apple-ios-sim

cargo build -p rusty2600-mobile --release --target aarch64-apple-ios
cargo build -p rusty2600-mobile --release --target aarch64-apple-ios-sim

# The generated Swift bindings only need ANY compiled build of the crate to
# introspect its UniFFI metadata (metadata is identical regardless of target
# triple) — a plain host build works fine for this step, same as Android's
# script uses its x86_64-linux-android build rather than arm64-v8a.
cargo run -p rusty2600-mobile --features bindgen --bin uniffi-bindgen -- generate \
  --library target/release/librusty2600_mobile.so \
  --language swift \
  --out-dir target/ios-swift

# Assemble the xcframework, embedding the generated header + modulemap into
# each platform slice (the standard UniFFI/Swift-on-iOS packaging shape) so
# `import rusty2600_mobileFFI` resolves via the binary target alone.
mkdir -p target/ios-headers/aarch64-apple-ios
mkdir -p target/ios-headers/aarch64-apple-ios-sim
cp target/ios-swift/rusty2600_mobileFFI.h target/ios-headers/aarch64-apple-ios/
cp target/ios-swift/rusty2600_mobileFFI.modulemap target/ios-headers/aarch64-apple-ios/module.modulemap
cp target/ios-swift/rusty2600_mobileFFI.h target/ios-headers/aarch64-apple-ios-sim/
cp target/ios-swift/rusty2600_mobileFFI.modulemap target/ios-headers/aarch64-apple-ios-sim/module.modulemap

rm -rf ios/RustyMobileFFI/Rusty2600Mobile.xcframework
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/librusty2600_mobile.a -headers target/ios-headers/aarch64-apple-ios \
  -library target/aarch64-apple-ios-sim/release/librusty2600_mobile.a -headers target/ios-headers/aarch64-apple-ios-sim \
  -output ios/RustyMobileFFI/Rusty2600Mobile.xcframework

cp target/ios-swift/rusty2600_mobile.swift ios/RustyMobileFFI/Sources/RustyMobileFFI/rusty2600_mobile.swift
cp target/ios-swift/rusty2600_mobileFFI.h ios/RustyMobileFFI/Generated/rusty2600_mobileFFI.h
cp target/ios-swift/rusty2600_mobileFFI.modulemap ios/RustyMobileFFI/Generated/rusty2600_mobileFFI.modulemap

echo "Regenerated. Review the diff, then commit the xcframework and both generated Swift/header copies."
