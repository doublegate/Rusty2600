# Phase 5 — Frontend

**Goal:** the always-on `winit + wgpu + cpal + egui` shell drives the core to a
visible, playable window — beam-raced TIA framebuffer presented under a brief
lock, joystick / paddle / console-switch input late-latched into the core, the
6507 / TIA / RIOT / memory debugger panels live, plus the frontend-owned
save-state / rewind / run-ahead orchestration and the wasm32 build. The shell
scaffold (the window loop, the gfx blit, the egui pass, the config / CLI / help
TUI) already compiles at v0.1; this phase fills the TODO bodies once the chips
emit real pixels and audio.

References: `docs/frontend.md`; `docs/adr/0004` (determinism — rate control +
run-ahead live HERE, never in the core); `crates/rusty2600-frontend/src/`
(`app.rs`, `gfx.rs`, `ui_shell.rs`/`shell.rs`, `emu_thread.rs`, `present_buffer.rs`,
`audio_ring.rs`, `input.rs`, `palette.rs`, `config.rs`, `cli.rs`); `ref-docs/
research-report.md` §6 / §9.

## Scope

In: the present path (copy framebuffer under brief lock, wgpu blit, egui pass,
present; never hold the emu lock inside the egui closure); the beam-raced display
buffer accumulation (TIA dot to region palette RGB to RGBA8); the 2600 input map
(joystick SWCHA/INPT4-5, paddles INPT0-3 analog, the console switches Select /
Reset / Color-B&W / Left & Right difficulty on SWCHB); the debugger panels (6507
regs, TIA object regs + beam position, RIOT timer + ports, memory view); the
audio ring + dynamic rate control + run-ahead (snapshot/restore); save-states +
rewind keyframe cache; the wasm32 build (winit + a lightweight canvas embed);
the `Board: Send` change that re-enables the dedicated `emu-thread`.

Out: netplay / RetroAchievements / TAS / Lua / shader ecosystem — Phase 8; the
deep post-process chain (CRT / NTSC filter / upscalers) — Phase 8.

## Exit criteria (verifiable)

- `cargo run -p rusty2600-frontend -- rom.a26` opens a window and presents the
  TIA beam-raced frame (not just a cleared buffer) with the region palette.
- Joystick + paddle + all four console switches reach the core (input
  round-trips through `SharedInput` and is visible in the RIOT panel).
- The debugger panels read live 6507 / TIA / RIOT state under the brief lock.
- A save-state round-trips bit-identically (the determinism contract holds); a
  rewind buffer replays without desync; run-ahead reduces latency by ≥1 frame.
- The wasm32 frontend builds (`trunk build --release`) and runs in a browser.
- The pacer holds the region frame rate (NTSC 60.0988 / PAL 50.00698 Hz) without
  audio underruns.

## Sprints

- Sprint 1 — present path + input + debugger panels → `sprint-1-shell-and-input.md`
  (`T-0501-NNN`).
- Sprint 2 — audio ring + rate control + save-state / rewind / run-ahead →
  `sprint-2-pacing-and-state.md` (`T-0502-NNN`).
- Sprint 3 — wasm32 build + `Board: Send` / `emu-thread` → `sprint-3-wasm-thread.md`
  (`T-0503-NNN`).

## Risks

- Holding the emu lock inside the egui closure deadlocks / stutters — the
  brief-lock-then-drop discipline (RustyNES `docs/frontend.md`) is load-bearing
  and easy to regress.
- Leaking wall-clock time / OS RNG into the core via the frontend breaks the
  determinism contract (ADR 0004); rate control + run-ahead must stay in the
  resampler / snapshot orchestration only.
- The paddle is analog (INPT0-3 dumped-capacitor timing) — mapping a digital
  axis to the discharge curve is a fidelity trap; pin against a paddle test ROM.
- wasm has no threads / rfd / gilrs — the `cfg(target_arch = "wasm32")` gating
  must hold or the wasm build breaks.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
