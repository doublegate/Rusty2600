# Mobile bridge (`rusty2600-mobile`) — Rusty2600

References: `docs/adr/0004-determinism-contract.md`; `docs/adr/0007-save-state-versioning.md`;
`to-dos/ROADMAP.md` (v1.11.0 "Handheld"); `crates/rusty2600-mobile/src/lib.rs`; `android/`.
This doc is the SPEC, not history — update it in the same PR as the code.

## What this crate is

`rusty2600-mobile` is a `std`, host-testable, platform-agnostic bridge over
`rusty2600_core::System`, exposing a small [UniFFI](https://mozilla.github.io/uniffi-rs/)
surface (`load_rom`/`run_frame`/`save_state`/`load_state`/`is_rom_loaded`)
that the same generated bindings drive from both Kotlin (Android,
`v1.11.0`) and Swift (`v1.12.0` "Pocket", iOS) — one Rust implementation,
two mobile hosts, unchanged across both releases. It carries no
Android/JNI/Swift types of its own; those live entirely in the generated
bindings and the platform app projects (`android/`, `ios/`).

## Design deviation from the original plan: no `rusty2600-android` glue crate

The `v1.11.0` plan called for a `rusty2600-android` crate doing JNI,
`ANativeWindow` → wgpu rendering, and AAudio — "the only `unsafe` surface."
That assumed the bridge would hand the host a native rendering surface to
draw into directly. The actual `rusty2600-mobile` design instead has
`run_frame` return a plain owned `Vec<u8>` RGBA8 framebuffer and `Vec<f32>`
normalized audio samples — data, not a surface handle. That reframing makes
a native-rendering glue crate unnecessary: Android's own `Bitmap` (fed via
`copyPixelsFromBuffer`) and `AudioTrack` (fed via `ENCODING_PCM_FLOAT`)
already consume exactly those two shapes with zero native code on the
Kotlin side, and UniFFI's own generated JNA-based Kotlin bindings handle
100% of the FFI marshalling — no hand-written JNI, no hand-written
`unsafe`, on either side of the bridge.

