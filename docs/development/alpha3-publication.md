---
title: Alpha.3 public Preview delivery
---

# Alpha.3 public Preview delivery

**Status: Authorized preparation and qualification in progress; not yet published.**

On 2026-10-02 the owner authorized alpha.3 candidate qualification, main
integration, GitHub prerelease publication, Preview bucket/tap updates and Pages.
Alpha.1/alpha.2 assets, tags and exact package definitions remain immutable.
Stable promotion, extra platforms, signing, documentation redesign and versioned
documentation snapshots are outside this delivery.

## Product and compatibility

- Product version: `1.0.0-alpha.3`. All eight format domains stay `1.0-alpha.1`.
- Include the verified [Migration CLI/help](./migration-cli-ux.md) and
  [retirement diagnostics](./retirement-diagnostics.md) improvements.
- Machine additions are optional members. Existing error identities, accepted-Run
  boundaries, deletion authority, storage formats and Pack/Hook semantics remain.
- No automatic socket deletion, privilege escalation, repair, retry or Cleanup
  replay. Installing alpha.3 does not repair an old Pack or foreign-owned data.
- Retain unsigned Windows x86-64 and Linux x86-64 Preview qualification; no
  macOS, ARM64, musl or general production-service certification is added.

## Delivery gates

1. Verify the exact release source after the product-version change, including
   full source CI and existing error/schema/privacy regressions.
2. Build source-qualified Windows/Linux artifacts using the manual read-only
   candidate workflow. Accept exact artifacts before integrating into main.
3. Rehearse alpha.2 to alpha.3 using isolated native managers, preserving exact
   alpha.2 selection and baseline data, and exercise candidate CLI diagnostics.
4. Publish those qualified bytes without rebuilding after adding catalog hashes.
   Link binary-source SHA, source manifest, tag and metadata integration receipts.
5. Verify anonymous downloads, real public package acquisition and Pages; merge
   release results back into develop and remove temporary remote branches.

Evidence will be recorded here as each gate completes. Historical verification
does not substitute for acceptance of the alpha.3 executable artifacts.
