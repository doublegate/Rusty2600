// swift-tools-version:5.9
import PackageDescription

// A local Swift Package wrapping the `rusty2600-mobile` UniFFI bridge for
// the iOS app (`../Rusty2600/`) — the same crate the Android app
// (`../../android/`) already consumes via its Kotlin bindings.
//
// `rusty2600_mobileFFI` is a `binaryTarget` pointing at
// `Rusty2600Mobile.xcframework`, which does NOT exist in this checkout.
// Building it requires cross-compiling `rusty2600-mobile` for
// `aarch64-apple-ios` + `aarch64-apple-ios-sim` on a real Mac with Xcode
// installed (this project was authored on Linux, which cannot target
// Apple's SDKs at all) — see `../regenerate-bindings.sh` for the exact
// steps. Until that xcframework is produced, `swift build`/Xcode cannot
// resolve this package; the `Sources/RustyMobileFFI/rusty2600_mobile.swift`
// file itself, however, IS real, tool-generated output (see
// `../Generated/` and `docs/mobile.md` for provenance) — only the compiled
// native binary is missing, not the Swift source.
let package = Package(
    name: "RustyMobileFFI",
    platforms: [.iOS(.v16)],
    products: [
        .library(name: "RustyMobileFFI", targets: ["RustyMobileFFI"]),
    ],
    targets: [
        .binaryTarget(
            name: "rusty2600_mobileFFI",
            path: "Rusty2600Mobile.xcframework"
        ),
        .target(
            name: "RustyMobileFFI",
            dependencies: ["rusty2600_mobileFFI"],
            path: "Sources/RustyMobileFFI"
        ),
    ]
)
