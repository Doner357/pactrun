---
title: Alpha.2 retirement correction
---

# Alpha.2 retirement correction

**Status: Implementation and verification in progress.** The owner authorized
the black-box follow-up on 2026-10-01 after the documentation corrective closeout,
and explicitly approved the bounded CLI diagnostic disclosure extension after
reviewing its compatibility implications. Do not read this record as a passing
real-service test, completed integration or a published alpha.2 executable.

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

## Evidence and remaining gates

The initial regression failed on the unchanged alpha.1 runtime with OS permission
denied during handoff. The first correction passed 15 retirement filesystem tests;
a focused persistence test passed with obligations retained, read-only handoff
available, discard refused until explicit external repair, and retry completed.
These are intermediate results, not final candidate qualification.

Final Core diagnostics, cross-UID/real-service evidence, full CI, integration and
public documentation readback remain to be recorded. Alpha.1 published binaries
and tag must not be replaced. Visual redesign and documentation snapshots remain
deferred as recorded in the [Preview ledger](./public-preview-delivery.md).
