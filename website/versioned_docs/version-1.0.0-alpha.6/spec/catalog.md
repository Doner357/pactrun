---
title: Specification Reference Index
---

# Specification Reference Index

## System and Core Concepts

- [System Model](./core/system-model.md)
- [Packages, Revisions, and Instances](./core/objects.md)
- [Vocabulary](./core/vocabulary.md)

## Packages and Revisions

- [Pack Authoring Model](./packages/authoring.md)
- [Pack Source Format](./packages/source-format.md)
- [Package and Revision Identity](./packages/identity.md)
- [Local Names and References](./packages/local-names.md)
- [Revision Format and Identity Encoding](./packages/revision-format.md)
- [Runtime Content](./packages/runtime-content.md)
- [Descriptions and Provenance](./packages/metadata.md)
- [Pack Distribution Format](./packages/distribution.md)
- [Identity and Metadata During Import](./packages/import-identity.md)

## Instances and Data

- [Instance Identity and State](./instances/identity-state.md)
- [Input and Resource Declarations](./instances/input-declarations.md)
- [Input Bindings and Protection](./instances/bindings.md)
- [Inputs, Secrets, and Readiness](./instances/inputs-secrets.md)
- [Service Resource Ownership and Identity](./instances/service-resources.md)
- [ServiceStorage Execution](./instances/service-storage.md)
- [Service Resource Commands](./instances/resource-commands.md)

## Operations and Execution

- [Action Definitions](./operations/action-definitions.md)
- [Invocation Parameters](./operations/parameters.md)
- [Actions, Plans, and Runs](./operations/actions-plans-runs.md)
- [Execution and Concurrency](./operations/execution.md)
- [Execution Diagnostics](./operations/diagnostics.md)

## Snapshots and Restore

- [Snapshot Capture and Restore](./snapshots/behavior.md)
- [Snapshot Definition and Managed Data](./snapshots/definition.md)
- [Snapshot Limits and Capabilities](./snapshots/limits.md)
- [Snapshot Capture and Restore Execution](./snapshots/execution.md)
- [Snapshot Commands](./snapshots/commands.md)
- [Snapshot Integrity Format](./snapshots/integrity.md)
- [Snapshot Bundle Format](./snapshots/bundles.md)
- [Snapshot Import Identity](./snapshots/import-identity.md)

## Migration

- [Revision Transitions](./migrations/behavior.md)
- [Migration Definition](./migrations/definition.md)
- [Migration Execution](./migrations/execution.md)
- [Migration Commands](./migrations/commands.md)

## Lifecycle and Recovery

- [Resource Lifetimes](./lifecycle/resource-lifetimes.md)
- [Object Deletion and Garbage Collection](./lifecycle/objects-gc.md)
- [Failure and Manual Recovery](./lifecycle/failure.md)
- [Recovery and Reconciliation](./lifecycle/recovery.md)
- [Cleanup and Abandonment](./lifecycle/deletion.md)
- [Instance Retirement](./lifecycle/retirement.md)

## Hooks and Integration Interfaces

- [Hook Execution and Authority](./interfaces/hooks.md)
- [Hook Protocol](./interfaces/hook-protocol.md)
- [Shell Loader](./interfaces/shell-loader.md)
- [Command and Output Reference](./interfaces/commands.md)
- [Object ID Selectors](./interfaces/id-selectors.md)
- [CLI Machine Interface](./interfaces/machine-output.md)
- [Error Taxonomy](./interfaces/errors.md)

## Compatibility and Storage

- [Product and Format Compatibility](./storage/compatibility.md)
- [Format Domains and Support](./storage/format-domains.md)
- [Store Preparation and Upgrades](./storage/store-opening.md)
- [Immutable Runtime Blob Storage](./storage/runtime-content.md)
- [Revision Persistence and Publication](./storage/revision-records.md)
- [Persistence Schema](./persistence/persistence-baseline.md)
