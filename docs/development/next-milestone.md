---
title: Baseline and M6 Handoff
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
| Scope and work order | Approved M5 S0-S5 scope implemented and locally integrated after operator authorization; M6 remains Proposed |
| Runtime/persistence integration | One owner/Run, whole-path pins, detached staging, independent Hook Sessions and per-edge atomic publication; no replay or service rollback |
| Command and diagnostic surface | [Migration spelling](../spec/behavior/m5-migration-command-reference.md), including path IDs, qualified operator files and per-invocation limits; no new stable JSON or public Rust API |
| Verification plan | Preserve valid/invalid sources, writer conflicts, absence, staging, intermediate states, final atomic success and fresh-process interruption tests; require full CI after any later edit |
| Contract conflict, if discovered | Name the conflicting rules and obtain semantic review instead of weakening a Frozen expectation |

The owning Spec pages define the approved new command and V6 direction. Frozen
HookProtocolV1 remains unchanged. The handoff is not a replacement for the
exact-source validation report.

## M6 is the next proposed milestone, not implicitly approved

Start by reviewing the broader pending recovery requirements and producing the
next bounded baseline. Reuse M5's owner continuation, risk handshake, exact pins,
committed boundary evidence and explicit reconciliation. Do not reinterpret an
Interrupted Run as resumable, salvage uncommitted output files, or broaden the
Managed Input checkpoint into a service-owned resource representation. M6 design
approval remains separate from the completed M5 implementation and integration.

## Deferred work must not leak into bounded M5

[ServiceStorage semantics](./design-notes/service-storage-semantic-baseline.md)
are closed, but concrete serialization, authority where needed, durable retention
and abandonment representation, and runtime remain deferred. The
[roadmap's storage gates](./implementation-roadmap.md#deferred-servicestorage-representation-and-runtime-gates)
remain explicit. Do not model this work as synchronized Managed Inputs or claim
that a service database transaction is atomic with Pactrun persistence.

Broader recovery generalization (M6), Cleanup/deletion (M7), and Recipes/advanced
authoring (M8) remain Proposed. Their unresolved future design is not permission
to expand M5; it becomes an M5 blocker only if the approved scope actually depends
on it. Snapshot deletion and a broader non-ServiceStorage resource taxonomy are
not introduced by this migration.

## Evidence caveat

Managed Input Migration requirements now link to their direct automated evidence.
Broader pending requirements, especially ServiceStorage and Package/service-owned
transformation rules, remain pending. Related Managed Input tests do not
automatically close those rules or retrospectively certify every future promise.
