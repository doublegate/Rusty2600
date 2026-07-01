---
name: ci-gate
description: Run the exact local CI-parity gate for the Rusty2600 Rust workspace before a commit or PR — fmt check, cargo check, full test suite, the test-roms feature suite, clippy with -D warnings (never --all-features), doc build with -D warnings, and the no_std thumbv7em build. Mirrors the project's GitHub Actions CI matrix step-for-step. Use this whenever the user asks to "run CI locally", "check before I commit/push", "run the full gate", or wants to verify a change is ready for a PR in this repo, even if they don't name every individual command.
---

# Rusty2600 CI gate

Runs the same sequence of checks as GitHub Actions, in the same order, so a
failure here is a failure there too. Each step must pass before the next
runs — there's no value in running `clippy` on code that doesn't even build.

## Run it

```bash
bash .claude/skills/ci-gate/scripts/ci-gate.sh
```

The script:
1. `cargo fmt --all --check`
2. `cargo check --workspace`
3. `cargo test --workspace`
4. `cargo test --workspace --features test-roms`
5. `cargo clippy --workspace --all-targets -- -D warnings` (never add `--all-features` — this project's clippy job intentionally excludes it; per-feature jobs cover combinations instead)
6. `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
7. `cargo build -p rusty2600-core --target thumbv7em-none-eabihf --no-default-features` (the no_std gate)

## Reporting back to the user

- Stream each step's `==> [name] ...` / `[PASS] name` line as it runs so the
  user sees progress, not just a final verdict.
- On failure, the script prints which step failed, the exact command, the
  exit code, and which steps already passed — relay that summary verbatim,
  then show the actual compiler/test/clippy output above it so the user can
  see the real error, not just "clippy failed."
- On success, the script prints `ALL GATES PASSED: <step names>` — report
  that as a one-line green summary; don't pad it with extra commentary.
- If a step fails because a target/toolchain isn't installed (e.g. the
  `thumbv7em-none-eabihf` target), say so explicitly rather than treating it
  as a code bug — that's an environment gap (`rustup target add
  thumbv7em-none-eabihf`), not something to "fix" in source.
