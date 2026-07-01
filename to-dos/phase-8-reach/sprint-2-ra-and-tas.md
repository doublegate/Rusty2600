# Sprint 8.2 — RetroAchievements + TAS Movies

**Context:** Part of Phase 8 — Reach. RetroAchievements pulled forward into
the v1.0.0 gate per `to-dos/ROADMAP.md`'s "Release line" (v0.7.0 "Cheevos").
TAS movies remain out of scope for v1.0.0 (Beyond-v1.0, unchanged).

## Tickets (`T-0802-NNN`)

- [x] `T-0802-001` (DONE — v0.7.0): Vendor the RetroAchievements `rcheevos`
  C library (MIT) into `crates/rusty2600-cheevos/vendor/rcheevos/` — the same
  50-file trimmed subset RustyNES's own `rustynes-cheevos` vendors (console-
  agnostic: the condition VM, runtime, rich presence, leaderboards, request/
  response codecs, and hashing are identical C source across consoles; only
  the memory-address mapping is console-specific). `build.rs` compiles it via
  `cc` into a static archive; the whole crate body is
  `#![cfg(not(target_arch = "wasm32"))]` so a wasm workspace build never
  needs a C toolchain.
- [x] `T-0802-002` (DONE — v0.7.0): `rusty2600_cheevos::memory::ra_addr_to_riot`
  — the RetroAchievements-flat -> 2600 CPU-bus address map. Far simpler than
  a typical console: the 2600's ONLY RAM is the RIOT's 128 bytes, so RA
  addresses `0x00..=0x7F` map directly to CPU bus `$0080..=$00FF` (no
  cartridge-WRAM-window split the way NES needs). Pure, unit-tested.
- [x] `T-0802-003` (DONE — v0.7.0): `RaClient` — the safe wrapper around
  `rc_client_t`, adapted from `rustynes-cheevos`'s own (console-agnostic
  except for one `RC_CONSOLE_ATARI_2600` constant and the memory map above):
  owns the client, drives per-frame achievement processing
  (`do_frame`/`idle`/`reset`), login/load-game, progress (de)serialization,
  rich presence, and an off-thread HTTP worker for the RA API. `RaClient` is
  deliberately `!Send`/`!Sync` — verified via the crate's own smoke tests
  (`create_drive_destroy`, `nested_read_guard_restores`,
  `login_completion_fires_on_transport_error`), all passing, proving the
  static link, the C callback trampolines, and the thread-local ownership
  discipline are sound.
- [x] `T-0802-004` (DONE — v0.7.0): Wire `retroachievements` into
  `rusty2600-frontend`: `cheevos.rs` owns a `CheevosState` (wrapping
  `RaClient`) on the winit/main thread — NOT inside `EmuCore`, since
  `RaClient`'s `!Send` bound is incompatible with `EmuCore`'s `Send`
  requirement (needed by the default-on `emu-thread` feature). Pumped once
  per frame under the SAME brief emu lock the present path and the debug
  snapshot already take (`app.rs`), peeking the bus via
  `|addr| bus.peek(addr)` rather than holding the lock any longer. ROM
  load/close hooks call `begin_load_game`/`unload_game`; a new Emulation ->
  RetroAchievements menu shows game-recognition status and a hardcore-mode
  toggle; achievement-unlock/server events surface as status-bar text.
- [ ] `T-0802-005` (not started): A dedicated achievement-list panel, a login
  dialog, and a rich-presence / unlock-toast HUD. The backend is real and
  events fire correctly today (surfaced as plain status-bar text); this
  ticket is the DEDICATED UI surface for them, deferred as a distinct,
  UI-heavy follow-up rather than folded into the initial wiring.
- [ ] `T-0802-006` (not started): TAS movies (record/play/branch + a
  deterministic movie format) and a TAStudio-style editor. Out of scope for
  v1.0.0 per the ROADMAP's non-requirements list; tracked here for
  Beyond-v1.0.

## Technical notes

`rusty2600-cheevos` is a direct lift-and-adapt of RustyNES's own
`rustynes-cheevos` (same author, same MIT-compatible license posture): the
vendored C library, the FFI bridge (`ffi.rs`), the event mirror
(`events.rs`), the HTTP worker (`http.rs`), and the C-string helpers
(`util.rs`) are ~95% identical since rcheevos itself doesn't care which
console it's tracking achievements for — only `memory.rs` (the RA-flat ->
bus-address map) and one console-ID constant differ. This mirrors the
project's own established "lift and adapt a sibling module" convention
(`crates/rusty2600-frontend/Cargo.toml`'s own comment about pinning versions
to RustyNES's for exactly this reason).

Rusty2600's workspace enforces `clippy::pedantic`/`clippy::nursery`/
`missing_docs` — RustyNES's own workspace does not, so the lifted files
needed real doc comments added to every public struct field/enum variant and
several mechanical clippy fixes (`cargo clippy --fix` resolved most; a
handful of `map_or_else` conversions, a `too_many_lines` split, and one
justified `#[allow(clippy::transmute_ptr_to_ptr)]` for a genuine lifetime-
erasing transmute were done by hand).

## Verification

- `cargo test -p rusty2600-cheevos` — 7 passed (2 `memory.rs` unit tests + 3
  `lib.rs` FFI smoke tests + 2 `http.rs`/`events.rs` unit tests), including
  real (not mocked) FFI calls into the vendored C library.
- `cargo clippy -p rusty2600-cheevos --all-targets -- -D warnings` — clean.
- `cargo test -p rusty2600-frontend --features retroachievements` — 43
  passed (same as the default build; the feature adds no new frontend tests
  yet, per `T-0802-005`'s deferred UI work).
- Compiles verified for native (`retroachievements` on and off) and
  `wasm32-unknown-unknown` (`--features wasm-winit,debug-hooks,
  retroachievements` — the crate itself compiles to empty on wasm32, so this
  proves the feature is a real no-op there, not something that needs its
  own wasm-specific exclusion).


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
