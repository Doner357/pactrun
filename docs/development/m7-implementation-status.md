---
title: M7 Implementation Status
---

# M7 implementation status

**Status: Implementation and verification complete; not integrated.**

The operator authorized continuous S0-S7 implementation on 2026-09-16 under the
[M7 baseline](./design-notes/m7-cleanup-deletion-implementation-baseline.md).
Work remains on `feature/m7-instance-retirement`; no commit, merge, push, release
or deployment is implied. Unrelated untracked workflow/archive files are untouched.
Pactrun remains a local, offline CLI; configured SSH is development verification
infrastructure, not a product feature.

## Current state

The temporary Linux refusal has been replaced by identity-qualified isolation
and durable physical progress. Every traversed object is captured into a fresh
private parent before destructive use; a retry does not reuse parents exposed
by an earlier handoff. Allocation-scoped ownership excludes handoff while new
processing positions are active. Relative native names and all surviving data
locations remain available for explicit partial handoff.

Windows now pins each affected namespace against rename/delete sharing until
its work ends, including ancestor directories. Both backends retain explicit
incarnation checks, refuse links/reparse points out of scope, preserve outside
hard-link contents, and never operate on a replacement using a completed receipt.

Private incarnation tags 3 (Linux isolation) and 4 (Windows pinning) fence older
readers that understand only tags 1/2. A legacy incarnation is promoted only after
matching its native fields, and before physical work. No Frozen Core, Hook,
Snapshot identity or canonical-vector bytes are changed.

Unexposed preparation intents are not proof of ownership. They retain their
conservative maintenance and origin records, are not promoted by retirement or
Abandon, and do not acquire detached-discard authority. Protected active,
retained and exposed/unpublished allocations retain the approved lifecycle.

| Slice | Implementation / acceptance |
| --- | --- |
| S0 | Approved behavior, V8, human CLI, ownership boundaries and traceability recorded |
| S1 | Exact V7-to-V8 upgrade; history, custody, strict readers and physical capability fencing |
| S2 | Pure compilation, typed requirements, durable acceptance/refusal, concurrent mutation and lifecycle gates |
| S3 | Real V1/V2 Cleanup, risk matrix, cancellation, ambiguous owner loss and no-replay boundaries |
| S4 | Windows namespace guards and Linux per-object isolation; crash/replacement/permission and outside-data coverage |
| S5 | Version-bound operator assertion, managed no-Hook Abandon, preserved original outcomes and surviving custody |
| S6 | Read-only detached inspection, explicit fragment handoff and discard; concurrency, corruption and interrupted retry |
| S7 | Actual CLI-process tests, docs, traceability and final source-matched Linux/Windows acceptance passed |

## Acceptance matrix

Requirement definitions remain in Spec; this table indexes evidence rather than
creating product semantics. Worker repetitions and overlapping filters are not
summed as distinct product tests.

| Area | Evidence |
| --- | --- |
| V8 exact upgrade, rollback, stale writers and history | PR-TEST-0392 through PR-TEST-0395; PR-TEST-0400 through PR-TEST-0403 |
| Compilation, refusal and ownership competition | PR-TEST-0396, PR-TEST-0397, PR-TEST-0422, PR-TEST-0446 |
| No lifecycle/recovery-override bypass | PR-TEST-0447 |
| Real Cleanup, risk and unchanged protocol authority | PR-TEST-0399, PR-TEST-0404, PR-TEST-0405, PR-TEST-0424, PR-TEST-0425, PR-TEST-0426 |
| Ambiguous completion and explicit confirmation | PR-TEST-0398, PR-TEST-0407 |
| Abandon, owned storage lifetimes and history | PR-TEST-0406, PR-TEST-0408, PR-TEST-0409, PR-TEST-0421, PR-TEST-0423 |
| Interrupted/replaced/absent targets | PR-TEST-0410 through PR-TEST-0414 |
| Human commands and explicit disclosure/intent | PR-TEST-0415 through PR-TEST-0418; PR-TEST-0436, PR-TEST-0437 |
| Strict custody and reference validation | PR-TEST-0419, PR-TEST-0420, PR-TEST-0428, PR-TEST-0441, PR-TEST-0445 |
| Concurrent discard writers | PR-TEST-0427 |
| Namespace swap and descendant reparenting | PR-TEST-0429, PR-TEST-0430, PR-TEST-0442 |
| Partial recovery, handoff and known completion | PR-TEST-0431, PR-TEST-0432, PR-TEST-0433, PR-TEST-0439, PR-TEST-0440, PR-TEST-0444 |
| Native names, handoff exclusion, readonly data and hard links | PR-TEST-0434, PR-TEST-0443 |
| Platform/capability-qualified evidence | PR-TEST-0435, PR-TEST-0438 |

