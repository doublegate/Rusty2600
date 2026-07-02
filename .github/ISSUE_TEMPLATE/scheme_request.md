---
name: Bankswitch Scheme Request
about: Request a new or variant cart bankswitch scheme
title: '[SCHEME] Name - short description'
labels: bankswitch, enhancement
assignees: ''
---

<!--
Rusty2600's bankswitch catalogue is CLOSED and complete at 26/26 known
schemes (see docs/cart.md) — every scheme ever used by a commercially
released or well-documented homebrew Atari 2600 cartridge is already
implemented. This template is for the two cases that can still genuinely
arise:

1. A sub-variant of an already-implemented family (e.g. a new CDFJ+
   driver-ROM revision, or an AR/Supercharger BIOS variant) that behaves
   differently from what's currently modeled.
2. A genuinely novel scheme discovered in a newly-dumped or newly-released
   homebrew cartridge that doesn't match any of the 26 known schemes.

If you're not sure which case applies, describe what you've found below and
we'll figure it out together.
-->

## Scheme Information

**Scheme name:**
[e.g. CDFJ+ rev.2, or a new scheme name if genuinely novel]

**Relationship to the existing catalogue:**

- [ ] Variant/revision of an already-implemented scheme (name it: ______)
- [ ] Genuinely novel scheme not in `docs/cart.md`'s 26-scheme table

## Cartridges Using This Scheme

List cartridges that use this scheme (title, region, and where you learned
it uses this scheme):

1. **Game name** — [region] — [source: dump header / community doc / other]

## Technical Details

**Bank count / size:** [e.g. 8 x 4 KiB banks]

**Hotspot addresses:** [known hotspot addresses, if documented]

**Special hardware:** [coprocessor, extra RAM, RNG, audio hardware, etc.]

**Reference implementations:**

- [ ] Stella (`ref-proj/stella/`): [link/path if you know it]
- [ ] Gopher2600 (`ref-proj/Gopher2600/`): [link/path if you know it]
- [ ] Other: [name and link]

## Test ROMs

- [ ] I can provide a legally-obtained ROM dump for local, gitignored
      testing (never committed to the repo — see `tests/roms/external/`)
- [ ] No test ROM available

## Checklist

- [ ] I checked `docs/cart.md`'s existing 26-scheme table first
- [ ] I checked this isn't already covered by an existing `Tier`
      (`Core`/`Curated`/`BestEffort` — see ADR 0003)
- [ ] I described the relationship to the existing catalogue above
