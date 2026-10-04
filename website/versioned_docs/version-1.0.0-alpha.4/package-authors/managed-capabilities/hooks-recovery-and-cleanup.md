---
title: Integrate Hooks, recovery, and Cleanup
---

# Integrate Hooks, recovery, and Cleanup

Start with the executable [Shell Hook tutorial](../fundamentals/authoring-model.md).
Use the canonical protocol only when implementing a direct/interpreter Hook.
You need a validated launch declaration and a disposable environment. This is
shared integration guidance; declaring Cleanup does not require running retirement
against a real service while learning.

## Respect the Session boundary

Read only granted Inputs and service authorities. Use Workspace for temporary
files. Access paths are execution-scoped; do not persist them as resource IDs.
Hooks are trusted host programs. These declarations do not provide OS isolation.

For Shell Loader scripts, invoke helpers through `PACTRUN_EXECUTABLE`. Do not
reconnect to the Core endpoint or reconstruct Session setup from environment
variables. Helper output can be JSON or paths; parse the documented type.

## Outputs and diagnostics

Allocate or copy into a declared output slot, then explicitly register it:

```text
pactrun hook output report --file <workspace-file>
pactrun hook output-register report
```

These commands are valid only inside the admitted Loader script with a declared
`report` output. Copying a file into Workspace does not retain an Artifact.

Use the diagnostic helper with a canonical JSON file when structured diagnostics
are needed. Treat stdout/stderr and retained diagnostics as potential disclosure
channels. Never emit Secret values, sensitive defaults, or value-derived hashes.

## Risk and recovery

For operations that use managed risk, wait for acknowledged risk entry before
performing the associated external work. Resolve risk only after verifying the
service's coherent state. Do not equate successful file writes with a repaired
service. Abnormal loss or unresolved risk cannot become success.

A failure may leave external effects. Document how an operator should inspect,
repair, and verify those effects before acknowledging manual recovery. A recovery
override cannot waive unrelated readiness or authority checks.

## Cleanup

Declare Cleanup when retirement requires Pack-specific service work. Document
what it stops or deletes, what remains, and how to diagnose a failed attempt.
Test normal completion, failure, cancellation, and interrupted execution against
disposable resources. Cleanup confirmation by an operator is an assertion of
completed work, not an automatic second attempt.

### Runtime artifacts are part of the promised Cleanup lifecycle

Stopping a daemon or removing its container may leave Unix sockets or FIFOs in
ServiceStorage. Current Linux finalization accepts regular files and directories;
it refuses other entry kinds rather than treating them as ordinary files. Do not
assume caller ownership alone makes every object reclaimable by the finalizer.
Where your Pack creates such artifacts, its Cleanup must handle them before
reporting successful completion. This does not require Packs that do not need
Cleanup, Snapshot or Migration to declare those optional capabilities.

For a Pack-owned Unix admin socket, use this controlled lifecycle test:

1. Start the service against disposable ServiceStorage; verify its admin socket.
2. Stop it and wait for the service to exit. Check whether the socket remains.
3. Have Cleanup qualify the exact expected socket: owned scope, expected type
   and ownership, no symlink traversal, and no live listener. Reject ambiguous
   or unexpected objects rather than recursively deleting an arbitrary path.
4. Remove only that qualified stale socket, then report Cleanup completion.
5. Verify **both** successful Hook completion and successful overall deletion.
   Also test an active socket, wrong object type and interrupted Cleanup.

A connection refusal alone is not an atomic ownership/deletion guarantee.
A check followed by pathname unlink can race with replacement; a cooperative
test fixture is not proof of safety against malicious concurrent writers. Choose
an isolation/coordination strategy appropriate to the actual service and retain
Pactrun's storage authority boundary. Do not copy an arbitrary privileged cleanup
helper or expose the full admin API merely to simplify retirement.

The alpha.2 Caddy evaluation left a socket after successful Cleanup; finalization
then failed. A new Pack Revision that cleaned its stale socket passed normal
retirement in a fresh controlled case. Fixing a still-managed Instance by
Migration **before** retirement is not a repair recipe for an already partially
deleted Instance. Completed Cleanup is not replayed on finalization-only retry.
Abandon/handoff preserves surviving bytes, not a complete service backup. See
[retirement guidance](../../guides/retirement.md#when-cleanup-succeeds-but-deletion-fails).

## Direct protocol implementations

Implement framing, Session startup, readiness, requests, completion, cancellation,
and terminal behavior exactly as the [Hook protocol](../reference/hook-protocol.md)
defines. Use its examples and error rules; ordinary process exit is insufficient.
Test malformed messages and loss of the connection as well as success.

**Next choice:** return to the [author guide](../index.md). Add Snapshot or Migration
only if your Pack needs it; they are independent examples, not required follow-ups.

More detail: [Pack fields and values](../reference/pack-fields.md).

<details>
<summary>Maintainer sources (optional)</summary>

Contracts: [Shell Loader](../../spec/contracts/shell-loader.md),
[Hook lifecycle](../../spec/contracts/hooks-recovery-and-cleanup.md), and
[recovery](../../spec/execution/recovery-and-reconciliation.md).

</details>
