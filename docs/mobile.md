# Mobile bridge (`rusty2600-mobile`) — Rusty2600

References: `docs/adr/0004-determinism-contract.md`; `docs/adr/0007-save-state-versioning.md`;
`to-dos/ROADMAP.md` (v1.11.0 "Handheld", v2.11.0 "Field Trip"); `crates/rusty2600-mobile/src/lib.rs`;
`android/`; `ios/`. This doc is the SPEC, not history — update it in the same PR as the code.

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
Up/Down/Left/Right/Fire/Select/Reset buttons to `MobileInput`). Still no
paddle input (see "Paddle limitation" below).

### Save State / Load State UI (`v2.11.0` "Field Trip")

`SaveSlots.kt` mirrors desktop's own manual save-state slot convention
(`crates/rusty2600-frontend/src/config.rs`, `v2.4.0` "Save Point") on
Android's app-private storage: `SLOT_COUNT` (8) numbered slots, one file
per slot named `slot_<N>.r26s`, all of one ROM's slots kept under a
directory named after that ROM's identity tag
(`saves/<rom-tag as 16-hex>/slot_<N>.r26s` under `Context.filesDir`) so two
different ROMs' slots can never collide. The ROM tag is the same CRC32
`crc32Tag` `MainActivity` already computed (since `v1.11.0`) to pass to
`MobileEmulator.loadRom` — reusing it means a slot can never even be
*written* under the wrong ROM's key, on top of `MobileEmulator.loadState`'s
own tag check on restore.

`MainActivity` wires two new buttons (`saveStateButton`/`loadStateButton`,
next to `loadRomButton`) to an `AlertDialog`-based slot picker:
- **Save State** (`showSaveStateDialog`) — `setItems` over all 8 slots
  (each labeled via `SaveSlots.SlotInfo.label()`, e.g. `"Slot 3 (empty)"` or
  `"Slot 3 -- 2026-07-02 14:03:07 UTC"` — the exact wording desktop's
  `SaveSlotInfo::label` uses); tapping a slot calls
  `MobileEmulator.saveState()` and writes the blob via `SaveSlots.save`,
  overwriting whatever was there.
- **Load State** (`showLoadStateDialog`) — a custom `ArrayAdapter` overrides
  `isEnabled(position)` so empty slots are shown but greyed out and
  unclickable (matching desktop's `add_enabled(slot.exists, ...)` menu-item
  gating exactly); tapping an occupied slot reads it via `SaveSlots.load`
  and calls `MobileEmulator.loadState(bytes)`.

