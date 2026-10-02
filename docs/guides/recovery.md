---
title: Inspect and recover after failure
---

# Inspect and recover after failure

A failure can leave external effects or an earlier Migration edge committed.
Stop automatic retries until you know the Run outcome and service state.
Use this page after a failure, suspected owner loss, or a recovery guard. It is
not a routine next step after a successful operation; return to the
[task index](./index.md) when no recovery work is indicated.

## Inspect without changing state

```text
pactrun instance show demo
pactrun run list demo
pactrun run show <run-id>
```

Read the committed Revision, recovery guard, terminal outcome, and available
diagnostics. Do not infer success from an exit message or progress event alone.
Use the Pack author's procedure to inspect the actual service and backups.

## Reconcile a lost execution owner

```text
pactrun run reconcile
```

This command is store-wide: it examines eligible retained Runs throughout the
selected store, not only `demo` or the Run you most recently inspected. Check the
selected executable and storage root first. It leaves live or
uncertain owners alone and handles confirmed owner loss; it does not replay a
Hook, repair a service, infer that Cleanup finished, or salvage staged outputs.

## Acknowledge manual recovery

After external inspection and repair establish the required coherent state,
inspect the Instance again and copy its complete state-version token:

```text
pactrun instance resolve-manual-recovery demo --if-version <token>
```

This is your assertion that the required work has been completed, not a repair
operation. A stale token is a conflict: reinspect rather than automatically
substituting a new token. A recovery override bypasses only its named trust guard,
not identity, ownership, readiness, capacity, or other checks.

## Migration blocked by a recovery guard

Inspect the triggering Run identified by the current recovery guard, not only
the newly refused operation. A refusal can have an accepted Run whose admission
failed before a Hook started. JSON inspection supplies `current_recovery_guard`
and, when available, `migration_progress`; do not assume every failure has the
same result wrapper or that a null progress means all earlier work rolled back.
The reported guard is current state, not a preserved copy of the historical
blocking guard; it may have changed or been cleared since that Run failed.
Check the last committed boundary and current Instance, then follow the Pack's
service-specific recovery procedure. Neither an explanatory error nor a
successful repair Action automatically authorizes retry or clears an old guard.

## Failed deletion

Use [retirement](./retirement.md) and its
[complete recovery options](../pactrun-users/reference/retirement-details.md).
A deletion completion assertion needs the matching Instance ID, attempt Run ID,
and state-version token. Do not retry Cleanup or confirm completion merely
because a process exited. Abandonment preserves data but does not stop the service.

For complete command behavior see [command details](../pactrun-users/reference/command-details.md).
