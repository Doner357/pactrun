---
title: Development Reading Paths
---

# Read by development task

**Status: Informative navigation.** Read the owning contracts, not just this table.
Within each page, use the reading map and section headings to narrow the context.

| Task | Read first | Then check |
| --- | --- | --- |
| Change object identity or ownership | [System model](../spec/foundations/system-model.md), [identity](../spec/foundations/identity-and-state.md) | [Resources and versions](../spec/foundations/resources-and-versioning.md), affected observable behavior |
| Maintain installation or Instance management | [Instance behavior](../spec/behavior/packages-revisions-and-instances.md), [Pack source](../spec/contracts/pack-source.md) | [Revision baseline](../spec/contracts/revision-canonical.md), [Persistence baseline](../spec/persistence/persistence-baseline.md) |
| Maintain Action execution | [Execution](../spec/execution/execution-and-concurrency.md), [Action behavior](../spec/behavior/actions-plans-and-runs.md) | [Hook protocol](../spec/contracts/hook-protocol.md), [recovery](../spec/execution/recovery-and-reconciliation.md), [shared Snapshot boundary](../spec/execution/m4-snapshot-lifecycle-approval-baseline.md) |
| Change Input or Secret handling | [Input behavior](../spec/behavior/inputs-secrets-and-readiness.md), [declarations](../spec/contracts/actions-inputs-and-parameters.md) | [Identity and state](../spec/foundations/identity-and-state.md), [Hook authority and disclosure](../spec/contracts/hooks-recovery-and-cleanup.md) |
| Work on Snapshots | [Snapshot semantics](../spec/contracts/snapshots-and-managed-data.md), [operation behavior](../spec/behavior/snapshots-migrations-and-recovery.md) | [Integrity baseline](../spec/contracts/snapshot-integrity.md), [bundle baseline](../spec/contracts/snapshot-bundle.md), [limits](../spec/behavior/m4-runtime-capabilities.md) |
| Maintain Migration | [Migration contract](../spec/contracts/migrations.md), [execution](../spec/execution/m5-migration-execution.md) | [Command behavior](../spec/behavior/m5-migration-command-reference.md), [selectors](../spec/contracts/cli-id-selectors.md), [Hook contexts](../spec/contracts/hook-protocol.md), [Persistence baseline](../spec/persistence/persistence-baseline.md) |
| Maintain recovery | [Recovery contract](../spec/execution/recovery-and-reconciliation.md), [execution ownership](../spec/execution/execution-and-concurrency.md) | [Retirement ambiguity and confirmation](../spec/execution/m7-instance-retirement.md), source-qualified failure evidence |
| Maintain ServiceStorage | [Resource ownership](../spec/foundations/resources-and-versioning.md), [service execution](../spec/execution/m6-5-service-storage-execution.md) | [Revision declarations](../spec/contracts/revision-canonical.md), [Hook grants](../spec/contracts/hook-protocol.md), [resource commands](../spec/behavior/m6-5-service-storage-command-reference.md) |
| Maintain Cleanup, abandonment or detached custody | [Retirement](../spec/execution/m7-instance-retirement.md), [Cleanup invariant](../spec/behavior/snapshots-migrations-and-recovery.md#pr-req-0247---cleanup-finalization-and-abandonment) | [Persistence baseline](../spec/persistence/persistence-baseline.md), [resource lifetimes](../spec/foundations/resources-and-versioning.md) |
| Change CLI or diagnostics | [Commands](../spec/behavior/command-and-output-reference.md), [machine interface](../spec/contracts/cli-machine-interface.md) | [Selectors](../spec/contracts/cli-id-selectors.md), [diagnostics](../spec/behavior/execution-diagnostics.md), [error identity](../spec/contracts/error-taxonomy-v1.md) |
| Change persistence or compatibility | [Persistence baseline](../spec/persistence/persistence-baseline.md), [version policy](../spec/foundations/product-versioning-and-compatibility.md) | [Identity/state](../spec/foundations/identity-and-state.md), [recovery](../spec/execution/recovery-and-reconciliation.md), exact crash boundaries |
| Maintain retained-object deletion or GC | [Lifecycle contract](../spec/behavior/managed-object-lifecycle.md), [resource lifetimes](../spec/foundations/resources-and-versioning.md) | [Persistence coordination](../spec/persistence/persistence-baseline.md), [delivery evidence](./managed-object-lifecycle-status.md) |
| Review historical ServiceStorage approval | [S0 baseline](./design-notes/m6-5-servicestorage-baseline.md), [activation record](./design-notes/m6-5-format-activation-review.md) | [Implementation record](./m6-5-implementation-status.md); its historical format labels do not establish current support |

For historical recovery/ServiceStorage sequencing, read the
[M6 bounded design](./design-notes/m6-recovery-implementation-baseline.md) and
[staged alignment](./design-notes/service-storage-staged-design-alignment.md).
Their original gates and approvals are historical evidence, not new work orders.

## Versioning and publication planning

For formal-release compatibility, read the
[owning product policy](../spec/foundations/product-versioning-and-compatibility.md)
and [independent version domains](../spec/foundations/resources-and-versioning.md).
For task scope, dependencies and acceptance gates, read
[release readiness](./release-readiness.md). M8 is [rejected and archived](./history/m8-recipes-rejected.md).
Use the [current handoff](./next-milestone.md) for work status. This table grants
no implementation or publication authorization and adds no requirement fields.

## When a decision seems missing

Check the actual owning rule and its linked dependencies before treating an
omission in this table as a specification gap. Use
[design notes](./design-notes/service-storage-semantic-baseline.md) for historical
decision context, then follow the current [ServiceStorage execution](../spec/execution/m6-5-service-storage-execution.md),
[retirement](../spec/execution/m7-instance-retirement.md), and
[persistence](../spec/persistence/persistence-baseline.md) contracts for their representations. Record unresolved
scope or design decisions in the next-milestone handoff and bring genuine
semantic conflicts back to review.

## Verification route

After identifying the owning rules, read [implementation guidance](./implementation-guidance.md)
for the approved boundaries on implementation choices, then select verification below.

Use [the verification policy](./development-and-verification.md). In an authorized
environment, the shared full entry point is `cargo xtask ci`. The documentation
entry is `cargo xtask docs-build`; it runs document checks and the normal website
build, including text publication. No command here grants machine or deployment
permission, and no documentation-only check proves product runtime coverage.
