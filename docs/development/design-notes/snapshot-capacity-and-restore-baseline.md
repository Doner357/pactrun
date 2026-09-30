---
title: Snapshot Capacity and Restore Baseline
---

# Snapshot capacity and Restore workflow baseline

**Approved: 2026-09-17. Implementation and verification complete; local develop integration subsequently authorized on 2026-09-18. No publication.**

The operator approved continuous S0-S5 delivery without slice-by-slice stops.
Stop only for a new semantic decision, required authorization, or an external
blocker. This is the second scope in the product-completion work order, not a
new M-series milestone. The original approval did not authorize Git integration.
The operator subsequently authorized local commit/merge and closeout on
2026-09-18; push and publication remain separate. See the implementation record.

## Originally approved behavior (storage restriction superseded below)

- Remove fixed service-data byte ceilings across Capture acquisition, distinct
  closure, storage, bundles and Restore expansion. Preserve checked accounting,
  bounded metadata and parser limits, Managed Input limits and Hook frames.
- Preserve Frozen identities, wire formats, database schema, publication
  transactions, recovery and service-authoritative ownership. Resource failure
  is not corruption, success, truncation or permission to repair stored data.
- Add `instance create <name> --revision <reference> --restore-from <snapshot-id>`.
  Revision is explicit and must exactly match the Snapshot producer. Initial
  Input options and combined `--plan` are rejected. Existing Restore parameter,
  timeout and recovery-override options apply.
- Perform read-only Snapshot/Revision/Restore-parameter preflight before Create;
  formal Admission still revalidates. Restore targets the exact created ID.
  Keep the Instance after Restore refusal/failure; report partial completion and
  any Run. No compensation, implicit resume, installation or import is added.

## Delivery and evidence

S0 records contracts and traceability; S1 changes capability policy and bounded
data paths; S2 proves failure and lifecycle safety; S3 adds the composition;
S4 exercises real increasing payloads beyond former ceilings; S5 closes docs,
focused checks and configured-remote full CI. Tests accompany each slice.

Audit acquisition, private staging, chunk persistence, verification, bundle
import/export and Restore for allocations, arithmetic, cancellation, temporary
space, transactional publication and pin/GC interaction. Keep metadata bounded;
never allocate an entire service payload or collect all chunks in memory.

Large-data evidence must read, hash, persist and materialize actual bytes, not
substitute fabricated lengths or sparse allocation. Record memory at increasing
sizes with fixed descriptor count. Inject I/O/storage/commit failures and test
crash/reopen and deletion races. Full CI runs on the persistent remote workspace.

The remote filesystem was expanded to 200 GiB (about 168.8 GiB free at preflight).
Recheck free space before each large run and execute large cases sequentially.
No test or implementation is claimed complete by this approval record.

## Implementation discovery requiring a new decision

Real large-data testing found payload-proportional SQLite WAL-index mapping
growth despite bounded application buffers. The [implementation record](../snapshot-capacity-and-restore-status.md)
records measurements and the stopped acceptance run. The operator subsequently
authorized necessary internal changes, including persistence/schema changes.
The memory guarantee is not narrowed.

## Authorized persistence adjustment

Reuse the existing private immutable SHA-256 blob store and publication/GC guard.
The historical `runtime-content` directory is a physical opaque-byte store; it
also holds Snapshot and restored Input values. Logical ownership stays in the
Snapshot and Instance tables, never in a pathname or Revision provenance.

V10 adds a Snapshot storage discriminator and an optional content digest on
Managed Input payloads. New Snapshot bytes are durable before one metadata
transaction makes the Snapshot visible. Restore publishes fresh Instance-scoped
payload identities referencing immutable bytes, preserving protection and both
conflict tokens. ServiceStorage remains separate service-authoritative state.

Explicit upgrade accepts exact V9 and the previously supported exact V8 under
exclusive coordination and writer quiescence. Legacy inline values remain
readable without a wholesale rewrite. No ordinary command upgrades a store.
GC roots include all new data references; pre-commit orphan blobs become eligible
only after the publisher's guard ends. Frozen identities, wire formats and
external atomic success boundaries do not change. Data-facing errors are redacted.
