---
title: Observable behavior
---

# Observable behavior

**Status: Informative navigation.**

These are behavior contracts. Use the user guides for procedures and the current
handoff for implementation availability.

- [Managed Object Lifecycle and GC](./managed-object-lifecycle.md): Deletion, Artifact delivery, retention constraints, and foreground garbage collection.

- [Actions, Plans, and Runs](./actions-plans-and-runs.md): Understand observable Action, Plan, and Run behavior before choosing internal execution mechanisms.
- [Command and Output Reference](./command-and-output-reference.md): Look up command behavior and approved spelling. Sections explicitly marked as open work do not authorize inventing new flags or output formats.
- [Inputs, Secrets, and Readiness](./inputs-secrets-and-readiness.md): Distinguish a legal Instance from one that is ready for an operation, and preserve configuration and Secret disclosure boundaries.
- [M4 Runtime Capabilities](./m4-runtime-capabilities.md): Read the exact inclusive limits for each Snapshot acquisition and import profile. A limit from one profile does not replace another profile's checks.
- [M4 Snapshot Commands](./m4-snapshot-command-reference.md): Capture, Restore, import, export, and verification. Snapshot deletion is owned by the lifecycle contract above.
- [Packages, Revisions, and Instances](./packages-revisions-and-instances.md): Package lineage, exact Revisions, long-lived Instances, configuration readiness, and lifecycle boundaries.
- [Snapshots, Migration, and Recovery](./snapshots-migrations-and-recovery.md): Observable Snapshot, Migration, and recovery behavior, with links to the owning operation contracts.

- [M5 Migration Commands](./m5-migration-command-reference.md): Path-ID discovery/selection, read-only planning, target-qualified files, and declarative/Hook-backed execution.

The [M6.5 ServiceStorage commands](./m6-5-service-storage-command-reference.md)
are implemented human CLI spellings for contract inspection, point-in-time
observation and authorized path disclosure, not generic service-content editing.

Return to the [specification map](../index.md). For current runtime support and
next-milestone boundaries, see [the development entry](../../development/next-milestone.md).

- [Execution diagnostics](./execution-diagnostics.md): accepted Hook evidence, bounded retention and explicit completeness.