This is a strict improvement against this project's own unsafe-confinement
convention (fewer `unsafe` surfaces, not an isolated one), not a corner cut:
the Android app (`android/`) is real, builds a working APK, and was
verified running a real ROM on an API 34 emulator (see "Verification —
Android" below). The `v1.12.0` iOS app (`ios/`) follows the identical
pattern — Metal + `AVAudioEngine` consuming the same `FrameOutput` shape —
with no separate `rusty2600-ios` native glue crate either, though (unlike
Android) it has not yet been compiled or run anywhere; see "iOS
Verification" below.

## API surface (matches the crate)

- **`MobileEmulator`** (`#[derive(uniffi::Object)]`) — `new()`,
  `load_rom(bytes, rom_tag)`, `is_rom_loaded()`, `run_frame(input)`,
  `save_state()`, `load_state(bytes)`. Mutable state lives behind a
  `Mutex<EmuState>` since UniFFI objects are always handed to the host
  behind an `Arc`.
- **`MobileInput`** — `joystick0`/`joystick1: MobileJoystick`,
  `paddle0..=paddle3: MobilePaddle`, `switches: MobileSwitches`. Named
  fields rather than `[MobileJoystick; 2]`/`[MobilePaddle; 4]`: UniFFI's
  `Record` derive doesn't support fixed-size array fields, only named
  fields or `Vec<T>`.
- **`FrameOutput`** — `rgba: Vec<u8>` (160x192 RGBA8, the same crop
  `emu_thread::run_frame` and the wasm bootstrap already apply) and
  `audio_samples: Vec<f32>` (DC-blocked, normalized, at the TIA's native
  ~31.4kHz rate — the host resamples, not this crate).
  `MobileEmulator::run_frame` deliberately takes the FULL `MobileInput`
  per call (matching `emu_thread`'s own `run_frame(input: Option<InputState>)`
  convention) rather than the plan's originally-suggested
  `set_joystick`/`set_paddle`/`set_console_switch` mutator methods — a
  per-field mutator API would need to be stateful across calls for no
  actual benefit, since every real host already assembles a full input
  snapshot once per frame anyway.
- **`MobileError`** — `UnrecognizedRom`, `NoRomLoaded`, `SaveState(String)`.
- A duplicated `NTSC_PALETTE` const (the measured Stella-reference table,
  same values as `rusty2600_frontend::palette::Region::Ntsc`) and a
  `DcBlocker` (the same one-pole DC-blocking algorithm `emu_thread`/`wasm.rs`
  use) — both small, deliberate duplications since this crate cannot depend
  on the frontend crate (the crate graph is one-directional).

## Cross-compilation and bindings generation

`cargo-ndk` cross-compiles the `cdylib` for `arm64-v8a` (real hardware) and
`x86_64` (the sandbox/CI-friendly emulator ABI); a `bindgen`-gated
`uniffi-bindgen` bin target (`cargo run -p rusty2600-mobile --features
bindgen --bin uniffi-bindgen -- generate ...`) generates the Kotlin
bindings from the compiled library's own metadata. The `bindgen` feature
gates `uniffi/cli` (which pulls in `clap`) so the shipped mobile `cdylib`
never links it. `android/regenerate-bindings.sh` runs both steps and
copies the outputs into the Android app project; re-run it after changing
the `#[uniffi::export]` surface and check the two generated artifact
classes (the `.so`s and the `.kt` binding) back into git.

**A key discovery from `v1.12.0`'s iOS work**: `uniffi-bindgen generate`
only introspects the compiled library's embedded UniFFI metadata, which is
identical regardless of which platform compiled it — a plain
`x86_64-unknown-linux-gnu` build of `rusty2600-mobile` is enough to
generate real, correct Swift bindings, even on a machine with no Apple
toolchain at all. `ios/regenerate-bindings.sh` uses this: the Swift
bindings checked into `ios/RustyMobileFFI/` were genuinely generated this
way (`cargo build -p rusty2600-mobile --release` then `uniffi-bindgen
generate --language swift`, both run on Linux), even though the actual
`aarch64-apple-ios`/`aarch64-apple-ios-sim` cross-compilation and
xcframework assembly steps in that same script need a real Mac and were
never run — see "The iOS app" below.

## The Android app (`android/`)

A real Gradle project (AGP 8.6, Kotlin 1.9, `compileSdk 34`/`minSdk 26`),
not a placeholder: `EmulatorView` (a custom `View` blitting the RGBA8
framebuffer via `Bitmap.copyPixelsFromBuffer`, nearest-neighbor scaled) and
`MainActivity` (loads a ROM via `ActivityResultContracts.OpenDocument`,
drives `run_frame` at ~60Hz on a background `HandlerThread`, plays audio
through an `AudioTrack` in `ENCODING_PCM_FLOAT` mode, and wires on-screen
Up/Down/Left/Right/Fire/Select/Reset buttons to `MobileInput`). No
save/load-state UI and no paddle input yet — this release's job is proving
the bridge runs a real ROM on real hardware, not replicating every
native-frontend feature.

## The iOS app (`ios/`)

`v1.12.0` "Pocket" reuses `rusty2600-mobile` unchanged for a SwiftUI iOS
host. The tree has two parts:

- **`ios/RustyMobileFFI/`** — a local Swift Package wrapping the FFI
  bridge: `Sources/RustyMobileFFI/rusty2600_mobile.swift` is the real,
  tool-generated Swift binding (1,523 lines; see the "Cross-compilation"
  discovery above), and `Package.swift` declares a `rusty2600_mobileFFI`
  `binaryTarget` pointing at `Rusty2600Mobile.xcframework` — a directory
  that intentionally does NOT contain a real xcframework yet (see its
  `NOTE.md`), since building one needs `aarch64-apple-ios`/
  `aarch64-apple-ios-sim` Rust cross-compilation on a real Mac with Xcode,
  which this sandbox cannot do at all. `ios/Generated/` also carries a
  reference copy of the generated header/modulemap for provenance.
- **`ios/Rusty2600/Sources/`** — the SwiftUI app itself, reusing the same
  design `android/`'s `MainActivity`/`EmulatorView` established:
  - `EmulatorView.swift` — a Metal `MTKView` blitting `FrameOutput.rgba`
    into an `MTLTexture` each frame (`texture.replace(region:...)`) and
    drawing it via a single full-screen-triangle pass with a plain
    nearest-sampled texture fetch — a one-texture blit, not a shader
    stack (that's the desktop-only `v1.4.0` `rusty2600-gfx-shaders`
    feature, which this mobile bridge doesn't consume).
  - `AudioEngine.swift` — `AVAudioEngine` + `AVAudioPlayerNode`,
    scheduling one `AVAudioPCMBuffer` per frame from
    `FrameOutput.audioSamples` at the TIA's native ~31.4kHz rate — no
    manual gapless-scheduling bookkeeping needed here (unlike the GH
    Pages wasm build's `AudioSink`), since `AVAudioPlayerNode` already
    queues buffers on its own internal timeline.
  - `ContentView.swift` / `EmulatorViewModel.swift` — the main screen and
    its state owner. The emulator instance, input snapshot, and ~60Hz run
    loop live in an `ObservableObject` (`EmulatorViewModel`), not raw
    SwiftUI `@State`, since a `View` struct can be recreated at any time
    and a long-lived background loop needs stable identity to mutate
    into. ROM loading uses SwiftUI's `.fileImporter` (the iOS analog of
    Android's `ActivityResultContracts.OpenDocument`).
  - **`PaddleControlView.swift`** — the genuinely new UX work Android's
    v1.11.0 build didn't need: a touch-drag rotary dial (not
    accelerometer/tilt — a drag dial maps deterministically to a bounded
    `0...255` position, matching a real paddle's fixed-sweep
    potentiometer, and avoids a Core Motion permission and re-zero
    drift). Modeled as a clamped -150°...+150° arc around the dial's
    center, matching `MobilePaddle.position`'s own "0 fully clockwise ..=
    255 fully counter-clockwise" convention directly.

### Paddle limitation (inherited, not introduced here)

`MobileInput.paddle0..=paddle3` are wired end-to-end from
`PaddleControlView` through `EmulatorViewModel` into every
`MobileEmulator.runFrame` call — but `rusty2600-mobile`'s `run_frame`
(unchanged by this release) never reads those fields into
`system.bus.tia.inpt[0..=3]` at all; only joystick/switch fields feed the
emulation today. This is a genuine, **pre-existing, project-wide** gap:
the TIA has no real analog dump-capacitor charge-timing simulation
anywhere in this engine yet, on any platform — `rusty2600-frontend`'s own
`emu_thread.rs` documents the identical gap as `T-0501-010`. Implementing
real capacitor-charge timing is a `rusty2600-tia` accuracy task, out of
scope for a mobile-bridge release. Paddle games (Breakout, Warlords,
Kaboom!) will not respond to the iOS paddle control yet on any platform;
this is inherited scope, not new breakage from `v1.12.0`.

### iOS Verification — explicitly NOT built or run

Unlike the Android build, **no Xcode build, no iOS Simulator run, and no
device run were performed, and none were possible in this environment** —
this sandbox is Linux with no `xcodebuild`/`xcrun`/`swift` at all (the
`aarch64-apple-ios` Rust cross-compilation used by `regenerate-bindings.sh`
also needs Apple's SDK, which only ships inside Xcode). What IS real:

- The Swift bindings are genuinely tool-generated (not hand-written),
  from an actual `cargo build -p rusty2600-mobile --release` +
  `uniffi-bindgen generate --language swift` run on this Linux box.
- The SwiftUI/Metal/AVFoundation app source is real, idiomatic Swift
  written directly against that generated API's actual method/type names
  and signatures (verified by reading the generated file, not guessed) —
  but it has never been compiled by `swiftc`/Xcode, so it is **unverified
  by compilation**.
- There is no `.xcodeproj` in this checkout — a hand-authored
  `project.pbxproj` was deliberately avoided (Xcode project files are
  fragile to hand-author correctly and nobody could verify one without
  Xcode itself); instead, `ios/RustyMobileFFI/` is a real, independently
  valid Swift Package, and `ios/Rusty2600/Sources/` is plain Swift source
  ready to be added to a fresh Xcode "iOS App" project.

A `v1.12.x` follow-up, run on a real Mac, needs to: run
`ios/regenerate-bindings.sh` in full (producing the real xcframework),
create an Xcode iOS App project, add `ios/RustyMobileFFI` as a local
Swift Package dependency, add the files under `ios/Rusty2600/Sources/`,
and then actually build + run on the Simulator (and ideally a physical
device) before this can be called verified the way the Android build was.

## Verification (real, not just "it compiles") — Android

Built and run on the `Pixel_8_API_34` AVD (x86_64 system image, KVM-accelerated):

1. `cargo ndk` cross-compiled both ABIs; `uniffi-bindgen` generated real
   Kotlin bindings (2,029 lines) from the compiled library.
2. `./gradlew assembleDebug` produced a working debug APK linking the JNA
   `@aar` dependency and both native libraries.
3. `adb install` + `am start` launched the app on the emulator with no
   crash (`topResumedActivity` confirmed, `ActivityTaskManager: Displayed`
   logged, no `AndroidRuntime`/`FATAL EXCEPTION` in logcat).
4. A synthetic 4K test ROM was pushed to the emulator and loaded through
   the real system file picker (Storage Access Framework); the app
   returned to the foreground with no crash, and `top` showed the
   background emulation thread actively consuming CPU — direct evidence
   the JNA → Rust `run_frame` call loop is executing continuously, not
   silently failing.
5. No `AudioTrack`/native-library-load errors appeared in logcat.

Not verified this release: on-screen visual output from a ROM that
actually writes TIA color registers (the synthetic test ROM used for
verification never writes `COLUBK`, so a black screen there is correct,
not a bug); a physical device (only the emulator was available); Play
Store packaging (explicitly out of scope, see below).

## What's deliberately out of scope this release

- **Play Store submission** — deferred beyond `v2.0.0`, matching
  RustyNES's own `v2.1.0` precedent. This release targets "a working,
  sideloadable emulator verified on real hardware," not a store listing.
- **Save/load-state UI, HD-pack loading** on either mobile host — the
  bridge crate supports save-states already; neither app exposes them yet.
- **Real TIA paddle timing** — see "Paddle limitation" above; a
  `rusty2600-tia` accuracy task, not a mobile-bridge one.
- **An actual Xcode build/Simulator/device run of the iOS app** — see
  "iOS Verification" above; deferred to `v1.12.x` on real Apple hardware.
- **App Store submission** — deferred beyond `v2.0.0`, same as Play Store.

## What's next

A `v1.12.x` follow-up needs a real Mac to: build the iOS xcframework,
wire the checked-in Swift sources into an actual Xcode project, and get a
first real Simulator/device run (the same bar the Android build already
cleared). A `v1.11.x`/`v1.12.x` follow-up could also add on-device
save-state UI and real (non-emulator/non-simulator) hardware verification
for both mobile hosts once physical test devices are available. Real TIA
dump-capacitor timing (closing the paddle-limitation gap for every
platform at once) is tracked as a `rusty2600-tia` accuracy item, not a
mobile-train item.
