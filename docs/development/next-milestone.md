---
title: Baseline and M7 Lifecycle Handoff
---

# Current baseline and next milestone

**Status: Informative handoff derived from the roadmap and owning contracts.**
Document migration is complete for existing normative pages. M5 was separately
approved on 2026-09-13; documentation migration itself granted no implementation
approval. Implementation evidence is recorded separately from design approval.

## What is already recorded as implemented

The [M4 closeout](./m4-implementation-status.md) records integration through M4:
installation and managed Instances, Action execution, Capture and exact-compatible
Restore, Snapshot inspection/verification, bundle import/export, and explicit
storage upgrade on V5. It is a historical implementation record, not a substitute
for validating a new code change.

The integrated baseline adds [M5 Managed Input Migration](./m5-implementation-status.md):
path IDs, operator file acquisition, Frozen Migration Hooks, per-edge atomic
publication and bounded interruption/reconciliation. The authorized local merge
is recorded in the closeout; remote publication is separate from integration.

[M6 bounded recovery](./m6-implementation-status.md) is also implemented and
integrated into local develop. It closes existing-operation recovery evidence
on V6 without adding a new recovery engine, schema or service rollback.
[M6.5 ServiceStorage](./m6-5-implementation-status.md) is now implemented and
integrated into local develop, including V7, V2 installation and service
execution. [M7 lifecycle](./m7-implementation-status.md) is implemented, verified
and integrated into local develop under separate operator authorization.

- Current integrated persistence: [V8](../spec/persistence/persistence-schema-v8.md), internal and non-Frozen, with explicit exact-V7 upgrade only.
- Current capture writer: [Snapshot integrity V2](../spec/contracts/snapshot-integrity-format-v2.md).
- Existing Snapshot compatibility: [V1](../spec/contracts/snapshot-integrity-format-v1.md) remains Frozen and supported as specified.
- Pack source spelling: explicitly selected [Candidate V1](../spec/contracts/pack-source-yaml-v1.md) or [Candidate V2](../spec/contracts/pack-source-yaml-v2.md), not a public compatibility promise.
- Frozen [Core](../spec/contracts/revision-core-format-v1.md), [Hook protocol](../spec/contracts/hook-protocol-v1.md), and [error identity](../spec/contracts/error-taxonomy-v1.md) retain their existing scope.
- [Core V2](../spec/contracts/revision-core-format-v2.md) and [Hook V2](../spec/contracts/hook-protocol-v2.md) are independently Frozen; their format numbers do not force one another.

## M5 is implemented and integrated into develop

