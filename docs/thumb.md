# ARM7TDMI Thumb interpreter — Rusty2600

References: `to-dos/ROADMAP.md` (v1.6.0 "Coprocessor" + the v1.6.x patch
train); `docs/cart.md` (the DPC+/CDF/CDFJ/CDFJ+ BestEffort family this
interpreter will eventually back); `crates/rusty2600-thumb/src/lib.rs`. This
doc is the SPEC, not history — update it in the same PR as the code.

## What this crate is

`rusty2600-thumb` is a real ARM7TDMI **Thumb-1** (16-bit encoding)
interpreter, ported from Gopher2600's Go implementation
(`hardware/memory/cartridge/arm/`), not Stella's C++ `Thumbulator`.
Gopher2600's memory-safety-first style (explicit bounds, no raw pointer
arithmetic) maps far more naturally onto this project's own
`#![forbid(unsafe_code)]` house style than a straight port of Stella's C++
would.

It backs the Harmony/Melody coprocessor cartridges (DPC+/CDF/CDFJ/CDFJ+,
`T-0401-006`) — those boards run a real ARM7TDMI alongside the 6507,
executing Thumb-1 code that streams graphics/audio data and drives a fast
RNG. **v1.6.0 landed the interpreter core plus conformance tests only.**
`rusty2600-cart` now depends on this crate (added when the first
consumer landed — see "What's next" below); `rusty2600-thumb` itself
remains a standalone `no_std + alloc` crate with zero dependency on
`rusty2600-cart`/`rusty2600-core`/`rusty2600-frontend`, keeping the
one-directional crate graph intact (`rusty2600-cart` depends on
`rusty2600-thumb`, not the other way around). Coprocessor families wire
in one at a time.

## Why Thumb-1 only

The Harmony/Melody boards these coprocessors run on are **ARM7TDMI**
(ARMv4T) — they never execute Thumb-2 (32-bit encoding) or ARMv7-M/Cortex-M0
instructions. Gopher2600's `arm/` package also supports those newer
architectures (for a different, non-2600 use case its authors needed), via
`thumb2*.go`, `extended_registers.go`, and an `fpu/` subpackage — none of
that is ported here. Likewise `rng/` and `timer/` (per-cartridge peripheral
register packages) and `architecture/` (per-board register-address maps)
are cartridge-specific concerns deferred to the `Board` that eventually
wires a real coprocessor family in; this crate's `ThumbMemory` trait is the
generic seam that future work plugs into, deliberately without hardcoding
any one board's memory map.

## Architecture (matches the crate)

- `registers.rs` — the 16 general registers (`R0..=R12` general purpose,
  `R13` = SP, `R14` = LR, `R15` = PC). The stored `PC` value is always
  `(next fetch address) + 2`, matching Gopher2600's own pipeline bookkeeping
  exactly (see the module doc for the full invariant); `Arm7Tdmi::register`/
  `set_register` normalize this away for external callers.
- `status.rs` — the N/Z/C/V flags (`bitflags`-backed). Thumb-1 has no
  IT-block/EPSR state (a Thumb-2 concept) and no saturation (`Q`)
  instructions, so both are omitted rather than carried as dead weight.
  `condition(cond: u8)` evaluates the 14 real Thumb-1 branch conditions;
  `0b1111` (reserved/unpredictable on real silicon) is an `unreachable!()`,
  not silently treated as "always branch".
- `memory.rs` — the `ThumbMemory` trait a future `Board` implements
  (mirrors Gopher2600's `SharedMemory`), plus a `Fault` enum
  (`IllegalAddress`/`UnimplementedPeripheral`/`NullDereference`/
  `Misaligned`) returned as a `Result`, not a Go-style panic-and-log.
- `cycles.rs` + `mam.rs` — the N/S/I cycle-type model and MAM
  (Memory Accelerator Module) prefetch-latch approximation, ported from
  `cycles.go`/`cycles_arm7tdmi.go`/`mam.go`. **This is a genuinely
  approximate hardware-timing model even in the reference implementation**
  (float-based cycle stretching; Gopher2600's own comments admit some
  constants are unverified) — this crate does not claim cycle-exactness for
  the coprocessor path, only a faithful port of the same approximation,
  consistent with this project's "never present approximate output as
  exact" rule (`docs/adr/0003`'s spirit, applied here even though this
  crate sits outside the cart bankswitch-tier honesty gate itself, since
  it isn't a `Cartridge` variant).
- `thumb.rs` — the actual Thumb-1 decode/execute, all 19 instruction-format
  classes from the ARM7TDMI Data Sheet (move-shifted-register,
  add/subtract, move/compare/add/subtract-immediate, ALU operations,
  hi-register operations + branch-exchange, PC-relative load,
  load/store-with-register-offset, load/store-sign-extended,
  load/store-with-immediate-offset, load/store-halfword,
  SP-relative-load/store, load-address, add-offset-to-SP,
  push/pop-registers, multiple-load/store, conditional branch, software
  interrupt, unconditional branch, long-branch-with-link).
- `lib.rs` — `Arm7Tdmi<M: ThumbMemory>`, generic over the memory it
  executes against rather than a `dyn Trait` object (the same reasoning
  `rusty2600-cart`'s closed `Cartridge` enum uses to avoid `dyn Board`,
  applied here via a type parameter since a coprocessor instance only ever
  has one memory implementor). `step()` executes exactly one instruction
  and returns `(StepOutcome, cycles)`; `StepOutcome::ProgramEnded` reports
  a `BX`/`BLX` reaching the address the 6507 originally called in from.

## Deliberate deviations from the Go reference

- Disassembly-string generation (interleaved into the same decode
  functions in `thumb.go`) is skipped entirely — this crate has no
  disassembler yet.
- Memory faults are a typed `Result<_, Fault>`, not a logged panic-and-continue
  — including the software-interrupt (`SWI`) path and the out-of-scope
  `ARMinterrupt` real-ARM32-function-call hook a `BX` to non-Thumb code can
  trigger in the reference (a cartridge-specific integration mechanism, not
  modeled here; reported as a fault instead of silently mishandled).
- A `go_shl` helper reproduces Go's "shift count at or beyond the operand's
  bit width yields zero" `<<` semantics for the one `ROR`-by-register carry
  computation (format 4) that relies on it with an unmasked shift count —
  Rust's native shift operators don't define that behavior, so this is
  ported as an explicit, documented helper rather than left to chance.
- `MapAddress`'s per-region flash/RAM latency table (Gopher2600's
  `architecture.Map`) is flattened to a single-region approximation here —
  a real per-board memory map is exactly the kind of cartridge-specific
  detail the `v1.6.x` wiring pass supplies via its own `ThumbMemory` impl,
  not something this generic crate should hardcode.

