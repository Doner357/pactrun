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
| [Product Versioning and Compatibility](foundations/product-versioning-and-compatibility.md) | Approved formal-release compatibility design; implementation and baseline reset pending. |
| [Actions, Plans, and Runs](behavior/actions-plans-and-runs.md) | Normative product behavior specification. |
| [Command and Output Reference](behavior/command-and-output-reference.md) | Normative product behavior specification. Exact option spelling remains open specification work where explicitly noted. |
| [Inputs, Secrets, and Readiness](behavior/inputs-secrets-and-readiness.md) | Normative product behavior specification except where linked to an owning requirement. |
| [M4 Runtime Capabilities](behavior/m4-runtime-capabilities.md) | Approved normative fixed build capabilities. |
| [M4 Snapshot Commands](behavior/m4-snapshot-command-reference.md) | Approved normative CLI, implemented and integrated into develop. Verification and milestone integration are recorded in the M4 execution record. |
| [Packages, Revisions, and Instances](behavior/packages-revisions-and-instances.md) | Normative product behavior specification except where marked informative. |
| [Snapshots, Migration, and Recovery](behavior/snapshots-migrations-and-recovery.md) | Normative product behavior specification. |
| [Actions, Inputs, and Parameters](contracts/actions-inputs-and-parameters.md) | Normative Package contract specification. |
| [Authoring Model](contracts/authoring-model.md) | Normative Package contract specification. |
| [Error Taxonomy V1](contracts/error-taxonomy-v1.md) | Frozen normative architecture specification. |
| [Hook Protocol V1](contracts/hook-protocol-v1.md) | Frozen normative Package contract specification. |
| [Hooks, Recovery, and Cleanup](contracts/hooks-recovery-and-cleanup.md) | Normative Package contract specification. |
| [Migrations](contracts/migrations.md) | Normative Package contract specification. |
| [Pack Source YAML V1](contracts/pack-source-yaml-v1.md) | Candidate normative Package authoring contract; versioned, non-Frozen, and not a public compatibility promise. |
| [Runtime Content and Retired Recipe Design](contracts/recipes-and-runtime-content.md) | Normative Package contract specification. |
| [Revision Core Format V1](contracts/revision-core-format-v1.md) | Frozen normative Package contract specification. |
| [Snapshot Bundle V1](contracts/snapshot-bundle-v1.md) | Normative transport contract. Adapter, application services and human CLI are implemented, verified and integrated into develop with M4. |
| [Snapshot Integrity Format V1](contracts/snapshot-integrity-format-v1.md) | Frozen normative Package contract specification. |
| [Snapshot Integrity Format V2](contracts/snapshot-integrity-format-v2.md) | Frozen normative Package contract specification. |
| [Snapshots and Managed Data](contracts/snapshots-and-managed-data.md) | Normative Package contract specification. |
| [Execution and Concurrency](execution/execution-and-concurrency.md) | Normative architecture. |
| [M4 Snapshot Lifecycle Approval Baseline](execution/m4-snapshot-lifecycle-approval-baseline.md) | Approved normative implementation boundary. M4 implementation and verification are complete and integrated into develop. |
| [Recovery and Reconciliation](execution/recovery-and-reconciliation.md) | Normative architecture. |
| [Identity and State](foundations/identity-and-state.md) | Normative architecture. |
| [Resources and Versioning](foundations/resources-and-versioning.md) | Normative architecture. |
| [System Model](foundations/system-model.md) | Normative architecture. |
| [Persistence Schema V2](persistence/persistence-schema-v2.md) | Implemented M1-D internal persistence schema and normative internal contract; superseded as the current schema by PersistenceSchemaV4; non-Frozen, non-public, and not a stable public support contract. |
| [Persistence Schema V3](persistence/persistence-schema-v3.md) | Implemented normative internal persistence contract; non-Frozen and non-public. Superseded as the current schema by PersistenceSchemaV4. |
| [Persistence Schema V4](persistence/persistence-schema-v4.md) | Historical implemented normative internal persistence contract; non-Frozen and non-public. |
| [Persistence Schema V5](persistence/persistence-schema-v5.md) | Implemented normative internal schema, integrated into develop with M4 Snapshot execution; non-Frozen and non-public. |

## M5 Managed Input Migration

| Contract | Status |
| --- | --- |
| [M5 Migration Execution](execution/m5-migration-execution.md) | Implemented normative M5 Managed Input execution contract. |
| [M5 Migration Commands](behavior/m5-migration-command-reference.md) | Implemented normative Migration CLI. |
| [Persistence Schema V6](persistence/persistence-schema-v6.md) | Implemented normative internal schema and explicit V5-to-V6 upgrade. Non-Frozen and non-public. |

## M6.5 contracts {#proposed-m65-s0-designs-not-active-contracts}

The historical S0 anchor is retained for existing links. The M6.5 design was
approved on 2026-09-15, including mapped-source consumption. Core/Hook V2 are
Frozen and the runtime/installer is integrated into local develop; final
verification and integration are recorded separately. The integrated
persistence baseline is V7; V6 is historical. Frozen V1
contracts are unchanged.

| Contract | Status |
| --- | --- |
| [Revision Core V2](contracts/revision-core-format-v2.md) | Frozen normative Package contract specification. |
| [Pack Source YAML V2](contracts/pack-source-yaml-v2.md) | Candidate normative Package authoring contract; versioned, non-Frozen, and not a public compatibility promise. |
| [Hook Protocol V2](contracts/hook-protocol-v2.md) | Frozen normative Package contract specification. |
| [Persistence V7](persistence/persistence-schema-v7.md) | Implemented normative internal schema and explicit V6-to-V7 upgrade. Non-Frozen and non-public. |
| [M6.5 Execution](execution/m6-5-service-storage-execution.md) | Implemented normative ServiceStorage execution contract. |
| [M6.5 Commands](behavior/m6-5-service-storage-command-reference.md) | Implemented normative human CLI contract. Non-Frozen human output. |

For V2 and V3 persistence, references to V4 as a successor record an intermediate
historical baseline. [V7](./persistence/persistence-schema-v7.md) is current in
the integrated M6.5 develop baseline.
Preserve the exact documented upgrade gates rather than inferring one from this
index. See [remaining decisions](../development/next-milestone.md) for deferred
runtime work, not just the historical status on a format page.

## M7 approved implementation contracts

M7 is implemented, verified and integrated into local develop, but not published.
See the [acceptance record](../development/m7-implementation-status.md).
The integrated baseline writes V8; the historical M6.5 integration record is unchanged.

| Contract | Status |
| --- | --- |
| [M7 Instance retirement](execution/m7-instance-retirement.md) | Approved M7 contract; runtime activation and verification are separate. |
| [Persistence V8](persistence/persistence-schema-v8.md) | Approved M7 internal contract; non-Frozen. |
