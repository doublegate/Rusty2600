# ADR 0003 — Accuracy-tiering honesty gate

## Status

Accepted.

## Context

The 2600's 8 KiB address window forces an unusually **broad** bankswitch-board
set: the research report enumerates **25 schemes** — from plain 2K/4K, through the
F8/F6/F4 and Superchip/CBS-RAM families, to ~17 long-tail schemes including
on-cart-RAM carts (E7, 3E, 4A50) and true coprocessor carts (DPC, and the
ARM-backed DPC+/CDF/CDFJ). Only a small core can be verified to the byte-identity
bar; an ARM-coprocessor cart cannot honestly claim the same accuracy as a plain
4K ROM. Per ref-docs/research-report.md §8, §10.5.

Without a guard, a headline like "25 schemes, AccuracyCoin 100%" would be
dishonest the moment a barely-modelled BestEffort board silently backed an oracle
ROM and inflated the pass-rate. Given the breadth here, this matters more than on
the NES.

## Decision

Every `Board` carries a `Tier { Core, Curated, BestEffort }` honesty marker
(`Tier::is_accuracy_gated` ⇒ true for Core/Curated, false for BestEffort).
Runtime behaviour is identical across tiers — the tier records only how much
external evidence backs the board. The split:

- **Core (2):** 2K, 4K — spec-implemented and oracle-gated.
- **Curated (6):** CV, F8, F6, F4, FA/CBS-RAM, Superchip (SC) — concrete game
  demand + a redistributable fixture/spec, register-decode unit-tested and
  boot-smoked.
- **BestEffort (17):** F0, FE, E0, E7, 3F, 3E, UA, 0840, EF, BF, DF, SB, X07,
  4A50, AR, DPC, DPC+/CDF/CDFJ — reference-ported, no redistributable fixture,
  register-decode tested only, **structurally never accuracy-gated**.

A CI test (`crates/rusty2600-test-harness/tests/mapper_tier_honesty.rs`)
**fails** if any board in the accuracy-oracle set reports a non-accuracy-gated
(BestEffort) tier. BestEffort boards may carry reference screenshots / boot-smoke
coverage but can never inflate the accuracy number. The gate's oracle set must be
extended in lockstep as each new board lands.

## Consequences

- The accuracy pass-rate stays truthful as the long-tail board set grows.
- BestEffort carts (including the deep ARM-coprocessor ones) can ship as "boots /
  approximate" without ever being mistaken for verified.
- Promoting a board BestEffort → Curated → Core is an explicit, reviewable change
  (add a fixture/oracle ROM, flip the tier, extend the gate's oracle set).
- The 25-scheme breadth is honestly representable from day one.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
