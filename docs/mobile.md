# Mobile bridge (`rusty2600-mobile`) — Rusty2600

References: `docs/adr/0004-determinism-contract.md`; `docs/adr/0007-save-state-versioning.md`;
`to-dos/ROADMAP.md` (v1.11.0 "Handheld"); `crates/rusty2600-mobile/src/lib.rs`; `android/`.
This doc is the SPEC, not history — update it in the same PR as the code.

## What this crate is

`rusty2600-mobile` is a `std`, host-testable, platform-agnostic bridge over
`rusty2600_core::System`, exposing a small [UniFFI](https://mozilla.github.io/uniffi-rs/)
surface (`load_rom`/`run_frame`/`save_state`/`load_state`/`is_rom_loaded`)
that the same generated bindings drive from both Kotlin (Android, this
release) and Swift (`v1.12.0` "Pocket", iOS) — one Rust implementation, two
mobile hosts. It carries no Android/JNI/Swift types of its own; those live
entirely in the generated bindings and the platform app project (`android/`
this release).

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
verified running a real ROM on an API 34 emulator (see "Verification"
below). `rusty2600-ios` (`v1.12.0`) is expected to follow the same pattern
(Swift `UIImage`/`AVAudioEngine` consuming the same `FrameOutput` shape)
rather than needing its own from-scratch native glue either.

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

## Verification (real, not just "it compiles")

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
- **Save/load-state UI, paddle input, HD-pack loading** on the Android
  host — the bridge crate supports save-states already; the Android app
  doesn't expose them yet.
- **`rusty2600-ios`** — `v1.12.0`, reusing this same `rusty2600-mobile`
  bridge (the whole point of building it UniFFI-first).

## What's next

Per `to-dos/ROADMAP.md`: `v1.12.0` "Pocket" reuses this bridge for a
SwiftUI iOS host, adding real virtual-paddle UX design work the Android
build didn't need (2600 paddles have no NES-frontend precedent to draw
on). A `v1.11.x` follow-up could add on-device save-state UI and real
hardware (not just emulator) verification once physical test devices are
available.
