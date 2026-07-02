# This directory intentionally does not contain a real xcframework

`Package.swift`'s `rusty2600_mobileFFI` binary target points here. This
placeholder exists only so the directory is present in the checkout and the
gap is impossible to miss; it is not itself consumable by Xcode/SwiftPM.

Producing the real `Rusty2600Mobile.xcframework` requires a Mac with Xcode
and the `aarch64-apple-ios`/`aarch64-apple-ios-sim` Rust targets installed —
see `../../regenerate-bindings.sh` for the exact commands. This project was
authored in a Linux sandbox with no Apple toolchain (`xcodebuild`/`xcrun`/
`swift` all absent), so that step could not be run here. Delete this file
once the real xcframework is built and copied into this directory.
