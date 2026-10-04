---
title: Alpha.4 Linux compatibility delivery
---

# Alpha.4 Linux compatibility delivery

**Status: Candidate preparation and qualification in progress; not yet published.**

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

Exact source, workflow, artifact and public verification receipts will be recorded
as gates complete. Existing source qualifications do not replace actual acceptance
of the new alpha.4 artifacts. The current public alpha.3 still requires glibc 2.39.
