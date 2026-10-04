---
title: Alpha.2 public Preview delivery
---

# Alpha.2 public Preview delivery

**Status: Publicly published and native acquisition verified on 2026-10-01.**
The owner authorized this delivery after the
[verified correction](./alpha2-retirement-fix.md). This is not formal Stable
promotion or a documentation redesign. [PR #10](https://github.com/Doner357/pactrun/pull/10)
integrated the delivery; [PR #11](https://github.com/Doner357/pactrun/pull/11)
activated the ordinary main-branch catalog and records final publication receipts.

- Public prerelease: [v1.0.0-alpha.2](https://github.com/Doner357/pactrun/releases/tag/v1.0.0-alpha.2).
- Immutable annotated tag target: `6e84bfec919c5614c99b0d28464c8aef63fba9f6`.
- Catalog activation main commit: `d57537d32de189155551495627eb3e6bdd0d0706`.

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

Actual isolated Scoop and Homebrew rehearsal covered alpha.1 to alpha.2 update,
hold/pin, unchanged exact selection, command conflicts, active Hook survival,
immutable managed data, explicit switch, bad-checksum refusal/recovery, source
relocation and uninstall. The actual released Linux payload also passed the
corrected Authentik lifecycle regression; a debug-binary pass was not substituted
for artifact acceptance. Receipts distinguish prepublication qualification from
anonymous download verification and real public bucket/tap acquisition.

| Check | Actual result |
| --- | --- |
| Isolated native lifecycle | Passed 40 Windows/Scoop and 36 Linux/Homebrew assertions with real alpha.1 and alpha.2 artifacts |
| Standalone user journey | Both platforms passed the published setup, checksum/destination refusal cases, fresh store and real Pack/Hook lifecycle |
| Actual release Authentik | Passed 18 CLI verification steps, including expected conflict failure, authenticated API, stop/start persistence, portable Pack import and all three ordinary-user retirement cases |
| Public Release bytes | All 11 assets downloaded without authentication and matched qualified size/SHA-256; server-reported asset digests also matched |
| Public native acquisition | Passed 20 Windows and 17 Linux assertions from the real GitHub bucket/tap and Release URLs, including alpha.2 selection, retained alpha.1 exact test package, Hook execution and data-preserving reinstall/uninstall |
| Hosted source/catalog checks | Windows/Ubuntu integration runs 36833256220, 36835454206 and 36835461314 passed; main publication pipeline is [36837360193](https://github.com/Doner357/pactrun/actions/runs/36837360193) |

Counts include checksum assertions and expected negative outcomes; they are not
independent certifications or a claim of full Authentik feature coverage. The
released Linux payload SHA-256 is
`ed43f47219a92c2e5dee8009e29b5408da63bdd26397b411716cdd81c386c5fe`;
Windows is `85c8e5a95ae3c76392d9e6d270d36babef72181cae57425204108ee22c751908`.
The corrected Authentik Revision remained
`sha256:a822adb7c0444fda388b1bc9fbc23d7f16d2051acac75ba167e4f332928389e2`.
Its test containers were removed through the tested lifecycle. Linux imports
GLIBC symbols through 2.39; Windows imports were OS DLLs with no separate MSVC
redistributable dependency observed. The initial standalone rehearsal mistakenly
supplied a test runner as its fixture producer; corrected runs used the actual
Pactrun payload and passed without changing the published procedure.

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
