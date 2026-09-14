---
title: Baseline and M6.5 ServiceStorage Handoff
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
on V6 without adding a new recovery engine, schema or service rollback. The next
proposed milestone is M6.5 ServiceStorage, starting with its representation
design gate rather than runtime implementation.

- Current persistence: [V6](../spec/persistence/persistence-schema-v6.md), internal and non-Frozen, with explicit exact-V5 upgrade only.
- Current capture writer: [Snapshot integrity V2](../spec/contracts/snapshot-integrity-format-v2.md).
- Existing Snapshot compatibility: [V1](../spec/contracts/snapshot-integrity-format-v1.md) remains Frozen and supported as specified.
- Pack source spelling: [Candidate V1](../spec/contracts/pack-source-yaml-v1.md), not a public compatibility promise.
- Frozen [Core](../spec/contracts/revision-core-format-v1.md), [Hook protocol](../spec/contracts/hook-protocol-v1.md), and [error identity](../spec/contracts/error-taxonomy-v1.md) retain their existing scope.

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
| Scope and work order | M5 and bounded M6 S0-S4 integrated into local develop; M6.5 is next and remains separately gated |
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

## M6.5 ServiceStorage is next, before M7

The fixed order is **Pre-M6 -> M6 -> M6.5 -> M7 -> M8**. M6.5 is the dedicated
[ServiceStorage milestone](./implementation-roadmap.md#m65---servicestorage),
not an indefinitely deferred runtime. Its implementation remains `Proposed`
until the separately reviewed Core, authority where needed, persistence,
access, compatibility and upgrade contracts are approved. M6 completion removes
the work-order prerequisite; it does not approve these representations.

M6.5 must align retention, finalization and abandonment persistence ownership
with M7 before schema approval. M7 delivers actual Cleanup coordination,
do-not-replay/finalization, deletion and AbandonManagement operations. M6 does
not claim their runtime coverage or wait for Cleanup to exist; M6.5 must not
expose deletion that bypasses M7. M7 and M8 retain their existing numbering.

[Closed ServiceStorage semantics](./design-notes/service-storage-semantic-baseline.md)
remain authoritative. Inputs, metadata and Workspace are not live-service
mirrors, and a Pactrun commit is not atomic with service bytes. The
[remaining design gates](./implementation-roadmap.md#deferred-servicestorage-representation-and-runtime-gates)
are assigned stages, not implied design approval. Snapshot deletion and the
broader non-ServiceStorage resource taxonomy remain outside this plan; that
taxonomy does not block bounded M6.5.

## Evidence caveat

Managed Input Migration requirements now link to their direct automated evidence.
Broader pending requirements, especially ServiceStorage and Package/service-owned
transformation rules, remain pending. Related Managed Input tests do not
automatically close those rules or retrospectively certify every future promise.
