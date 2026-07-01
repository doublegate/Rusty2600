#!/usr/bin/env bash
# Runs the local CI-parity gate for Rusty2600, mirroring the GitHub Actions
# CI matrix. Stops on the first failing step so the error is easy to spot.
set -u

cd "$(git rev-parse --show-toplevel 2>/dev/null || echo .)" || exit 1

steps=(
  "fmt|cargo fmt --all --check"
  "check|cargo check --workspace"
  "test|cargo test --workspace"
  "test-roms|cargo test --workspace --features test-roms"
  "clippy|cargo clippy --workspace --all-targets -- -D warnings"
  "doc|RUSTDOCFLAGS=-D\\ warnings cargo doc --workspace --no-deps"
  "no_std|cargo build -p rusty2600-core --target thumbv7em-none-eabihf --no-default-features"
)

passed=()
for step in "${steps[@]}"; do
  name="${step%%|*}"
  cmd="${step#*|}"
  echo "==> [$name] $cmd"
  if eval "$cmd"; then
    passed+=("$name")
    echo "    [PASS] $name"
  else
    code=$?
    echo
    echo "GATE FAILED at step: $name"
    echo "Command: $cmd"
    echo "Exit code: $code"
    echo "Passed before failure: ${passed[*]:-none}"
    exit "$code"
  fi
  echo
done

echo "ALL GATES PASSED: ${passed[*]}"
