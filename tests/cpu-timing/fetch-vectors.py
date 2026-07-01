#!/usr/bin/env python3
"""Regenerate the trimmed SingleStepTests/65x02 corpus in `singlestep-6502/`.

The committed vectors are a size-trimmed subset of
https://github.com/SingleStepTests/65x02 (MIT, Thomas Harte et al) — see the
README in this directory. This script reproduces (or extends) that subset by
downloading one opcode's full case list and keeping the first N cases.

Dependency-free (Python 3.8+ standard library only).

Fetch a single opcode:
    python3 fetch-vectors.py <opcode-hex> <out.json> [cases]
    # e.g. python3 fetch-vectors.py a9 singlestep-6502/a9.json 20

Regenerate the whole committed corpus (all 233 implemented opcodes, 20 cases
each) from a shell — the opcode list is every `<hex>.json` already present:
    for f in singlestep-6502/*.json; do
        op=$(basename "$f" .json)
        python3 fetch-vectors.py "$op" "$f" 20
    done
"""
import sys, json, urllib.request

DEFAULT_CASES = 20  # matches tests/cpu-timing/README.md ("20 cases per opcode")

opcode = sys.argv[1].lower()
out = sys.argv[2]
n = int(sys.argv[3]) if len(sys.argv) > 3 else DEFAULT_CASES

url = f"https://raw.githubusercontent.com/SingleStepTests/65x02/main/6502/v1/{opcode}.json"

try:
    with urllib.request.urlopen(url, timeout=30) as resp:
        data = json.load(resp)
except Exception as e:
    print(f"SKIP {opcode}: {e}")
    sys.exit(0)

subset = data[:n]
with open(out, "w") as f:
    json.dump(subset, f)
print(f"OK {opcode}: {len(subset)} cases -> {out}")
