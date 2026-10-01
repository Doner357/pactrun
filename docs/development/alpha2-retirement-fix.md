---
title: Alpha.2 retirement correction
---

# Alpha.2 retirement correction

**Status: Implemented, with source-qualified Core and real-service verification.** The owner authorized
the black-box follow-up on 2026-10-01 after the documentation corrective closeout,
and explicitly approved the bounded CLI diagnostic disclosure extension after
reviewing its compatibility implications. Git integration and hosted checks are
recorded in [PR #8](https://github.com/Doner357/pactrun/pull/8). This correction
did not itself publish an alpha.2 executable or replace the alpha.1 release.
The separately authorized [alpha.2 delivery](./alpha2-publication.md) owns public
artifacts, native acquisition and publication receipts.

## Scope and responsibility

- Correct Linux read-only handoff's unnecessary readable-directory access while
  preserving incarnation, kind, journal, no-follow and allocation-lock checks.
- Preserve permission/contention classifications without leaking OS strings or
  paths. Keep error identity, failure step, Cleanup completion, retained obligations
  and explicit retry semantics unchanged. Never add automatic elevation or chmod.
- Qualify a corrected disposable Authentik Pack whose PostgreSQL and Redis
  containers use the invoking non-root host UID/GID from initial deployment.
  This is not a conversion of existing foreign-owned databases, a Core permission
  bypass, or a promise that every container/service UID policy is supported.
- Reproduce permission failures, prove read-only handoff and retained obligations,
  rerun real startup/persistence/three retirement scenarios, then run full candidate
  verification and integrate through protected branches.

## CLI format review

The approved change extends the set of Core errors eligible for explanatory
`detail`; it is not an implementation-only change to the old disclosure policy.
The field remains nullable text with the same diagnostic meaning. No field,
requiredness, error identity, enum, retry authority or lifecycle state is removed
or reinterpreted. Therefore the CLI format stays `1.0-alpha.1`; the software
candidate independently advances to `1.0.0-alpha.2`. All other format domains
remain unchanged. Tests must exercise both accepted exact reasons and rejected
arbitrary/prefixed messages, using the owning PR-REQ-0359 contract rather than
misattributing retirement diagnostics to the initialization requirements.

Consumers that assumed every non-initialization detail would always be null must
relax that assumption. This prerelease disclosure extension is explicit; it is
not a blanket claim of compatibility with clients relying on that narrower
policy. Clients must not parse diagnostic prose into a new machine reason code
or use it as retry authorization. Unknown historical messages stay withheld.

## Qualification evidence

The initial regression failed on the unchanged alpha.1 runtime with OS permission
denied during handoff. The first correction passed 15 retirement filesystem tests;
a focused persistence test passed with obligations retained, read-only handoff
available, discard refused until explicit external repair, and retry completed.
Those initial results were followed by the following candidate evidence:

| Check | Result and scope |
| --- | --- |
| Core source | `3bea5882863e74187256a2462440a695151df9c0`; all 519 committed source files matched the configured remote checkout byte-for-byte |
| Complete remote gate | Passed `cargo xtask ci` with Rust 1.98.1 on the persistent Linux/ZFS workspace, opt-level 1, debug assertions and overflow checks enabled; 525 library tests passed, 4 existing capacity tests remained explicitly ignored; 75 system tests passed, plus the other workspace/conformance suites |
| Documentation | Typecheck, build and 89 source/link/traceability checks passed; repeated focused documentation checks also passed |
| Public catalog | Generator comparison and immutable-record checks passed; package definitions remain on the published alpha.1 assets |
| CLI diagnostics | Permission failure survives live, historical JSON and human Run inspection without exposing native paths or service bytes; exact allowlist and unknown-message rejection passed |
| Actual cross-UID negative case | Non-root UID 1000 encountered synthetic UID 12345 data; deletion/discard refused with safe diagnostics, read-only handoff succeeded, owner/mode stayed unchanged; separately authorized fixture repair enabled explicit discard |
| Corrected Authentik Pack | 18 CLI operations qualified formal export/fresh-store import, execution with author source moved away, authenticated API access, stop/start persistence, conflicting deployment retirement, normal retirement and imported-store retirement |
| Final service cleanup | Both stores had no live Instances or detached allocations; no test containers or Compose networks remained |

The real-service executable SHA-256 was
`2d9838075dda85ee72f4e68399dac4814dad8d466d2efa3a2c8850dbf1745e8e`.
The corrected Pack kept PackageId `c5db0e627956645236b74e53a2334086` with Revision
`sha256:a822adb7c0444fda388b1bc9fbc23d7f16d2051acac75ba167e4f332928389e2`.
PostgreSQL and Redis actually ran as UID/GID 1000:105; their initialized host
directories were respectively 0700 and 0755 under that same owner. All three
positive retirements ran as that non-root user, without administrative repair.
The cross-UID negative case's administrative cleanup is separate evidence, not
a positive ordinary-user retirement result.

The retained private evidence archive SHA-256 is
`7b27ecb0b2a5722dc2b18f802fa0c186a370aac3ed50eae20d879c4b4db24af9`.
It contains scoped logs and receipts, not the generated private credential files.
It is not automatically published by this document.

### Windows test-harness correction

The first hosted attempt passed Ubuntu but failed the existing PR-TEST-0165
orphan-inspection assertion on Windows. The harness used `taskkill /T` before
killing the owner handle: child-first termination can let the live owner publish
a normal terminal result, defeating the intended orphan setup. The original log
did not capture the terminal result, so this is a supported race explanation,
not a forensic identification of that particular scheduler ordering.

The Windows harness now kills/reaps the exact owner first, then cleans recorded
Hook trees. A pre-inspection assertion proves the orphan exists; all original
read-only and no-extra-Hook assertions remain. Twenty exact native Windows
repetitions and both related reconciliation cases passed. No product timeout,
runtime termination behavior or failed assertion was relaxed. Final hosted checks
and merge/publication receipts belong to PR #8 and the subsequent integration
record. The Core/Pack inputs remain those qualified above; test-only and
documentation follow-ups reuse that runtime evidence with affected checks.

## Limits and deferred work

This is a same-host Linux real-service regression with a corrected evaluation
Pack, not a fresh independent blind evaluation or general Authentik certification.
It does not qualify TLS trust, browser SSO, service upgrades, cross-host data
movement or automatic repair of old foreign-owned storage. Docker daemon access
is privileged capability; non-root Pactrun is not rootless Docker certification.
The default gate's existing capacity/native-package opt-in tests were not rerun
as new delivery acceptance. Alpha.2 artifact/native-manager publication remains
a separate delivery task; alpha.1 binaries, tag and exact entries remain intact.
Visual redesign and documentation snapshots remain deferred in the
[Preview ledger](./public-preview-delivery.md).
