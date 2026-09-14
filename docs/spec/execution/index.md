---
title: Execution and recovery
---

# Execution and recovery

**Status: Informative navigation.**

Read these boundaries before choosing a transaction, ownership, retry, or recovery mechanism.

- [Execution and Concurrency](./execution-and-concurrency.md): Follow the boundary between resolving intent, compiling a Plan, accepting a Run, admitting work, and executing under ownership and concurrency checks.
- [M4 Snapshot Lifecycle Approval Baseline](./m4-snapshot-lifecycle-approval-baseline.md): This is a normative operation boundary, not merely a historical plan. Read it when extending the shared Action, Capture, and Restore acceptance and recovery substrate.
- [Recovery and Reconciliation](./recovery-and-reconciliation.md): Determine what durable evidence permits after interruption or owner loss. Do not infer that recovering Pactrun state rolls back service-owned side effects.

- [M5 Migration Execution](./m5-migration-execution.md): Exact paths, whole-chain binding preflight, staged operator/Hook values, per-edge commits and bounded reconciliation.

Return to the [specification map](../index.md). For current runtime support and
next-milestone boundaries, see [the development entry](../../development/next-milestone.md).