## Testing

No bundled ARM/Thumb conformance corpus exists in this repo (unlike the
6507's SingleStepTests/Klaus oracles) — `thumb.rs`'s test module is real,
hand-authored register-decode-style coverage per instruction-format class
(shifts including the shift-by-zero edge cases, add/sub with flag
verification, all ALU operations, hi-register ops + `BX`-to-return,
PC-relative/SP-relative/register-offset/immediate-offset/halfword/
sign-extended load-store round-trips, load-address, push/pop including
LR/PC, multiple load-store including the base-register-in-list edge case,
conditional/unconditional branch, `BL`, and `SWI` faulting rather than
panicking) against a minimal in-memory `TestMemory` harness. This crate
itself is **not** part of the cart bankswitch-tier honesty gate
(`tests/mapper_tier_honesty.rs`, ADR 0003) — that gate is `Core`/`Curated`-
only, and DPC+ (this interpreter's first real consumer) lands `BestEffort`
tier, so the gate's oracle set is correctly NOT extended for it either
(same as `BankAr` before it). `BankDpcPlus`'s own tests (`docs/cart.md`,
`crates/rusty2600-cart/src/lib.rs`) include a real hand-assembled synthetic
Thumb-1 program proving `Arm7Tdmi::step()` actually executes against a real
`ThumbMemory` implementation, not just that the register-decode logic
compiles — the first end-to-end proof this interpreter drives a real
cartridge board correctly.

## What's next

Both Harmony/Melody coprocessor families now consume this interpreter,
closing the bankswitch catalogue to 26 of 26 schemes:

- **DPC+ landed first** (`BankDpcPlus`, `[2.2.0]`, see `docs/cart.md`): a
  real `ThumbMemory` implementation over the board's own driver/custom/
  data/freq ROM+RAM segments (mapped at Gopher2600's own Harmony-
  architecture Flash/SRAM addresses), driven synchronously from the `$5A`
  "CALLFUNCTION" register write rather than a per-color-clock scheduler
  hook — DPC+'s ARM entry point turned out to be a synchronous call-and-
  run-to-completion from within `Board::cpu_write`, not something needing
  `Board::tick_coprocessor()` or any `Bus`/scheduler change at all.
- **CDF/CDFJ/CDFJ+ landed second** (`BankCdf`, `[2.3.0]`, one struct
  covering all four sub-versions via a `CdfVersion` const table — see
  `docs/cart.md`): reuses the exact same synchronous CALLFN-to-
  `ProgramEnded` shape, plus a genuinely new consumer of this crate's
  `Fault::UnimplementedPeripheral` path — CDF's driver ROM makes real
  host-serviced calls via a `BX` to a fixed non-Thumb address (DPC+'s
  equivalent hook is a no-op stub its driver never triggers). No changes
  to this crate were needed to support it: `Arm7Tdmi::instruction_pc()`
  already reports the exact call-site address the dispatch needs, and
  `set_register`'s existing `+2` PC-storage convention already produces
  the correct resume target with no extra adjustment from the caller.
  This is the first real-world validation that the fault-reporting API
  this crate exposes is sufficient for a genuine ARM-interrupt-driven
  cartridge, not just a hypothetical future need.

`Board::tick_coprocessor()` remains unused by both families — the
synchronous call-and-block model needs no per-color-clock scheduler tick,
only `Board::tick()`'s existing once-per-CPU-cycle hook (for each family's
own music-fetcher phase-accumulator divider).
