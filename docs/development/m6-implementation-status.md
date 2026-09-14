---
title: M6 Recovery Implementation Status
---

# M6 recovery implementation record

**Status: Complete. Bounded M6 S0-S4 is implemented and integrated into local
develop. This is not a release, push, or Pages deployment.**

The operator approved continued S0-S4 work on 2026-09-14, without per-slice
approval stops. The [bounded baseline](./design-notes/m6-recovery-implementation-baseline.md)
records the contract audit and exact V6 decision. The operator subsequently
authorized commit and merge, explicitly without push. Implementation commit
`5ddfb57b141426cbde8cbefd5016662f47b2ca54` was integrated on September 14, 2026
by no-fast-forward merge `ea6e57c07dbdb8a15c98f69de162f3043a153aa8`.
The merge tree equals the verified feature tree
`2263a06d474677cdd172d18283a93b9465239205`. The feature branch is retained.

The implementation tree passed full configured-remote CI, workspace-contained
M6 tests and Windows adapter tests, with all 256 source hashes unchanged before
and after CI. This integration closeout changes documentation and its regression
checks only. The delivery report must additionally record full CI and source
manifest verification after this final closeout edit; the earlier result alone
does not verify this later tree. No ServiceStorage runtime is claimed.

## Slice outcomes

| Slice | Implemented outcome |
| --- | --- |
| S0 | Traced the existing application owner probes, shared terminal transaction, operation-specific publications, strict loaders and reference release against the owning rules. No new schema or recovery policy is needed. |
| S1 | Reused the existing owner mechanism. PR-TEST-0325 drives live owners, missing/inconclusive leases, confirmed loss and two independent reconciler processes for accepted and admitted Action/Capture/Restore/Migration Runs. Exactly one process publishes the terminal result. |
| S2 | Added before/after risk-resolution fault points without changing production semantics. PR-TEST-0326 covers ten acceptance/Admission/entry/resolution/terminal crash points for all four operations. PR-TEST-0327 proves provenance roots, execution-reference release and no old-Input restoration after later legal management. |
| S3 | PR-TEST-0328 runs a trusted native Hook that changes a test-owned external file after risk acknowledgment; recovery neither rolls it back nor claims confinement. PR-TEST-0329 proves successful Action/Capture overrides preserve the guard and do not authorize the next ordinary execution. Existing Migration and Restore assertions now map to their generic recovery requirements. |
| S2/S3 boundary closure | PR-TEST-0330 injects crashes before/after orphan terminalization and explicit manual resolution across all four operations; state token, guard and consequence publication remain atomic. Manual-resolution fault calls are test-only instrumentation, not a new lifecycle. |
| S4 | Cross-operation tests and bidirectional evidence passed full CI; the implementation tree was integrated after explicit operator authorization. The delivery report supplies final post-closeout CI and exact-source evidence. |

The owner-probe synchronization point makes the race deterministic: both
reconcilers observe the same Running candidate before publication is released.
A separate probe race holds the lease after the first observation and proves
that the mandatory second probe prevents interruption.

S1-S3 findings were proof and instrumentation gaps, not grounds to rewrite the
existing recovery engine. The shared owner lease, mutation lock, transactional
terminal consequence and strict V6 readers/writers remain the production path.
The added fault calls are no-ops outside tests. No production schema, CLI,
public Rust API, Frozen identity, error or Hook wire changed.

## Evidence and scope

Generic checkpoint and Plan-free recovery now link to the real per-edge and
mixed-chain crash tests PR-TEST-0304/0317. Those tests retain committed
intermediate state and verify final-edge atomic success; M6 does not replace
them with a synthetic resource model. Capture and Restore retain their real
publication/crash tests, including PR-TEST-0252/0259/0260/0263/0264.

Recovery-reference release follows the existing V4-V6 operation contracts:
the committed Instance owns current Revision/bindings, an unresolved guard
strongly references its terminal Run, and no completed recovery directive needs
execution-only bytes for replay. The M6 tests also verify that legitimate later
Input changes survive repeat reconciliation while the trust guard remains.

PR-REQ-0053/0059/0061/0062 and the existing-operation portion of PR-REQ-0070
have direct automated mappings instead of only pending declarations. The
successful-deletion clause of PR-REQ-0070 remains pending under M7. ServiceStorage
PR-REQ-0246 remains pending under M6.5; Cleanup/finalization/AbandonManagement
PR-REQ-0247 remains pending under M7. Broader M5 and service-resource promises
are not certified by related recovery tests.

## Verification and integration gates

The delivery report must contain the final configured-remote `cargo xtask ci`
result, including conformance/traceability, formatting, Clippy, all workspace
tests, document typechecking and the production build. It must also report
workspace-contained Windows adapter tests and the before/after exact-source
manifest comparison. Tests of a previous tree or a shared-target checkout are
not final evidence. Use checkout-local Cargo target and locked document
dependencies, with no node_modules symlink to another checkout.

Commit and local integration were explicitly authorized and performed; push was
explicitly excluded and was not performed. Existing unrelated Pages workflow
and archive files remain excluded; no Pages deployment, release tag, branch
deletion or new preview service is part of M6. Retained verification resources
must be disclosed in the delivery report. M6.5 remains Proposed pending its
separate representation and runtime design approval.
