---
title: Alpha.4 Linux compatibility delivery
---

# Alpha.4 Linux compatibility delivery

**Status: Published and verified on 2026-10-04; Preview catalog activated,
public acquisition and Pages verified, release synchronized into develop.**

On 2026-10-04 the owner requested the alpha.4 compatibility delivery after the
[Debian 12 CI gate](./linux-compatibility-ci.md) passed. Public mutations remain
subject to execution approval. Alpha.1/alpha.2/alpha.3 tags, Release assets and
exact package records are immutable; no qualified alpha.3 bytes are replaced.

## Scope

- Product `1.0.0-alpha.4`; all eight format domains remain `1.0-alpha.1`.
- Build Linux x86-64 with the pinned Rust 1.98.1 Bookworm recipe and qualify the
  actual normal/test/standalone artifacts on glibc 2.36 before publication.
- Enforce the 2.36 ABI ceiling on both launcher and payload. Record the actual
  symbol requirements separately from the operating systems actually tested.
- Retain the Windows x86-64 native path and requalify its versioned artifacts.
- Verify the same Linux bytes on the configured newer Linux environment and
  rehearse actual alpha.3 to alpha.4 native-manager upgrades where alpha.3 runs.
- No Core behavior, store schema, Pack/Hook contract or lifecycle-authority change.
  No automatic privilege elevation, repair, libc replacement or custom global
  library path. Homebrew may still install its own GCC/glibc dependencies.
- No macOS, ARM64, musl, signing, Stable promotion, visual redesign or versioned
  documentation snapshots are introduced.

## Qualification and publication gates

1. Full source checks after the product-version change and the shared Debian 12
   build/direct-runtime/Homebrew workflow; preserve the published alpha.3 failure
   as an immutable negative control rather than silently altering it.
2. Source-qualified Windows/Linux artifacts, identical source provenance, exact
   archive/payload checksums, dependency/Rust/MIT notices and runtime receipts.
3. Actual native-manager upgrade, exact alpha.3 retention, data/Hook preservation,
   standalone reader journeys and a newer Linux runtime check on the same bytes.
4. Integrate through protected PRs, publish qualified bytes without rebuilding
   after catalog hashes are inserted, then activate ordinary main-branch Preview
   definitions and verify anonymous downloads, native acquisition and Pages.
5. Synchronize release results into develop and remove temporary remote branches.

Debian containers share a hosted kernel; they do not certify every Debian kernel,
filesystem or service. A later bounded check on the reported real Debian 12 host
can supplement this evidence. Passing glibc 2.36 qualification is not a promise
that every distribution with an equal or newer libc is certified.

## Immutable candidate and qualification

