# Security Policy

## Supported Versions

Rusty2600 is pre-1.0, iterating through a `v2.x` gap-closure arc toward a
`v3.0.0` "Convergence" milestone. Security updates are provided for the
following versions:

| Version | Supported          |
| ------- | ------------------ |
| main    | :white_check_mark: |
| Latest minor (`v2.x`) | :white_check_mark: |
| Older `v2.x` / `v1.x` | :x: |

## Security Considerations for Emulators

While emulators primarily execute untrusted code in a sandboxed environment
(the emulated Atari 2600 hardware), several security considerations apply
to Rusty2600:

### ROM File Parsing

ROM files (`.a26`/`.bin`/`.rom`, and `.zip` archives containing them) are
untrusted input that must be parsed safely:

- **Bankswitch scheme auto-detection** (`rusty2600-cart::detect`) inspects
  raw ROM bytes to select one of the 26 known bankswitch schemes — all size
  and hotspot-address calculations must be bounds-checked, never trusting
  a ROM's declared or inferred size.
- **Integer overflows**: calculations involving ROM/bank sizes must be
  checked; the sparse/mirrored bus decode and cart hotspot match run on
  every CPU access and must stay panic-free on malformed input.
- **Malformed headers/images**: an unrecognized or truncated ROM image
  must be rejected gracefully (`detect` returning `None`), never panicking.
- **Zip-archive extraction**: ROM loading from `.zip` archives reads only
  a single in-memory entry's bytes into a size-bounded buffer; it never
  writes to a filesystem path derived from an archive's internal entry
  names (no path-traversal surface) and rejects an implausibly large
  claimed entry size before attempting decompression.

### The Thumb (ARM7TDMI) Coprocessor Interpreter

`rusty2600-thumb` interprets Thumb-1 instruction streams embedded in
DPC+/CDF-family cartridge images (the Harmony/Melody coprocessor family).
This is a second untrusted-bytecode surface beyond the 6507 CPU itself —
a malformed or adversarial driver ROM must fault cleanly (via
`Fault::UnimplementedPeripheral` and friends) rather than panic or read/write
out of bounds.

### Save State Deserialization

Save states (`rusty2600-core::SaveState`, `postcard`-encoded) contain
serialized emulator state that could be crafted maliciously:

- **Format/magic validation**: a save-state file is rejected if its magic
  bytes or format version don't match before any data is trusted
  (`SaveStateError::BadMagic`/`UnsupportedFormat`).
- **ROM-identity check**: a save-state is rejected if its `rom_tag` doesn't
  match the currently loaded ROM (`SaveStateError::RomMismatch`), so a
  save file can't silently be restored against the wrong cartridge.
- **No arbitrary code execution surface**: `postcard` deserializes into
  plain data structures only — never Rust code or function pointers.
- **Resource exhaustion**: manual save-state slots and the rewind ring are
  bounded (slot count, ring depth), not unbounded.

### Network Features (Netplay)

Rollback netplay (native UDP + STUN, with browser WebRTC planned) introduces
additional attack vectors:

- **Denial of service**: connection validation and reasonable timeouts
  required on all peer-facing paths.
- **State manipulation**: netplay state must be validated, not blindly
  trusted from a remote peer.
- **Privacy**: peer addresses (including STUN-discovered public addresses)
  must be handled carefully and only exchanged as the user explicitly
  intends.

### Scripting (Lua)

The Lua scripting interface (`rusty2600-script`, via `mlua`) provides
powerful automation and requires sandboxing:

- **Filesystem access**: restrict or audit all file operations exposed to
  scripts.
- **Resource limits**: guard against runaway scripts consuming excessive
  CPU time or memory.
- **API surface**: keep the exposed `emu.*` API minimal — only what
  scripts genuinely need (state read/write, save/load state,
  `drawText`/`drawRect`/`drawPixel` overlay calls).

### External Libraries

Rusty2600 depends on external crates that may have their own vulnerabilities:

- **FFI bindings**: `rc_client`/RetroAchievements integration
  (`rusty2600-cheevos`) uses `unsafe` FFI, confined to that crate and
  documented with `// SAFETY:` comments per this project's convention.
- **Dependency audits**: run `cargo audit`/`cargo deny` regularly (see
  "Planned Security Measures" below for CI integration status).
- **WASM security**: WebAssembly builds must not expose local filesystem
  access beyond the browser's own sandboxed File/FileReader APIs.

## Reporting a Vulnerability

**Please do not report security vulnerabilities through public GitHub issues.**

### How to Report

Send security vulnerability reports to:

**Email**: <parobek@gmail.com>

**Subject**: `[SECURITY] Rusty2600 - Brief Description`

### What to Include

Please provide the following information:

1. **Description**: Detailed description of the vulnerability
2. **Impact**: What could an attacker accomplish by exploiting this?
3. **Reproduction**: Step-by-step instructions to reproduce the issue
4. **Proof of Concept**: If applicable, a minimal test case or exploit
5. **Affected Versions**: Which versions of Rusty2600 are affected
6. **Suggested Fix**: If you have ideas for how to fix it (optional)

### Example Report Template

