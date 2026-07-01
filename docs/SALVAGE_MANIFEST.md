# Salvage Manifest — Rusty2600

Record of files rescued out of volatile `/tmp` into the project tree (via the
`tmp-salvage` skill / manual curation) so they survive reboot.

## 2026-06-30 — session e0ae5628 scratchpad

Reviewed the session scratchpad
(`/tmp/claude-1000/-home-parobek-Code-OSS-Public-Projects-Rusty2600/`) and the
sibling `gopher-harness/`. Everything there was duplicate, superseded, or
regenerable — salvaged **one** item of standalone value:

| Source | Destination | Note |
|---|---|---|
| `…/scratchpad/fetch_one.py` | `tests/cpu-timing/fetch-vectors.py` | Corpus generator for `singlestep-6502/`; cleaned up (docstring, 20-case default, usage) and referenced from `tests/cpu-timing/README.md`. Original left in `/tmp` to clear on reboot. |

Deliberately **not** salvaged (regenerable/transient):

- `scratchpad/commit_vecs/` (233 JSON) — identical to the already-committed
  `tests/cpu-timing/singlestep-6502/`; pure duplicate.
- `scratchpad/testvecs/` (233 JSON, 5 cases each) — superseded intermediate trim.
- `gopher-harness/` — compiled Go probe binaries (~48 MB) + transient trace
  `.txt` outputs from the Frogger-jitter differential investigation; rebuildable
  from `ref-proj/Gopher2600/` (see the `rusty2600-gopher-differential-oracle`
  memory) and reproducible on demand.
- Assorted `*.txt` PC-trace / RAM-dump comparison dumps — transient analysis.
