---
title: Persistence Schema V11
---

# PersistenceSchemaV11

**Status: Implemented and verified normative internal contract; non-Frozen.**

<!-- spec-navigation:start -->
Read [V10](./persistence-schema-v10.md) and [execution diagnostics](../behavior/execution-diagnostics.md).
Delivery is tracked in the [implementation status](../../development/execution-diagnostics-observability-status.md).
<!-- spec-navigation:end -->

### PR-REQ-0352 - Run evidence persistence and compatibility

New stores MUST use V11. Explicit upgrade accepts exact V8, V9 or V10, applying
missing ladder steps in one transaction under the existing exclusive coordination
guard and writer-quiescence checks. Unsupported/foreign/drifted/newer schemas are
refused; ordinary operations MUST NOT silently upgrade. Old Run data is unchanged,
and absence of a collection row denotes Legacy/not-collected evidence.

New Run acceptance MUST record retention policy with the Run. Diagnostic writes
MUST revalidate writable admission and never recreate a missing collection. They
are separate from outcome/managed-output commit success and MUST NOT change those
boundaries. Evidence rows and state MUST be read in one snapshot; managed Run
inspection includes evidence in the same snapshot as Run facts and current guard.
Run deletion cascades to its evidence, without a new retention blocker or GC root.

Discriminators are exact: kind 0 is diagnostic, 1 completion, and 2 protocol-error.
Severity is null outside diagnostics; 0 is info, 1 warning and 2 error. Completion
status is null outside completion; 0 is success and 1 failure. Boolean flags use
0/1. A null message is absent and an empty string is present-empty. Truncated
messages concatenate retained prefix and suffix; truncated_prefix_bytes records
the UTF-8-safe split, and is zero for untruncated text. The stage description
contains Pactrun-owned operation/revision context and a Run-local Hook ordinal.
It is evidence for inspection, never an authority input.

A null receipt timestamp means the host wall clock was unavailable; it MUST NOT
be fabricated as the Unix epoch. Sequence remains the ordering authority.

Started means that collection initialization committed, not that a Hook launched.
Closed means that collection drained, not that the operation succeeded. An
uninitialized collection cannot prove that no Hook text was emitted.

Each batch MUST atomically update retained events, observed sequence and collection
status. A crash preserves committed evidence, not a claim of complete coverage.
A recording failure that cannot itself be persisted leaves an unclosed collection.
Missing events are not evidence that nothing was emitted. Disabled text retention
MUST NOT persist Hook code/message bytes, including through debug/error paths.
This is not an encrypted vault, backup facility or secure-erasure promise.

```sql
CREATE TABLE run_diagnostic_collections (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    retain_text INTEGER NOT NULL CHECK(retain_text IN (0, 1)),
    started INTEGER NOT NULL DEFAULT 0 CHECK(started IN (0, 1)),
    closed INTEGER NOT NULL DEFAULT 0 CHECK(closed IN (0, 1)),
    observed INTEGER NOT NULL DEFAULT 0 CHECK(observed >= 0),
    failed INTEGER NOT NULL DEFAULT 0 CHECK(failed IN (0, 1)),
    PRIMARY KEY(run_id),
    FOREIGN KEY(run_id) REFERENCES runs(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
CREATE TABLE run_diagnostic_events (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    sequence INTEGER NOT NULL CHECK(sequence > 0),
    received_at_unix_ms INTEGER CHECK(received_at_unix_ms >= 0),
    stage TEXT NOT NULL,
    kind INTEGER NOT NULL CHECK(kind IN (0, 1, 2)),
    severity INTEGER CHECK(severity IN (0, 1, 2)),
    code TEXT,
    message TEXT,
    truncated INTEGER NOT NULL CHECK(truncated IN (0, 1)),
    truncated_prefix_bytes INTEGER NOT NULL CHECK(truncated_prefix_bytes BETWEEN 0 AND 32768),
    completion_status INTEGER CHECK(completion_status IN (0, 1)),
    PRIMARY KEY(run_id, sequence),
    FOREIGN KEY(run_id) REFERENCES run_diagnostic_collections(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
DROP TABLE writable_admissions;
CREATE TABLE writable_admissions (
    owner_session BLOB NOT NULL CHECK(length(owner_session) = 40),
    admitted_schema_version INTEGER NOT NULL CHECK(admitted_schema_version = 11),
    PRIMARY KEY(owner_session)
) STRICT, WITHOUT ROWID;
```

**Verification: PR-TEST-0521, PR-TEST-0522, PR-TEST-0523, PR-TEST-0524, PR-TEST-0526, PR-TEST-0527.**
