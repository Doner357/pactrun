---
title: M5 Migration Implementation Baseline
---

# M5 Migration implementation baseline

**Status: Approved scope and implementation decisions, 2026-09-13. Implementation
status and verification are recorded in the linked milestone closeout.**

The approved scope is target-owned Managed Input Migration graphs, chained
execution in one Run, staged bindings, Frozen Migration Hooks, per-edge commits,
and operation-specific interruption and reconciliation.

## Approved decisions

- Resolve a unique simple path automatically; on ambiguity display candidates
  and require a path ID. The approved follow-up replaces --via with stateless
  exact path selection; direct paths need no separate --direct option.
- Keep intrinsic validation, exact-source relational validation, and actual
  binding Admission separate. Never select a path by binding readiness.
- Preserve absence for optional transition sources; declared writers still
  reserve their target. Explicit requirements continue to require presence.
- Use target-digest-qualified operator file inputs and explicit declassification
  authorization. No parameters, stdin bindings, or request-file format.
- Introduce internal V6 with explicit exact-V5 upgrade only.
- Publish the final edge and Run success atomically; intermediate edges leave
  the same Run Running. Never replay or automatically resume a Hook.

Owning rules: [Migration](../../spec/contracts/migrations.md),
[M5 execution](../../spec/execution/m5-migration-execution.md), and
[M5 commands](../../spec/behavior/m5-migration-command-reference.md).

[V6](../../spec/persistence/persistence-baseline.md) now defines implemented
exact DDL and explicit V5 upgrade. Migration uses this same substrate for
declarative and Hook-backed edges without another persistence format.
See [implementation status](../m5-implementation-status.md) for actual progress.

## Delivery gates

1. S0: approved behavior, command, and persistence contracts.
2. S1: graph resolution and typed symbolic transition/preflight logic.
3. S2: V6, upgrade, pins, checkpoints, and atomic edge publication.
4. S3: declarative Migration, operator acquisition, CLI, and readiness.
5. S4: Frozen Migration Sessions and bounded risk/reconciliation.
6. S5: real-process/crash/upgrade regression and bidirectional evidence; full
   cargo xtask ci on the supported persistent test workspace.

Passing an earlier gate does not make the milestone implemented. Verification
must include the final documentation and closeout edits.

## Exclusions

ServiceStorage representation/runtime, generalized M6 recovery, Cleanup/deletion,
Recipes, public Rust APIs, stable machine-output envelopes, and publication
workflow changes remain outside M5. Frozen Core, Hook wire, identities, and
existing error identities remain unchanged. Pactrun commits do not encompass
service-owned bytes.