```text
Subject: [SECURITY] Rusty2600 - Out-of-bounds Read in Bankswitch Detection

Description:
An out-of-bounds read exists in the bankswitch scheme auto-detector when
processing an undersized ROM image claiming a scheme that expects a larger
fixed bank size.

Impact:
An attacker could craft a malicious .a26/.bin file that causes Rusty2600
to crash or read out-of-bounds memory.

Reproduction:
1. Create a truncated ROM image matching a BankF8-style hotspot pattern
   but shorter than the scheme's expected bank size
2. Load the file in Rusty2600
3. Observe out-of-bounds memory access

Proof of Concept:
[Attached: malicious.a26]

Affected Versions:
- main branch as of commit abc123
- All unreleased versions

Suggested Fix:
Add a bounds check in rusty2600-cart's detect() before assuming the
scheme's fixed bank size.
```

### Response Timeline

- **Initial Response**: Within 48 hours (acknowledgment of report)
- **Triage**: Within 5 business days (severity assessment)
- **Status Update**: Weekly until resolved
- **Fix Development**: Depends on severity (see below)
- **Public Disclosure**: After fix is released (coordinated disclosure)

### Severity Levels

| Severity | Response Time | Description |
|----------|---------------|-------------|
| **Critical** | 24-48 hours | Remote code execution, arbitrary file access |
| **High** | 3-7 days | Denial of service, memory corruption |
| **Medium** | 1-2 weeks | Information disclosure, resource exhaustion |
| **Low** | 2-4 weeks | Minor security issues with limited impact |

### What to Expect

1. **Acknowledgment**: We'll confirm receipt of your report
2. **Investigation**: We'll investigate and validate the issue
3. **Fix Development**: We'll develop and test a fix
4. **Disclosure**: We'll coordinate public disclosure with you
5. **Credit**: We'll credit you in the security advisory (unless you prefer to remain anonymous)

### Coordinated Disclosure

We follow responsible disclosure practices:

- **Embargo Period**: Typically 90 days from initial report
- **Early Disclosure**: If actively exploited in the wild, we may release earlier
- **Credit**: Security researchers will be credited in a security advisory
- **CVE Assignment**: Critical/High severity issues will receive CVE identifiers

### Public Disclosure

After a fix is released, we will:

1. Publish a security advisory on GitHub
2. Credit the reporter (with permission)
3. Notify users through GitHub Releases

### Hall of Fame

Security researchers who have responsibly disclosed vulnerabilities will be
credited here:

(No vulnerabilities reported yet)

---

## Security Best Practices for Users

### ROM Sources

Only load ROMs from trusted sources:

- **Homebrew**: download from official homebrew/AtariAge sources.
- **Test ROMs**: use the test suites already vetted into this repo
  (`tests/roms/`) or other well-known Atari 2600 test-ROM corpora.
- **Commercial ROMs**: legal backups of games you own. Rusty2600 never
  ships or commits commercial ROM images (see `tests/roms/external/`,
  entirely gitignored).

### Save States

Save states should be treated as potentially malicious input:

- **Unknown Sources**: don't load save-state files from untrusted sources.
- **File Inspection**: Rusty2600 rejects malformed/mismatched save states
  by construction (magic + format-version + ROM-tag checks), but treat any
  unusually large save-state file with suspicion.
- **Backups**: keep backups of important save-state slots.

### Netplay

When using netplay features:

- **Trusted Players**: only connect to players you trust.
- **Public Servers**: be cautious with any third-party STUN/signaling
  infrastructure.
- **Port Forwarding**: understand the security implications of opening
  ports for direct-IP/LAN netplay.

### Lua Scripts

When using Lua scripting:

- **Script Review**: review Lua scripts before running them.
- **Trusted Sources**: only use scripts from trusted authors.
- **Permissions**: pay attention to what a script's `emu.*` API calls do.

### Updates

Keep Rusty2600 updated:

- **Latest Version**: use the latest stable release.
- **Security Patches**: apply security updates promptly.
- **Release Notes**: read `CHANGELOG.md` for security-relevant fixes.

---

## Security Audits

Rusty2600 has not yet undergone a formal external security audit. Community
security reviews are welcome.

### Planned Security Measures

- **Static Analysis**: integrate `cargo-audit`/`cargo-deny` as a CI job
  (tracked for the `v3.0.0` "Convergence" release-hygiene gate).
- **WASM Sandboxing**: ensure WASM builds have no local filesystem access
  beyond the browser's own File/FileReader sandbox.
- **Fuzzing**: no `cargo-fuzz` harnesses exist yet for cart/ROM parsing or
  the Thumb interpreter — a real gap, not yet scheduled to a specific
  release; tracked as future work.
- **Dependency Scanning**: planned for CI integration alongside the audit
  job above.

### Current Security Posture

- **Language**: Rust provides memory safety by default; the chip stack
  (`rusty2600-cpu`/`rusty2600-tia`/`rusty2600-riot`/`rusty2600-cart`/
  `rusty2600-core`) is `#![no_std]` + `alloc`.
- **Unsafe Code**: confined to FFI boundaries (RetroAchievements'
  `rc_client` bindings in `rusty2600-cheevos`) and a small number of
  frontend/mobile FFI shims, each guarded by a `// SAFETY:` comment per
  this project's convention.
- **Fuzzing**: not yet implemented (see "Planned Security Measures" above).
- **Dependency Scanning**: not yet integrated into CI.
- **Security Testing**: ad-hoc, not formalized.

---

## Contact

- **Security Issues**: <parobek@gmail.com>
- **General Issues**: [GitHub Issues](https://github.com/doublegate/Rusty2600/issues)
- **Discussions**: [GitHub Discussions](https://github.com/doublegate/Rusty2600/discussions)

---

**Thank you for helping keep Rusty2600 and its users safe!**
