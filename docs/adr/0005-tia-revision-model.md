# ADR 0005 — TIA revision variation as independent quirk flags, not a chip enum

## Status

Accepted.

## Context

Real Atari 2600 units contain TIA chips from different fabrication runs, and
software has been observed to depend on subtle timing/behavioral differences
between them (research report §14, open question 3: "Multiple TIA revisions
(TIA-1A etc.) introduce subtle variations... Gopher2600 models several.
Decide which revision Rusty2600 defaults to and whether to parameterize.").

Investigating Gopher2600's actual approach
(`ref-proj/Gopher2600/hardware/tia/revision/bugs.go`) found it does **not**
model this as a small number of named chip revisions (e.g. "TIA-1A" vs
"later"). It models eight **independent, individually-named hardware
quirks**, each tied to a specific documented behavior and a specific
real-world ROM/demo that depends on it:

| Quirk | Behavior | Notable ROM |
|---|---|---|
| `LateVDELGRP0` / `LateVDELGRP1` | `VDELP0`/`VDELP1` delayed-graphics-bit write lands one cycle late | He-Man |
| `LateRESPx` | `RESPx` triggers a little later under certain HMOVE conditions | 36-char demos |
| `EarlyScancounter` | `RESPx` triggers an early draw under certain HMOVE conditions | 36-char demos |
| `LatePFx` | Playfield bits set one video cycle late | Pesco |
| `LateColor` | `COLUPF`/`COLUBK` update one video cycle late | Quickstep (RGB-modded light sixer) |
| `LostMOTCK` | The motion clock is sometimes ineffective when HBLANK is off | **Cosmic Ark** (the starfield/missile bug — see `docs/tia.md`, already tracked as a v0.7.0/v0.8.x hard problem) |
| `RESPxHBLANK` | `RESPx` reacts late to an HBLANK reset; temperature-dependent on real silicon | "2 or 3 sprite" demo |

This is a better-fitting model than a coarse revision enum: real-world
variation isn't cleanly bucketed into a handful of discrete named chip
generations — it's closer to a continuum of per-die/per-batch quirks (one is
even documented as *temperature*-dependent on real hardware), and different
ROMs depend on different, non-overlapping subsets of them. A single
`TiaRevision::Early | Late` enum would force an artificial pairing between
quirks that don't actually co-occur.

## Decision

Model TIA revision variation as a **bitset of independently-toggleable named
quirk flags**, mirroring Gopher2600's set above, rather than a coarse
revision enum. **Default: all quirks off** — the idealized/most-common TIA
behavior the vast majority of commercial ROMs were authored and tested
against — with per-quirk opt-in for the specific titles/demos that need one.

This ADR records the **decision and the flag catalogue**, not the
implementation: each quirk becomes its own scoped ticket, generally attached
to whichever area of the codebase it affects —
`LostMOTCK` (Cosmic Ark) is already tracked as a v0.7.0/v0.8.x hard-problem
item; `LateRESPx`/`EarlyScancounter`/`RESPxHBLANK` refine the already-
implemented RESPx pipeline (`docs/tia.md` §Object positioning) and belong
with `T-0601-007`'s future HBLANK-collision work or a sibling ticket;
`LatePFx`/`LateColor` are playfield-timing refinements; `LateVDELGRP0/1`
touch the vertical-delay graphics-register swap. None are implemented by
this ADR.

## Consequences

- No single "which TIA revision does Rusty2600 emulate" question needs an
  answer — the honest answer is "the idealized common case, with named
  per-quirk opt-ins," which is also more honest with users than a fictional
  "TIA-1A vs TIA-1B" toggle would be.
- Each quirk can be implemented, tested, and differential-oracle-verified
  (against Gopher2600, which already has all eight) independently, in
  whatever order matches demand, without blocking on the others.
- A future settings/debug UI surface (`debug-hooks`, v0.5.0) can expose these
  as individual checkboxes rather than a single ambiguous dropdown.
- `RESPxHBLANK`'s temperature dependence is **not** modeled (no thermal
  simulation) — if a quirk flag is added for it, it is a fixed on/off toggle,
  not a dynamic real-time-dependent behavior.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