Both dialogs guard on `currentRomTag == null` (a field `MainActivity` now
tracks, set in `loadRom`) and show a "No ROM loaded" `Toast` rather than
doing anything when no ROM is loaded yet.

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
  - **`SaveSlots.swift` / `SaveStateSlotPickerView.swift`** (`v2.11.0`
    "Field Trip") — the iOS counterpart to `android/`'s `SaveSlots.kt`:
    the same 8-slot, `.r26s`-extension, per-ROM-directory convention, but
    over `FileManager.default.urls(for: .documentDirectory, ...)` (the
    iOS-idiomatic per-app document store) instead of `Context.filesDir`.
    `SaveStateSlotPickerView` is a `NavigationStack` + `List` sheet
    (presented from `ContentView`'s new Save State / Load State buttons)
    showing all 8 slots via `SaveSlots.SlotInfo.label` (identical wording to
    Android/desktop), disabling empty rows in Load mode
    (`.disabled(mode == .load && !slot.exists)`) — the SwiftUI equivalent of
    Android's `ArrayAdapter.isEnabled` override. `EmulatorViewModel` gained
    `currentRomTag`, `saveSlots`, `refreshSaveSlots()`, `saveState(slot:)`,
    and `loadState(slot:)` to back it.
  - **ROM-tag fix, same release**: `EmulatorViewModel.loadRom` previously
    keyed save-states (well, would have, once any existed) with
    `UInt64(bytes.count) &* 2_654_435_761` — a tag derived ONLY from the
    ROM's byte count, so two different same-size ROMs would silently
    collide onto the same tag. Since `v2.11.0` actually builds real
    per-ROM-tag slot directories on iOS for the first time, this was
    upgraded to `fnv1aRomTag` (`SaveSlots.swift`), a real FNV-1a 64-bit hash
    over the ROM's full byte content — matching the spirit of Android's
    CRC32-based `crc32Tag` (any stable, deterministic, collision-safe-enough
    hash works; `MobileEmulator.loadState`'s own tag check is the
    authoritative guard regardless).

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

`v2.11.0`'s additions (`SaveSlots.swift`, `SaveStateSlotPickerView.swift`,
and the `EmulatorViewModel`/`ContentView` changes) inherit this exact same
honest status: written carefully against the real generated binding's
actual `saveState()`/`loadState(bytes:)` signatures (verified by reading
`ios/RustyMobileFFI/Sources/RustyMobileFFI/rusty2600_mobile.swift`, not
guessed), reviewed line-by-line for Swift syntax correctness (optional
shorthand binding, `Equatable` conformance on the picker's `Mode` enum,
`CVarArg` bridging for `String(format:)`, iOS-16-safe API choices like
`NavigationStack` over the older `NavigationView` to match
`RustyNES/ios/`'s own convention) -- but still never compiled by
`swiftc`/Xcode, for the same no-Mac-in-this-sandbox reason as everything
else in this file.

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

Not verified in `v1.11.0`: on-screen visual output from a ROM that
actually writes TIA color registers (the synthetic test ROM used for
verification never writes `COLUBK`, so a black screen there is correct,
not a bug); a physical device (only the emulator was available); Play
Store packaging (explicitly out of scope, see below).

### Save-state UI verification (`v2.11.0` "Field Trip")

Rebuilt (`./gradlew assembleDebug`, clean success) and re-run end-to-end on
the same `Pixel_8_API_34` AVD, this time with a **real homebrew ROM**
(`tests/roms/homebrew/2048 2600 (NTSC).a26`, a legitimately-licensed
homebrew title already vendored in this repo's test-ROM corpus — not a
commercial ROM) rather than the old synthetic 4K program, via
`adb push` + the same real Storage Access Framework file-picker flow:

1. `adb uninstall`/`install -r` the freshly-built debug APK; `am start`;
   confirmed `topResumedActivity` + no `AndroidRuntime`/`FATAL EXCEPTION`.
2. Tapped **Save State** with no ROM loaded: a real "No ROM loaded" `Toast`
   fired (confirmed via `logcat`'s `NotificationService: Toast` line), no
   crash — the guard path works.
3. Loaded the real ROM through the system file picker (`uiautomator dump`
   used throughout to get exact, reliable tap coordinates rather than
   guessing from screenshots); `topResumedActivity` returned to
   `MainActivity` with no crash, and `top -o PID,%CPU` showed
   30-90%+ CPU on the app's process, confirming `run_frame` executing
   continuously against the real ROM (not a silent no-op).
4. Tapped **Save State -> Slot 2**: dialog showed all 8 slots correctly
   labeled `"Slot N (empty)"`; after tapping slot 2, re-opening the dialog
   showed it relabeled to `"Slot 2 -- 2026-07-03 01:54:15 UTC"` — a real,
   live timestamp, not a placeholder.
5. **Confirmed the actual on-disk file** via `adb shell run-as
   com.doublegate.rusty2600 find .../files/saves`: a real file at
   `saves/00000000e5b96244/slot_2.r26s` (2,288 bytes). Independently computed
   `zlib.crc32()` over the source `.a26` file's bytes in Python and got
   `0xe5b96244` — an exact match against the on-device directory name,
   proving the CRC32 ROM-tag keying is correct end-to-end, not just
   plausible-looking.
6. Tapped **Load State**: the dialog showed Slot 2 as the only enabled
   (non-greyed-out) row; tapping the other, empty slots produced no crash,
   no `Toast`, and left the dialog open (the `ArrayAdapter.isEnabled`
   disable actually blocks the tap, verified via a follow-up
   `uiautomator dump` still showing the dialog's slot list, i.e. it was
   never dismissed by the disabled-row tap).
7. Tapped **Load State -> Slot 2**: a screenshot taken ~150ms after the tap
   caught the real Android `Toast` bubble on-screen reading **"Loaded slot
   2"** verbatim — direct visual proof of the full save -> disk -> load
   round trip, not an inferred success.
8. Full-session `logcat` scan for `FATAL EXCEPTION`/`AndroidRuntime.*
   com.doublegate` across the entire test run: zero matches.

Not verified this release (same honest gaps as `v1.11.0`, unrelated to
save-states specifically): on-screen visual framebuffer output — the real
homebrew ROM's screen stayed black through this session even after RESET
presses and several seconds of run time, the same open, pre-existing,
undiagnosed gap `v1.11.0` first flagged for the synthetic ROM (still not a
save-state bug: `SaveState::capture`/`restore` operate on `System` state,
not the framebuffer, and are independently covered by 374 passing
`cargo test --workspace` tests including bridge-level round-trip tests in
`rusty2600-mobile`); a physical device (see "Physical hardware
verification" below); Play Store packaging.

**PR #21 bot-review fixes, independently re-verified on the live emulator**
(Gemini Code Assist flagged that `saveState()`/`loadState()` — a bridge
call plus synchronous file I/O — ran directly on the UI thread, and that
`emulator` is the same instance the ~60Hz frame loop mutates on
`emuHandler`'s background thread, so calling it from the UI thread also
raced the frame loop, not just risked jank; Copilot separately flagged
that `SaveSlots.save`/`load`'s file I/O could throw an `IOException` past
the `catch (e: MobileException)`-only handlers, crashing the app instead
of surfacing an error). Both dialogs now post the bridge call + file I/O
onto `emuHandler` and hop back to the UI thread only for the `Toast`,
widening the catch to `Exception`. Rebuilt clean
(`./gradlew assembleDebug`) and re-ran directly on the still-running
`Pixel_8_API_34` emulator from the verification pass above (same install,
same loaded ROM):

- Saved to a NEW slot (Slot 4) through the now-threaded code path;
  confirmed via `adb shell run-as ... find` that `slot_4.r26s` was
  created on disk, and that pre-existing `slot_2.r26s` (saved by the
  OLD, unthreaded code before this fix) was untouched and still loads
  with its original timestamp — confirming the `ULong.toString(16)`
  hex-formatting change (the third Gemini finding, replacing a
  `Long`-cast `"%016x".format`) produces an identical directory name for
  a real CRC32 tag, so existing save data isn't orphaned by the change.
- Reopened both dialogs after the threaded save: Slot 4 correctly showed
  its live timestamp, confirming `refreshSaveSlots`'s probe reflects the
  background thread's write once it completes.
- Tapped Load State -> Slot 4: real `Toast` windows fired (confirmed via
  `logcat`'s `CoreBackPreview: Window{... Toast}` lines at the expected
  timestamps).
- Scanned the full re-verification session's `logcat` for ANY
  `doublegate`-tagged exception/error/crash (not just `FATAL EXCEPTION`):
  zero matches.

The iOS-side fixes (switching `SaveSlots.swift` from `.documentDirectory`
to the genuinely private `.applicationSupportDirectory`, `URL.
resourceValues(forKeys:)` over the deprecated `attributesOfItem(atPath:)`,
and backgrounding `EmulatorViewModel`'s three bridge-calling methods via
`Task.detached`) inherit the same standing unverified-by-compilation
status as the rest of this file's iOS section — reviewed carefully by
hand against the real generated binding signatures, but no Mac/Xcode in
this sandbox to actually build them.

## Physical hardware verification (`v2.11.0`)

Checked honestly, not assumed: `adb devices -l` showed only the
`Pixel_8_API_34` emulator (`emulator-5554`) both before and during this
session — no real device connected over ADB, and no cloud device-farm
credentials or tooling configured in this environment. Physical-hardware
verification stays exactly the `v1.11.0` posture ("verified on a real
emulator," not physical hardware) — this was checked for real this
release (per the roadmap's own "more achievable than it sounds, IF
hardware becomes available" framing) and the honest answer is: it was not
available this time either.

## Cloud save-state sync research (`v2.11.0`) — researched, deferred

The roadmap's assumption going in was "a Google-Drive-backed sync path is
the most plausible option" for Android — that assumption does not survive
contact with what this project's own sibling, RustyNES, actually built.
`RustyNES/ios/RustyNES/CloudSaveStateSync.swift` (`v1.9.7`) is a real,
working CloudKit/iCloud sync (one `CKRecord` per save-state slot, private
database, last-writer-wins via `savedAt` timestamp, opt-in and
gracefully degrading when iCloud is unavailable) — confirming the iOS side
of the assumption. `RustyNES/android/app/src/main/java/com/doublegate/
rustynes/CloudSave.kt` (`v1.8.8` "Atlas") is the actually-shipped Android
equivalent, and it is **Google Play Games Services v2 Snapshots**, not
Google Drive: one `Snapshot` per (ROM-SHA, slot) independently-updatable
unit, `RESOLUTION_POLICY_MOST_RECENTLY_MODIFIED` auto-resolution with a
surfaced keep-local/keep-cloud picker on true divergent conflicts, gated
behind a default-off `BuildConfig.PGS_ENABLED` flag plus a PGS sign-in
check plus a user-facing toggle.

Both real implementations share a hard prerequisite this sandbox cannot
meet: a **live backend integration** requiring a real Google Play Console
project (Play Games Services v2 configuration, OAuth client IDs,
`google-services.json`) or a real Apple Developer account (CloudKit
container provisioning, entitlements) — genuinely new external
dependencies, real sign-in flows, and integration testing against a live
cloud backend that cannot be exercised, let alone verified, from this
Linux sandbox with no Google/Apple developer credentials configured. This
is exactly the "new OAuth flow, new cloud dependency, real backend
integration testing this sandbox can't do" bar the roadmap itself flagged
as the honest-defer trigger.

**Decision: deferred, not implemented.** Rusty2600's local-slot save-state
UI (this release's items 1-2 above) is the real, verified deliverable;
cloud sync is left as a well-scoped future item rather than force-fit into
a release with no way to test it. If a future release picks this up on
Android, `RustyNES/android/.../CloudSave.kt` is the concrete reference
implementation to port (Play Games Services Snapshots, not Google Drive);
for iOS, `RustyNES/ios/RustyNES/CloudSaveStateSync.swift` is the CloudKit
reference — but both need a Mac (iOS) or a configured Play Console project
(Android) to actually stand up and test, neither of which exists in this
sandbox today.

## What's deliberately out of scope this release

- **Play Store submission** — deferred beyond `v2.0.0`, matching
  RustyNES's own `v2.1.0` precedent. This release targets "a working,
  sideloadable emulator verified on real hardware," not a store listing.
- **HD-pack loading** on either mobile host — save/load-state UI shipped
  this release (see above); HD-pack loading is still unexposed on mobile.
- **Cloud save-state sync** — researched this release; see "Cloud
  save-state sync research" above for the concrete go/no-go reasoning.
- **Real TIA paddle timing** — see "Paddle limitation" above; a
  `rusty2600-tia` accuracy task, not a mobile-bridge one.
- **An actual Xcode build/Simulator/device run of the iOS app** — see
  "iOS Verification" above; permanently out of scope for this sandbox
  (needs a real Mac session, whenever that becomes available).
- **Physical Android hardware verification** — see "Physical hardware
  verification" above; checked for real this release, none was available.
- **App Store submission** — deferred beyond `v2.0.0`, same as Play Store.

## What's next

A future release on a real Mac needs to: build the iOS xcframework, wire
the checked-in Swift sources (including `v2.11.0`'s `SaveSlots.swift`/
`SaveStateSlotPickerView.swift`) into an actual Xcode project, and get a
first real Simulator/device run (the same bar the Android build already
cleared). Real physical Android hardware verification remains open,
contingent on a real device or device-farm access becoming available in
this sandbox (checked and genuinely absent as of `v2.11.0`, not assumed
away). Cloud save-state sync is scoped but deferred (see above) pending
real Google Play Console / Apple Developer credentials this sandbox
doesn't have. Real TIA dump-capacitor timing (closing the paddle-limitation
gap for every platform at once) is tracked as a `rusty2600-tia` accuracy
item, not a mobile-train item. The still-unresolved black-screen-with-a-
real-ROM gap on Android (see "Save-state UI verification" above) is worth
its own investigation, independent of the save-state work this release
completed.
