import SwiftUI

/// The v1.12.0 "Pocket" app entry point — the iOS counterpart to
/// `android/app/src/main/java/com/doublegate/rusty2600/MainActivity.kt`,
/// reusing the same `rusty2600-mobile` UniFFI bridge via `RustyMobileFFI`.
///
/// This file is not itself an Xcode project — there is no `.xcodeproj` in
/// this checkout (see `docs/mobile.md`'s "The iOS app" section for why, and
/// for the exact steps to wire these sources into a real Xcode "iOS App"
/// project on a Mac). Every file under `Sources/` is real, believed-correct
/// Swift/SwiftUI/Metal/AVFoundation source, but none of it has been compiled
/// or run — there is no Apple toolchain in the sandbox this was authored in.
@main
struct Rusty2600App: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
        }
    }
}
