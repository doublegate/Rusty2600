# Contributing

Thanks for your interest in contributing to Rusty2600, a cycle-accurate Atari
2600 / VCS emulator written in pure Rust.

## Development setup

- Install [rustup](https://rustup.rs).
- The toolchain is pinned in `rust-toolchain.toml` (Rust 1.96); `rustup`
  auto-installs it, including the `wasm32-unknown-unknown` and
  `thumbv7em-none-eabihf` targets.
- The `winit` + `wgpu` + `cpal` frontend needs the platform graphics/audio
  libraries. On Debian/Ubuntu:
  `sudo apt-get install -y libxkbcommon-dev libwayland-dev libxkbcommon-x11-dev libasound2-dev libudev-dev`.
  On Arch / CachyOS:
  `sudo pacman -S --needed libxkbcommon wayland alsa-lib systemd-libs`.
- `cargo check --workspace` to verify the workspace compiles.
- `cargo test --workspace` to run the unit and integration tests.

## Workflow

1. Pick a ticket from `to-dos/` (or open an issue first if your work
   isn't already represented there).
2. Create a branch: `<type>/<short-description>` (e.g.,
   `feat/cpu-immediate-addressing`, `fix/tia-hmove-comb`).
3. Make changes. Keep commits focused.
4. Run the local quality gate before pushing.
5. Open a PR. Reference the ticket(s) and any relevant `docs/` files.

## Quality gate

Before opening a PR, ensure:

- [ ] `cargo fmt --all --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] `cargo build -p rusty2600-core --target thumbv7em-none-eabihf --no-default-features`
      compiles (the chip stack must stay `no_std`)
- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` is clean
- [ ] New public items have rustdoc

A chip-behavior change touches both the chip code and the chip's
`docs/<subsystem>.md`; keep them in sync in the same PR.

## Documentation expectations

- New subsystems get a doc in `docs/`.
- Architecture-affecting changes update `docs/architecture.md`.
- User-visible changes are noted in `CHANGELOG.md` under `[Unreleased]`.
- Ticket completion is reflected in the relevant `to-dos/` sprint file.

## Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org):
`<type>(<scope>): <subject>`.

Types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `perf`, `build`,
`ci`. Keep the imperative subject at or under 72 characters; add a blank line
and a body explaining the *why* (not the *what* — the diff shows the what).

No emojis in code, comments, or commits (project policy).

## Code review

- One reviewer minimum; two for changes to `docs/architecture.md` or
  cross-subsystem refactors.
- Reviewers focus on correctness, design, and adherence to the relevant
  `docs/` specification.
- Discussion is preferred over deferral; if a comment can't be resolved
  in review, file a follow-up ticket explicitly.

## Test ROM legalities

Never commit commercial Atari ROMs. Only CC0 / public-domain test ROMs and
their reference screenshots are committed; your own commercial dumps live in
the gitignored `tests/roms/external/`.
