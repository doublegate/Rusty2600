#!/usr/bin/env bash
# Regenerates the Android app's `rusty2600-mobile` glue: cross-compiled
# `.so`s per ABI (via `cargo-ndk`) and the UniFFI-generated Kotlin bindings.
# Not run by Gradle itself — `rusty2600-mobile`'s Rust source is the source
# of truth; re-run this manually after changing its `#[uniffi::export]`
# surface, then re-check the two generated artifact classes into git.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo ndk -t arm64-v8a -t x86_64 -o target/android-jniLibs build -p rusty2600-mobile --release

cargo run -p rusty2600-mobile --features bindgen --bin uniffi-bindgen -- generate \
  --library target/x86_64-linux-android/release/librusty2600_mobile.so \
  --language kotlin \
  --out-dir target/android-kotlin

cp target/android-jniLibs/arm64-v8a/librusty2600_mobile.so android/app/src/main/jniLibs/arm64-v8a/
cp target/android-jniLibs/x86_64/librusty2600_mobile.so android/app/src/main/jniLibs/x86_64/
cp target/android-kotlin/uniffi/rusty2600_mobile/rusty2600_mobile.kt \
  android/app/src/main/java/uniffi/rusty2600_mobile/rusty2600_mobile.kt

echo "Regenerated. Review the diff, then commit both the .so files and the .kt binding."
