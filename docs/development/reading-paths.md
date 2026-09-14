---
title: Development Reading Paths
---

# Read by development task

**Status: Informative navigation.** Read the owning contracts, not just this table.
Within each page, use the reading map and section headings to narrow the context.

| Task | Read first | Then check |
| --- | --- | --- |
| Change the object or ownership model | [System model](../spec/foundations/system-model.md), [identity](../spec/foundations/identity-and-state.md) | [Resources and versions](../spec/foundations/resources-and-versioning.md), public behavior affected by the change |
| Implement installation or Instance management | [Resource behavior](../spec/behavior/packages-revisions-and-instances.md), [source format](../spec/contracts/pack-source-yaml-v1.md) | [Core identity](../spec/contracts/revision-core-format-v1.md), [current persistence](../spec/persistence/persistence-schema-v6.md) |
| Implement an Action or execution boundary | [Execution](../spec/execution/execution-and-concurrency.md), [Action behavior](../spec/behavior/actions-plans-and-runs.md) | [Hook wire](../spec/contracts/hook-protocol-v1.md), [recovery](../spec/execution/recovery-and-reconciliation.md), [shared M4 boundary](../spec/execution/m4-snapshot-lifecycle-approval-baseline.md) |
| Change Input or Secret handling | [Input behavior](../spec/behavior/inputs-secrets-and-readiness.md), [declarations](../spec/contracts/actions-inputs-and-parameters.md) | [Identity and state](../spec/foundations/identity-and-state.md), authority and disclosure rules in [Hook semantics](../spec/contracts/hooks-recovery-and-cleanup.md) |
| Work on Snapshots | [Snapshot semantics](../spec/contracts/snapshots-and-managed-data.md), [behavior](../spec/behavior/snapshots-migrations-and-recovery.md) | [Integrity V1](../spec/contracts/snapshot-integrity-format-v1.md), [V2](../spec/contracts/snapshot-integrity-format-v2.md), [bundle](../spec/contracts/snapshot-bundle-v1.md), [limits](../spec/behavior/m4-runtime-capabilities.md) |
| Maintain M5 Migration | [Migration contract](../spec/contracts/migrations.md), [M5 closeout](./m5-implementation-status.md) | [Execution](../spec/execution/m5-migration-execution.md), [Hook contexts](../spec/contracts/hook-protocol-v1.md), [recovery](../spec/execution/recovery-and-reconciliation.md), [current schema](../spec/persistence/persistence-schema-v6.md) |
| Maintain M6 recovery | [M6 bounded baseline](./design-notes/m6-recovery-implementation-baseline.md), [recovery](../spec/execution/recovery-and-reconciliation.md) | [M6 closeout](./m6-implementation-status.md), [staged alignment](./design-notes/service-storage-staged-design-alignment.md), existing M3-M5 evidence and exact publication boundaries |
| Design M6.5 ServiceStorage and M7 lifecycle | [Closed semantics](./design-notes/service-storage-semantic-baseline.md), [staged alignment](./design-notes/service-storage-staged-design-alignment.md) | [M6.5 gate](./implementation-roadmap.md#m65---servicestorage), [Cleanup boundary](../spec/behavior/snapshots-migrations-and-recovery.md#pr-req-0247---cleanup-finalization-and-abandonment), future representation approval before implementation |
| Change CLI or diagnostics | [Command contract](../spec/behavior/command-and-output-reference.md), [Snapshot commands](../spec/behavior/m4-snapshot-command-reference.md) | [Error identity](../spec/contracts/error-taxonomy-v1.md); distinguish specified, open, and implemented spellings |
| Change persistence or upgrade behavior | [V6](../spec/persistence/persistence-schema-v6.md), [identity/state](../spec/foundations/identity-and-state.md) | Earlier [schema contracts](../spec/persistence/index.md), [recovery](../spec/execution/recovery-and-reconciliation.md), exact crash boundaries |

## When a decision seems missing

Check the actual owning rule and its linked dependencies before treating an
omission in this table as a specification gap. Use
[design notes](./design-notes/service-storage-semantic-baseline.md) to navigate
closed semantics, not to invent their deferred representation. Record unresolved
scope or design decisions in the next-milestone handoff and bring genuine
semantic conflicts back to review.

## Verification route

Use [the verification policy](./development-and-verification.md). In an authorized
environment, the shared full entry point is `cargo xtask ci`. The documentation
entry is `cargo xtask docs-build`; it runs document checks and the normal website
build, including text publication. No command here grants machine or deployment
permission, and no documentation-only check proves product runtime coverage.
