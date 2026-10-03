---
title: Alpha.3 public Preview delivery
---

# Alpha.3 public Preview delivery

**Status: Qualified artifacts published as a public prerelease on 2026-10-02;
package-source activation and public acquisition/Pages closeout in progress.**

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

## Immutable published candidate

- Release: `v1.0.0-alpha.3`, 11 qualified assets; no Stable promotion.
- Annotated tag target: `1959122b63853320dd7ac34bb65145db68389c20` (PR #18).
- Binary source: `100747eda54386604ef455c66143bf24b770eca4`.
- Source manifest SHA-256:
  `d3ae4f5b9997790a0dbb9f1f199e9278b2a8446c872ce45c91639cd55f928996`.
- Candidate workflow: `37021984811`; Windows 2025 / Ubuntu 24.04, Rust 1.98.1.
- Windows payload: `c9acc46cdd347e38a2f509174118f7fa6cce283cb199df02baf1801bbca0703c`.
- Linux payload: `4ba29e21852919270342a3cf4e9371dfb360207d0d7c58d2231857c0fbe6104b`.

Both platform snapshots have identical source archive/manifest bytes and matching
decoded provenance. All normal/test/standalone archives carry the same native
payload per platform and include MIT, dependency and Rust notices. The 11 server
asset size/digest records matched qualified bytes before publishing the draft.
Later catalog and documentation commits do not rebuild these payloads.

## Qualification

| Gate | Result |
| --- | --- |
| Final source CI | Hosted Windows/Ubuntu run `37021991676` passed |
| Configured persistent Linux full gate | Rust 1.99.0 `cargo xtask ci` passed on the exact 527-file source snapshot; 531 library passes, four existing capacity opt-ins ignored, 17 Migration CLI, three retirement CLI and 76 system tests, other workspace/conformance checks, 89 docs tests, typecheck/build |
| Actual artifact diagnostics | 13 Windows and 21 Linux assertions passed on the final release payloads, including help parity, redacted acquisition failure, no accepted Run, and real stale-socket retirement/obligation/discard on Linux |
| Native alpha.2 to alpha.3 rehearsal | 40 Windows/Scoop and 38 Linux/Homebrew assertions passed, including hold/pin, exact alpha.2 retention, command conflicts, active Hook survival, identity/data preservation, checksum refusal/recovery and source relocation |
| Standalone reader journey | Both final native archives passed the prospective alpha.3 documentation snippets: checksum/destination refusal, fresh-store setup, portable Pack import and actual Hook lifecycle |
| Release tooling | Seven source/artifact tooling tests passed on Windows and Linux; dispatch remains manual and read-only, and off-main builds require the exact versioned release head |

Counts include checksum checks and expected negative outcomes, not independent
certifications. The diagnostics scenario is a synthetic native socket regression,
not a Caddy/Authentik production deployment or a repeat of all external black-box
cases. The existing opt-in native-manager test stays opt-in in ordinary source CI;
the actual supplied-artifact rehearsal above ran separately. Scoop's temporary
user PATH entry was removed and its before/after fingerprint matched.

The first unpublished candidate at `1ed5a74` failed the full-version-label
handoff documentation check (run `37019679379`); runtime checks passed. Correcting
the handoff produced the final source above, fresh candidate artifacts and fresh
artifact/native/standalone acceptance. No failed candidate was publicly uploaded,
no test was weakened, and no existing release was replaced.

Linux imports GLIBC symbols through 2.39. Windows imports are OS DLLs, with no
separate MSVC redistributable dependency observed. Existing platform/trust limits
remain; no user installation or real service store was used for acceptance.

## Public rollout

Preview normal/test entries advance to alpha.3; alpha.3 exact entries are added.
All alpha.1/alpha.2 exact definitions and release records remain byte-identical.
Anonymous download, real public bucket/tap acquisition and the deployed Pages
digest are verified separately after activation and recorded at closeout.
The source snapshot preserves build-time documentation; main carries the later
publication instructions and receipts. Visual redesign and documentation version
snapshots remain deferred.