- Product: `1.0.0-alpha.4`; annotated tag target
  `7b119c741440ea9aef6f20cfbac6c341753c6c0f` (PR #25).
- Binary source: `6dd235ea4b970f26d90cd7ed58a0243c8c37c0ea`.
- Source manifest: `8a770e40b237a57da91724a035f6232096d1db228ac092505ebb467c84717498`.
- Candidate workflow: `37176534928`, Rust 1.98.1. Windows used Windows 2025;
  Linux used the pinned Bookworm image and both non-root Debian runtime gates.
- Windows payload: `fe40a5ad08ef4cf00ba3e3d69ff30cd7ddf6a6a3ef4a7efb3cda492ab2bd9500`.
- Linux payload: `95052d6f925b57b4cc0cf4c42c42ed4b2036d206fef6806a78063a45fcaf1413`.

The two platform snapshots have identical source archive/manifest bytes and
matching decoded provenance. All 537 selected source files match the committed
candidate and were checked before and after persistent Linux qualification.
Normal/test/standalone archives share the exact product payload per platform and
include MIT, dependency and Rust notices. All 11 server asset digests/sizes were
checked against qualified bytes before publishing the Release draft. Later
catalog or documentation commits do not rebuild or replace them.

| Check | Result |
| --- | --- |
| Release-candidate Debian 12 gates | Passed: both ELF ABI audits, 33 actual CLI invocations across three archive flavors without Homebrew, and 11 CLI invocations plus normal Homebrew install/formula test/uninstall as UID 1001 |
| ABI and runtime boundary | Both Linux binaries' highest observed glibc symbol version is 2.34 under a 2.36 ceiling; no private RPATH/RUNPATH or custom loader. Actual Debian userland is 12/glibc 2.36 |
| Original failure control | Immutable public alpha.3 archive matched its checksum and reproduced GLIBC_2.39 failure on clean Debian 12 |
| Actual final payload regression | Passed 13 Windows and 21 native Debian 13/glibc 2.41 assertions, including help, safe Migration acquisition and Linux stale-socket retirement boundaries |
| Native upgrade rehearsal | Passed 40 Windows/Scoop and 38 Linux/Homebrew assertions using real alpha.3 and alpha.4 artifacts, including exact retention, active Hooks, data/identity preservation, checksum refusal and source relocation |
| Standalone reader journeys | Both native archives passed checksum/destination refusals, fresh-store setup and an actual supplied Pack/Hook lifecycle; five guide/helper inputs match the qualified prospective snippets |
| Persistent Linux full source gate | Rust 1.99.0 cargo xtask ci passed: 531 library tests (four existing capacity opt-ins ignored), 17 Migration CLI, three retirement CLI, 76 system tests, other workspace/conformance checks, 89 docs tests and site typecheck/build |
| Hosted source gate | Run `37176535121` passed on the same source after the bounded Windows retry described below; no gate was bypassed |

Counts include checksum checks and expected refusals, not independent service
certifications. The existing opt-in native-manager test remains opt-in in ordinary
source CI; actual artifact rehearsals above ran separately. The temporary Scoop
PATH entry was removed and its before/after fingerprint matched. No existing
installation or real service data was used for these isolated acceptance runs.

Homebrew still installed its glibc/GCC dependency set in the clean Debian gate.
This is not a claim of smaller downloads. The independent no-Homebrew gate is
what proves the candidate runs against system glibc 2.36 without those libraries.
The same payload passed the configured native Debian 13/glibc 2.41 checks.

### Retained Windows CI observation

The first hosted source attempt failed one existing concurrent catalog test at
writer-session preparation, before its compare-and-set operation. The underlying
cause was not established from the bounded error. One unchanged failed-job rerun
passed the complete Windows gate; 30 isolated local repetitions with the release
compiler Rust 1.98.1 also passed, retaining the test's concurrent workers and all
assertions. The hosted compiler was Rust 1.99.0. No product behavior, test timing,
assertion or scheduling was changed to obtain a pass. Preserve the original failed
attempt and investigate session-preparation diagnostics if it recurs; do not
describe this observation as a fixed Core defect or a proven environment cause.

## Public rollout

The ordinary main-branch Preview entries advance to alpha.4 and new alpha.4 exact
entries are added. All alpha.1/alpha.2/alpha.3 exact definitions and release records
remain byte-identical. Anonymous downloads, real public source acquisition and
the deployed Pages digest are separate post-activation checks recorded at closeout.
The published alpha.3 remains unchanged and still requires glibc 2.39. Current
user instructions state the alpha.4 qualification baseline and do not promise
automatic per-glibc version selection or removal of Homebrew dependencies.

### Activation receipts

- Release `402847622` is a public prerelease, not the Stable/latest release.
  All 11 assets were downloaded anonymously and matched qualified sizes and
  SHA-256 values after publication; no binaries were rebuilt for catalog activation.
- Catalog/installation commit `2ac8a688d8bdc1de1b24fb670e9f64a8b3d016a6`
  passed both PR gates (`37179332049`, `37179334543`) without retries. PR #26
  merged into main at `e24e5275d3dd4676319e09998cd1f48acb5aa8ea`; PR #27
  synchronized develop at `c6e082fb8bd7d732a5dc0b3ad91b2a750f6da679`.
  Both merge trees match the catalog candidate. Main CI/Pages `37180043435`
  and develop CI `37180045800` passed without retries.
- Actual public-source acquisition passed 20 Windows/Scoop and 19 native
  Linux/Homebrew assertions: moving alpha.4 and exact alpha.3 selection,
  ordinary main source, separate management roots, command-conflict refusal,
  active Hook survival, reinstall and uninstall data retention. The temporary
  Windows PATH fingerprint was restored exactly. An initial Windows harness
  invocation failed before Scoop setup because its manager path was relative;
  the recorded acceptance run used absolute paths and a fresh isolated directory.
- The public Homebrew coexistence run used Debian 13/glibc 2.41, where alpha.3
  can execute. It is not a Debian 12 host result. The independent Debian 12
  qualification consumed the final alpha.4 candidate archives; anonymous public
  byte checks and generated formula parity tie that evidence to the publication.
- Public Pages browser checks passed all eight selected routes, the alpha.4
  installation and glibc 2.36 text, mobile overflow and JavaScript-error checks.
  Activation source digest:
  `630253daf8eea7517dd98ae63d030e05637ed5e37d5fb2a3b94b9855e5612fd9`.
  Subsequent receipt-only documentation has a new digest and must pass the same
  publication checks; this activation digest is not its final digest.
- Alpha.1/alpha.2/alpha.3 Release IDs and asset IDs, names, sizes and digests were
  unchanged against the pre-delivery audit. Their exact package definitions and
  release records remain byte-identical to pre-delivery main.

Closeout documentation reuses the unchanged source/artifact qualification above;
its own scope is documentation tests, links, typecheck/build and final Pages
verification, not a claimed additional full runtime qualification. The original
Windows CI observation remains retained, not silently classified as resolved.
Real reported-host Debian 12 acceptance remains a useful follow-up, not a claimed
part of these container/shared-kernel and native Debian 13 results.
