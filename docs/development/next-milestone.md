---
title: Baseline and M5 Handoff
---

# Current baseline and next milestone

**Status: Informative handoff derived from the roadmap and owning contracts.**
Document migration is complete for existing normative pages. This does not
approve the next product milestone or close its future design decisions.

## What is already recorded as implemented

The [M4 closeout](./m4-implementation-status.md) records integration through M4:
installation and managed Instances, Action execution, Capture and exact-compatible
Restore, Snapshot inspection/verification, bundle import/export, and explicit
storage upgrade on V5. It is a historical implementation record, not a substitute
for validating a new code change.

- Current persistence: [V5](../spec/persistence/persistence-schema-v5.md), internal and non-Frozen.
- Current capture writer: [Snapshot integrity V2](../spec/contracts/snapshot-integrity-format-v2.md).
- Existing Snapshot compatibility: [V1](../spec/contracts/snapshot-integrity-format-v1.md) remains Frozen and supported as specified.
- Pack source spelling: [Candidate V1](../spec/contracts/pack-source-yaml-v1.md), not a public compatibility promise.
- Frozen [Core](../spec/contracts/revision-core-format-v1.md), [Hook protocol](../spec/contracts/hook-protocol-v1.md), and [error identity](../spec/contracts/error-taxonomy-v1.md) retain their existing scope.

## M5 is the next design/implementation handoff, still Proposed

The [M5 roadmap section](./implementation-roadmap.md#m5---migration) remains
**Proposed**. This documentation task does not change it to Planned or grant
runtime implementation approval. The next step is a bounded M5 scope and
dependency review using the now-consolidated contracts.

### Already defined: preserve these decisions

- [Target-owned Migration edges and transitions](../spec/contracts/migrations.md), including source roles and single-writer rules.
- [Intrinsic versus relational validation](../spec/behavior/packages-revisions-and-instances.md#pr-req-0262---migration-relational-installation-policy); installing a target is different from admitting an executable edge.
- Managed Input transitions, staged targets, incomplete intermediate state, and per-edge commit behavior as bounded in the roadmap.
- The shared [acceptance/concurrency](../spec/execution/execution-and-concurrency.md), [risk/recovery](../spec/execution/recovery-and-reconciliation.md), and [Hook protocol](../spec/contracts/hook-protocol-v1.md) contracts.

### Resolve in the M5 approval baseline

| Review item | Required outcome before the affected implementation |
| --- | --- |
| Scope and work order | Explicit bounded approval and completion gates; do not infer approval from this handoff |
| Runtime/persistence integration | Map per-edge state and commit/recovery evidence onto the existing substrate; identify any compatibility-constraining schema change before implementing it |
| Command and diagnostic surface | Reuse already defined behavior; decide exact new spelling or output only where the [command contract](../spec/behavior/command-and-output-reference.md) leaves it open |
| Verification plan | Map owning rules to real tests for valid/invalid sources, writer conflicts, staging, intermediate states, and interruption/per-edge outcomes; preserve existing regression protection |
| Contract conflict, if discovered | Name the conflicting rules and obtain semantic review instead of weakening a Frozen expectation |

These are review deliverables, not newly invented product requirements. This
handoff does not claim a new schema, command, or Hook version is necessary.

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

Existing pending verification declarations remain unchanged, including broader
rules that coexist with narrower implemented requirements. Related tests do not
automatically close a broader rule. The migration preserves the evidence graph;
it does not retrospectively certify every promise as completely covered.
