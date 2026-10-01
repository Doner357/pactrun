---
title: Alpha.2 public Preview delivery
---

# Alpha.2 public Preview delivery

**Scope: Public Preview artifact and native-source publication authorized on
2026-10-01.** This follows the [verified correction](./alpha2-retirement-fix.md),
not formal Stable promotion or a documentation redesign. Publication receipts
and final public acquisition are recorded in the release integration PR.

## Immutable candidate

- Product: `1.0.0-alpha.2`; all eight format domains remain `1.0-alpha.1`.
- Binary source: `26c44462aa02a5278db0a8b8771190205c921d18`.
- Source manifest SHA-256: `23049e8867f13198481bdf0af1fcea9a40493fd4c89dc28f34a746450709ee2f`.
- Candidate workflow: [36831581504](https://github.com/Doner357/pactrun/actions/runs/36831581504),
  Windows 2025 / Ubuntu 24.04, x86-64, Rust 1.98.1.
- Both platform builds used identical source archive and manifest bytes. Their
  JSON provenance differs only in platform-native line endings; decoded fields
  agree. Each platform's normal/test/standalone archives contain the same product
  payload plus MIT/dependency/Rust notices.

The versioned Release provides six program archives, build-source archive,
source manifest/provenance, release receipt and SHA256SUMS. Publish the qualified
bytes without rebuilding after inserting checksums into the package catalog.
The tag records the delivery; embedded source identity identifies the earlier
binary inputs. Alpha.1 records/assets/exact definitions are never replaced.

## Qualification

Actual isolated Scoop and Homebrew rehearsal covers alpha.1 to alpha.2 update,
hold/pin, unchanged exact selection, command conflicts, active Hook survival,
immutable managed data, explicit switch, bad-checksum refusal/recovery, source
relocation and uninstall. The actual released Linux candidate also undergoes the
corrected Authentik lifecycle regression; a debug-binary pass alone is not its
artifact acceptance. Final receipts distinguish prepublication qualification
from anonymous download verification and real public bucket/tap acquisition.

No user installation or real service store is used for acceptance. The temporary
Scoop PATH entry is removed and its before/after fingerprint checked. Test build
and evidence directories remain; generated private credentials are not release
assets. Native qualification is Windows 11/NTFS and Debian 13/ZFS, not a claim
for all platforms, kernels or filesystems. No macOS, ARM64, musl, code signing or
formal data downgrade guarantee is introduced.

## User-facing change

Preview moves to alpha.2; alpha.2 normal/test exact entries are added. Stable stays
absent. See [installation](../guides/installation.md) and
[updates/exact selection](../guides/version-management.md). The Core changes
improve safe handoff and bounded diagnostics; they do not elevate privileges,
rewrite existing Packs or automatically repair foreign-owned service data.
The corrected Authentik evaluation Pack is separate from the executable update.
