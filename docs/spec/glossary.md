---
title: Shared Vocabulary
---

# Shared vocabulary

**Status: Informative reading aid, not an independent set of definitions.**

This is for readers of the specification, not a beginner usage guide. Follow the
owning contract for exact identity, representation, lifetime, and failure rules.

| Term | Reading aid | Owning contract |
| --- | --- | --- |
| Package / Pack | Stable product lineage; Pack is the supported package kind | [System model](./foundations/system-model.md), [identity](./foundations/identity-and-state.md) |
| Revision | Exact immutable operational definition and owned runtime content | [Resource model](./behavior/packages-revisions-and-instances.md), [Core format](./contracts/revision-core-format-v1.md) |
| Instance | Long-lived managed object with stable identity and an active Revision | [Identity and state](./foundations/identity-and-state.md) |
| Managed Input Binding | Detached value whose authoritative bytes are owned by Pactrun | [Identity and state](./foundations/identity-and-state.md), [Input behavior](./behavior/inputs-secrets-and-readiness.md) |
| Secret | Protection and disclosure rules, not a claim that Pactrun is a secret vault | [System model](./foundations/system-model.md), [Input behavior](./behavior/inputs-secrets-and-readiness.md) |
| ServiceStorage / Managed Service Resource | Provided storage lifetime and separately service-authoritative live state; runtime remains deferred | [Identity and state](./foundations/identity-and-state.md), [Migration](./contracts/migrations.md) |
| Workspace | Execution-scoped scratch under granted Hook authority | [Hooks and recovery](./contracts/hooks-recovery-and-cleanup.md) |
| Plan | Compiled intent and checks, not a durable replay log | [Execution](./execution/execution-and-concurrency.md) |
| Run | Durable execution-attempt record, distinct from an ephemeral Plan | [Execution](./execution/execution-and-concurrency.md), [behavior](./behavior/actions-plans-and-runs.md) |
| Hook | Trusted service-specific code operating through the defined context and authority | [Hook semantics](./contracts/hooks-recovery-and-cleanup.md), [wire](./contracts/hook-protocol-v1.md) |
| Snapshot | Immutable recovery representation with its own identity and lifetime | [Snapshot contracts](./contracts/snapshots-and-managed-data.md) |
| Run Artifact | Managed output with retention separate from Workspace and Snapshot lifetimes | [Resources](./foundations/resources-and-versioning.md) |
| Migration | Target-owned Revision transition; Managed Input transitions are not atomic service-database migration | [Migration contract](./contracts/migrations.md) |
| Recovery / reconciliation | Decisions based on durable ownership and risk evidence, not automatic Hook replay | [Recovery](./execution/recovery-and-reconciliation.md) |

Read [the specification map](./index.md) for authority and status rules.
