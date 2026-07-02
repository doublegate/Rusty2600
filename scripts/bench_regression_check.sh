#!/usr/bin/env bash
# CI-gated performance-regression trip-wire for `rusty2600-core`'s
# whole-system frame bench (`crates/rusty2600-core/benches/frame_bench.rs`,
# the `system_full_ntsc_frame` function — one full NTSC frame, 262 lines x
# 228 color clocks, driven through the real CPU+TIA+RIOT+cart System, not a
# per-chip proxy).
#
# Runs the bench, reads Criterion's own JSON output for the measured mean,
# and fails if it exceeds a fixed ABSOLUTE ceiling (see CEILING_NS below).
#
# Deliberately absolute, not relative/percentage-based: CI runners have
# enough run-to-run timing variance (shared cores, noisy neighbors) that a
# relative "N% slower than last run" comparison would be unreliable — an
# absolute ceiling sized with real headroom above the measured baseline
# catches a genuine multi-x regression without flapping on noise.
set -u

cd "$(git rev-parse --show-toplevel 2>/dev/null || echo .)" || exit 1

# Measured baseline on a single unoptimized development-machine run
# (2026-07-02): ~1.2545 ms/frame (see docs/performance.md). The documented
# system-wide target is <=2 ms/frame. This ceiling is ~3x that measured
# baseline -- comfortable headroom for CI-runner noise while still catching
# any regression severe enough to matter (a 3x-or-worse slowdown is never
# just noise). Re-baseline this number (with a comment explaining why) if a
# deliberate, reviewed change legitimately moves the measured mean.
CEILING_NS=3750000

BENCH_NAME="system_full_ntsc_frame"
ESTIMATES_JSON="target/criterion/${BENCH_NAME}/new/estimates.json"

echo "==> Running cargo bench -p rusty2600-core --bench frame_bench"
if ! cargo bench -p rusty2600-core --bench frame_bench -- --output-format bencher >/tmp/bench_regression_check.out 2>&1; then
  cat /tmp/bench_regression_check.out
  echo
  echo "BENCH FAILED TO RUN"
  exit 1
fi
cat /tmp/bench_regression_check.out

if [ ! -f "$ESTIMATES_JSON" ]; then
  echo
  echo "REGRESSION CHECK FAILED: expected Criterion output at $ESTIMATES_JSON but it does not exist."
  echo "(Did the bench function name change? Update BENCH_NAME in this script to match.)"
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo
  echo "REGRESSION CHECK FAILED: jq is required to parse Criterion's JSON output but is not installed."
  exit 1
fi

mean_ns=$(jq '.mean.point_estimate' "$ESTIMATES_JSON")
if [ -z "$mean_ns" ] || [ "$mean_ns" = "null" ]; then
  echo
  echo "REGRESSION CHECK FAILED: could not read .mean.point_estimate from $ESTIMATES_JSON"
  exit 1
fi

# Integer comparison (Criterion's JSON gives a float; truncate for a plain
# bash arithmetic comparison -- more than precise enough for a ceiling check).
mean_ns_int=${mean_ns%%.*}

echo
echo "Measured mean: ${mean_ns_int} ns/frame"
echo "Ceiling:       ${CEILING_NS} ns/frame"

if [ "$mean_ns_int" -gt "$CEILING_NS" ]; then
  echo
  echo "PERFORMANCE REGRESSION DETECTED"
  echo "system_full_ntsc_frame measured ${mean_ns_int} ns/frame, exceeding the ${CEILING_NS} ns ceiling."
  exit 1
fi

echo
echo "PERF GATE PASSED: ${mean_ns_int} ns/frame <= ${CEILING_NS} ns ceiling"
