---
title: Contract Catalog
---

# Contract catalog

**Status: Informative index; each linked document retains its original status.**

This catalog distinguishes normative contracts from implementation availability.
Status descriptions are copied from the owning pages and checked during the
website build. They do not constitute additional requirements.

| Contract | Original status |
| --- | --- |
| [Persistence baseline](persistence/persistence-baseline.md) | Approved E normative baseline, 1.0-alpha.1; implementation verified for local E delivery. |
| [Revision Canonical Baseline](contracts/revision-canonical.md) | Approved E normative baseline, 1.0-alpha.1; implementation verified for local E delivery. |
| [Pack Source Baseline](contracts/pack-source.md) | Approved E normative baseline, 1.0-alpha.1; implementation verified for local E delivery. |
| [Hook Protocol Baseline](contracts/hook-protocol.md) | Approved E normative baseline, 1.0-alpha.1; implementation verified for local E delivery. |
| [Snapshot Integrity Baseline](contracts/snapshot-integrity.md) | Approved E normative baseline, 1.0-alpha.1; implementation verified for local E delivery. |
| [Snapshot Bundle Baseline](contracts/snapshot-bundle.md) | Approved E normative baseline, 1.0-alpha.1; implementation verified for local E delivery. |
| [Pack Distribution Baseline](contracts/pack-distribution.md) | Approved E normative baseline, 1.0-alpha.1; implementation verified for local E delivery. |
| [Execution Diagnostics](behavior/execution-diagnostics.md) | Implemented and verified normative behavior; integration is tracked separately. |
| [Shell Loader](contracts/shell-loader.md) | Implemented normative adapter contract; non-Frozen. |
| [Product Versioning and Compatibility](foundations/product-versioning-and-compatibility.md) | Approved formal-release compatibility policy; E alpha mechanisms implemented, final qualification tracked in the E ledger. |
| [Actions, Plans, and Runs](behavior/actions-plans-and-runs.md) | Normative product behavior specification. |
| [Command and Output Reference](behavior/command-and-output-reference.md) | Normative product behavior specification. Exact option spelling remains open specification work where explicitly noted. |
| [Managed Object Lifecycle and GC](behavior/managed-object-lifecycle.md) | Implemented normative contract; verification and integration are tracked separately. |
| [Inputs, Secrets, and Readiness](behavior/inputs-secrets-and-readiness.md) | Normative product behavior specification except where linked to an owning requirement. |
| [M4 Runtime Capabilities](behavior/m4-runtime-capabilities.md) | Approved normative structural capabilities and checked byte accounting; revised on 2026-09-17. |
| [M4 Snapshot Commands](behavior/m4-snapshot-command-reference.md) | Approved normative CLI, implemented and integrated into develop. Verification and milestone integration are recorded in the M4 execution record. |
| [Packages, Revisions, and Instances](behavior/packages-revisions-and-instances.md) | Normative product behavior specification except where marked informative. |
| [Snapshots, Migration, and Recovery](behavior/snapshots-migrations-and-recovery.md) | Normative product behavior specification. |
| [Actions, Inputs, and Parameters](contracts/actions-inputs-and-parameters.md) | Normative Package contract specification. |
| [Authoring Model](contracts/authoring-model.md) | Normative Package contract specification. |
| [Error Taxonomy V1](contracts/error-taxonomy-v1.md) | Frozen normative architecture specification. |
| [Hooks, Recovery, and Cleanup](contracts/hooks-recovery-and-cleanup.md) | Normative Package contract specification. |
| [Migrations](contracts/migrations.md) | Normative Package contract specification. |
| [Runtime Content and Retired Recipe Design](contracts/recipes-and-runtime-content.md) | Normative Package contract specification. |
| [Snapshots and Managed Data](contracts/snapshots-and-managed-data.md) | Normative Package contract specification. |
| [Execution and Concurrency](execution/execution-and-concurrency.md) | Normative architecture. |
| [M4 Snapshot Lifecycle Approval Baseline](execution/m4-snapshot-lifecycle-approval-baseline.md) | Approved normative implementation boundary. M4 implementation and verification are complete and integrated into develop. |
| [Recovery and Reconciliation](execution/recovery-and-reconciliation.md) | Normative architecture. |
| [Identity and State](foundations/identity-and-state.md) | Normative architecture. |
| [Resources and Versioning](foundations/resources-and-versioning.md) | Normative architecture. |
| [System Model](foundations/system-model.md) | Normative architecture. |

## M5 Managed Input Migration

| Contract | Status |
| --- | --- |
| [M5 Migration Execution](execution/m5-migration-execution.md) | Implemented normative M5 Managed Input execution contract. |
| [M5 Migration Commands](behavior/m5-migration-command-reference.md) | Implemented normative Migration CLI. |

## M6.5 contracts {#proposed-m65-s0-designs-not-active-contracts}

The historical S0 anchor is retained for existing links. The M6.5 design was
approved on 2026-09-15, including mapped-source consumption. Its original
format activation and runtime integration are recorded in the
[M6.5 delivery record](../development/m6-5-implementation-status.md). Current
Revision, Hook and Persistence formats are owned by the complete E baselines
listed above; historical format labels are not a current support matrix.

| Contract | Status |
| --- | --- |
| [M6.5 Execution](execution/m6-5-service-storage-execution.md) | Implemented normative ServiceStorage execution contract. |
| [M6.5 Commands](behavior/m6-5-service-storage-command-reference.md) | Implemented normative human CLI contract. Non-Frozen human output. |

Use the [current handoff](../development/next-milestone.md) to distinguish
current availability from earlier activation gates. The
[Persistence baseline](./persistence/persistence-baseline.md) defines present
support and refusal behavior. This index does not authorize development-era
readers, upgrade chains or new runtime work.

## CLI presentation contract

| Contract | Status |
| --- | --- |
| [CLI JSON V1](contracts/cli-machine-interface.md) | Approved normative versioned CLI presentation contract. |
| [CLI Object ID Selectors](contracts/cli-id-selectors.md) | Approved CLI contract, 2026-09-23. |

## M7 approved implementation contracts

M7 is implemented, verified and integrated into local develop, but not published.
See the [acceptance record](../development/m7-implementation-status.md).
The [current handoff](../development/next-milestone.md),
[Persistence baseline](./persistence/persistence-baseline.md), and
[E ledger](../development/e-implementation-status.md) identify current support
and qualification. Earlier integration records retain their development-era
format history; they do not authorize those readers or upgrade chains.

| Contract | Status |
| --- | --- |
| [M7 Instance retirement](execution/m7-instance-retirement.md) | Approved M7 contract; runtime activation and verification are separate. |