The [M5 roadmap section](./implementation-roadmap.md#m5---migration) is
**Implemented and integrated into develop** under the
[approved baseline](./design-notes/m5-migration-implementation-baseline.md).
[Implementation status](./m5-implementation-status.md) records the typed compiler,
V6 upgrade, declarative/operator/Hook execution, regression evidence and exact-tree
verification discipline. The delivery report must include the final post-integration
cargo xtask ci result and source-manifest check. A remote push still needs its
configured destination and a non-force branch-state check.

### Already defined: preserve these decisions

- [Target-owned Migration edges and transitions](../spec/contracts/migrations.md), including source roles and single-writer rules.
- [Intrinsic versus relational validation](../spec/behavior/packages-revisions-and-instances.md#pr-req-0262---migration-relational-installation-policy); installing a target is different from admitting an executable edge.
- Managed Input transitions, staged targets, incomplete intermediate state, and per-edge commit behavior as bounded in the roadmap.
- The shared [acceptance/concurrency](../spec/execution/execution-and-concurrency.md), [risk/recovery](../spec/execution/recovery-and-reconciliation.md), and [Hook protocol](../spec/contracts/hook-protocol-v1.md) contracts.

### Handoff checks

| Review item | Required outcome before the affected implementation |
| --- | --- |
| Scope and work order | M5, bounded M6, M6.5 and approved M7 S0-S7 are implemented and integrated into local develop; M8 is rejected and archived |
| Runtime/persistence integration | One owner/Run, whole-path pins, detached staging, independent Hook Sessions and per-edge atomic publication; no replay or service rollback |
| Command and diagnostic surface | [Migration spelling](../spec/behavior/m5-migration-command-reference.md), including path IDs, qualified operator files and per-invocation limits; no new stable JSON or public Rust API |
| Verification plan | Preserve valid/invalid sources, writer conflicts, absence, staging, intermediate states, final atomic success and fresh-process interruption tests; require full CI after any later edit |
| Contract conflict, if discovered | Name the conflicting rules and obtain semantic review instead of weakening a Frozen expectation |

The owning Spec pages define the approved new command and V6 direction. Frozen
HookProtocolV1 remains unchanged. The handoff is not a replacement for the
exact-source validation report.

## M6 is complete and integrated into develop

After the initial scheduling approval, the operator approved continuous bounded
M6 S0-S4 implementation on 2026-09-14. This does not approve ServiceStorage
representation or runtime. Start with the
[staged ServiceStorage alignment](./design-notes/service-storage-staged-design-alignment.md)
and [approved M6 bounded baseline](./design-notes/m6-recovery-implementation-baseline.md).
S0's operation/boundary/evidence audit retains exact V6. The
[M6 implementation record](./m6-implementation-status.md) identifies the added
cross-operation evidence and authorized local integration. M6 is `Complete`;
the delivery report records final post-closeout full CI and exact-source
verification. No push was authorized or performed.

M6 covers existing Action, Capture, Restore and Migration recovery, reusing
M3-M5 owner leases, continuations, risk handshake, exact pins and committed
boundaries. Its slices are S0 contract/evidence review, S1 owner/reconciliation,
S2 durable evidence/publication, S3 guard/diagnostics and S4 crash verification.
No replay, resumed Interrupted Run, uncommitted-output salvage, new CLI spelling
or silent V6 change is authorized.

<a id="m7-implementation-is-verified-integration-is-separate" />

## M7 is integrated; M8 is rejected

The completed sequence is **Pre-M6 -> M6 -> M6.5 -> M7**. M8 was removed by the
[2026-09-16 rejection decision](./history/m8-recipes-rejected.md), not postponed.
No next numbered milestone is selected. A public Candidate API is deferred until
concrete demand; YAML remains the built-in frontend. M6.5 is the dedicated
[ServiceStorage milestone](./implementation-roadmap.md#m65---servicestorage),
not an indefinitely deferred runtime. It is now `Complete` under its own
reviewed Core, authority, persistence, access, compatibility and upgrade
contracts. M6 completion removed the work-order prerequisite; the separate
2026-09-15 M6.5 approval authorized these representations.

The [M6.5 S0 design package](./design-notes/m6-5-servicestorage-baseline.md)
was reviewed and the user authorized continued S1-S7 implementation on
2026-09-15. Mapped sources are consumed atomically; only unmapped source-only
associations are retained, with no persistent alias mechanism. Core/YAML V2
conformance, V7 persistence and runtime paths are integrated into local develop.
Public V2 installation and the Core/Hook V2 Freeze gate are implemented;
final-source verification, Git integration and release remain distinct claims.
See [M6.5 implementation status](./m6-5-implementation-status.md) for runtime
evidence and the authorized integration record. This is not a release or push.

M6.5 closes the retention, finalization and abandonment ownership boundary:
V7 preserves custody, and M7 must add explicit versioned durable receipts before
destructive operations. M7 delivers actual Cleanup coordination,
do-not-replay/finalization, deletion and AbandonManagement operations. M6 does
not claim their runtime coverage or wait for Cleanup to exist; M6.5 must not
expose deletion that bypasses M7. The retired M8 number is not reused.

The [M7 closeout](./m7-implementation-status.md) now records V8, real Cleanup,
identity-bound physical finalization, explicit operator assertion, Abandon and
detached handoff/discard, together with the fresh remote full gate and Windows
evidence. The operator separately authorized the completed local commit and
merge. This does not grant permission to push or publish. The integrated
develop baseline is now M7/V8; it initializes V8 and explicitly upgrades exact V7.

[Closed ServiceStorage semantics](./design-notes/service-storage-semantic-baseline.md)
remain authoritative. Inputs, metadata and Workspace are not live-service
mirrors, and a Pactrun commit is not atomic with service bytes. The
[remaining design gates](./implementation-roadmap.md#deferred-servicestorage-representation-and-runtime-gates)
are assigned stages, not implied design approval. Snapshot deletion and the
broader non-ServiceStorage resource taxonomy remain outside this plan; that
taxonomy does not block bounded M6.5.

## Release readiness is not the next milestone

[Pre-release readiness work](./release-readiness.md) is approved but has no
assigned start after M7 or the retirement of M8. Its tasks can be scheduled as
appropriate and must be accepted before formal publication. Removing M8 is not a
release trigger. The [formal compatibility policy](../spec/foundations/product-versioning-and-compatibility.md)
owns the same-Major guarantee, development-format reset and 0.1.0-to-1.0.0
boundary; documenting that policy implements none of those new mechanisms.

## Evidence caveat

Managed Input Migration requirements now link to their direct automated evidence.
M6.5 ServiceStorage and transformation rules have their own real-resource,
cross-process and format evidence. M7 lifecycle now has its own acceptance
record; it is not inferred from Managed Input or M6.5 tests. The broader resource
taxonomy remains outside the approved scope.