## Evidence history

An earlier snapshot passed configured-remote full `cargo xtask ci` with source
manifest `4f82a4b46efbec016f8e59b25113bfe2e4bcdb325bcbb6bf133929123f5b42e7`.
That pass did not cover the subsequently reproduced namespace interleavings.
The two counterexamples then failed against the former Linux algorithm and a
fail-closed hold prevented its further production use. Those results remain
historical evidence, not proof of the replacement.

The replacement has closed the recorded namespace gap. Both original
counterexamples now pass, along with repeated interruption, malformed progress,
previously handed-off bindings, native names, outside hard links and namespace
sharing conflicts. Self-review is not independent verification; the acceptance
results below are the automated evidence for this bounded M7 implementation.

## Final acceptance — 2026-09-16

**Passed:** fresh configured-remote `cargo xtask ci`, run in the persistent M7
test workspace against source-manifest SHA-256
`513dbf3b7296f01fed2ae2fc4ddf1e4e9a36f4cec34b63122328d1b4e4af0647`.
All listed source files matched before and after the run; the full gate exited 0.

- Rust library: **399 passed**.
- Actual CLI processes: M3 **4**, M5 **10**, M7 **2** passed.
- System tests: **43 passed**; xtask tests: **40 passed**.
- Frozen conformance/vector checks, workspace formatting and Clippy passed.
- Documentation typecheck, all **19** documentation/link checks, and Docusaurus
  production build passed. No site publication or deployment occurred.
- The focused Linux retirement filesystem group independently passed **14**
  tests. Filter/worker repetitions are not added to the full-suite counts.

Remote build inputs: Rust/Cargo 1.98.0, Node 24.19.0, pnpm 11.21.0;
`CARGO_PROFILE_DEV_DEBUG=0` and `CARGO_PROFILE_TEST_DEBUG=0` were used to avoid
repeatedly copying debug-symbol-heavy Hook fixtures. No test was skipped.

**Passed on Windows, using its ordinary debug profile:** deletion filter **35**,
retirement filter **14**, V8 filter **9**, preparation-maintenance filter **2**,
and the two actual M7 CLI-process tests. Namespace/hard-link/readonly/foreign-
progress cases are included in the retirement evidence. Windows workspace Clippy
and formatting passed. These overlapping focused groups are not a complete
Windows product-suite claim.

The final runtime, tests, dependencies, embedded DDL and vectors remain unchanged
after this snapshot. Only informative status/navigation/README and their
documentation assertions are updated for closeout; their affected checks are
rerun separately. The full Rust gate is reused for unchanged runtime inputs,
not represented as a second fresh run after editorial closeout.

## Handoff

S0-S7 implementation and verification are complete on the feature branch. Git
review, commit and integration into develop require the operator's separate
instruction; no merge or remote publication is inferred. M8 and release-readiness
work are not started by this closeout. The product remains an offline CLI with
trusted local Hooks; it is not a sandbox against deliberate control-store
tampering or a service supervisor.

Development evidence is kept under ignored `target/m7-linux-claims/` and earlier
`target/m7-transfer/` / `target/m7-race-evidence/` directories. The configured
persistent test workspace and development caches are retained. No product
network feature, background service or deployment is introduced.
