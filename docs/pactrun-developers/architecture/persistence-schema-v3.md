---
title: Persistence Schema V3
---

# Persistence Schema V3

**Status: Candidate / implementation-ready normative internal persistence
contract; non-Frozen, non-public, and not yet the implemented schema.**

This page extends the exact implemented
[PersistenceSchemaV2](./persistence-schema-v2.md) for M2. It does not modify
the V1/V2 tables or any Frozen identity or wire format. Until M2 is integrated,
V2 remains the current implemented schema and V3 remains Candidate.

### PR-REQ-0269 - Exact Candidate PersistenceSchemaV3

PersistenceSchemaV3 MUST contain the exact V2 schema followed by exactly the
four tables below, retain application ID `0x50414354`, and set SQLite
`user_version` to `3`. Every table remains `STRICT` and `WITHOUT ROWID`.
Correctness MUST NOT depend on rowid, insertion order, timestamp, query-plan
order, or a performance-only index.

```sql
CREATE TABLE instances (
    instance_id BLOB NOT NULL
        CHECK(length(instance_id) = 16),

    instance_name BLOB NOT NULL
        CHECK(length(instance_name) BETWEEN 1 AND 128),

    active_package_id BLOB NOT NULL
        CHECK(length(active_package_id) = 16),

    active_revision_content_digest BLOB NOT NULL
        CHECK(length(active_revision_content_digest) = 32),

    instance_state_version BLOB NOT NULL
        CHECK(length(instance_state_version) = 16),

    PRIMARY KEY (instance_id),
    UNIQUE (instance_name),
    UNIQUE (instance_state_version),

    FOREIGN KEY (
        active_package_id,
        active_revision_content_digest
    )
    REFERENCES revisions(
        package_id,
        revision_content_digest
    )
    ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE managed_input_payloads (
    instance_id BLOB NOT NULL
        CHECK(length(instance_id) = 16),

    payload_id BLOB NOT NULL
        CHECK(length(payload_id) = 16),

    protection_rank INTEGER NOT NULL
        CHECK(protection_rank IN (0, 1)),

    byte_length INTEGER NOT NULL
        CHECK(byte_length BETWEEN 0 AND 536870912),

    PRIMARY KEY (
        instance_id,
        payload_id
    ),

    FOREIGN KEY (instance_id)
        REFERENCES instances(instance_id)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE managed_input_payload_chunks (
    instance_id BLOB NOT NULL
        CHECK(length(instance_id) = 16),

    payload_id BLOB NOT NULL
        CHECK(length(payload_id) = 16),

    chunk_index INTEGER NOT NULL
        CHECK(chunk_index >= 0),

    chunk_bytes BLOB NOT NULL
        CHECK(length(chunk_bytes) BETWEEN 1 AND 1048576),

    PRIMARY KEY (
        instance_id,
        payload_id,
        chunk_index
    ),

    FOREIGN KEY (
        instance_id,
        payload_id
    )
    REFERENCES managed_input_payloads(
        instance_id,
        payload_id
    )
    ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE managed_input_bindings (
    instance_id BLOB NOT NULL
        CHECK(length(instance_id) = 16),

    input_identity BLOB NOT NULL
        CHECK(length(input_identity) > 0),

    payload_id BLOB NOT NULL
        CHECK(length(payload_id) = 16),

    PRIMARY KEY (
        instance_id,
        input_identity
    ),

    FOREIGN KEY (instance_id)
        REFERENCES instances(instance_id)
        ON DELETE CASCADE,

    FOREIGN KEY (
        instance_id,
        payload_id
    )
    REFERENCES managed_input_payloads(
        instance_id,
        payload_id
    )
    ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

PRAGMA user_version = 3;
```

`instance_name` and `input_identity` store exact UTF-8 bytes as BLOBs. The
repository MUST validate their Domain syntax when writing and loading.
`protection_rank` is the closed semantic rank `Normal = 0`, `Secret = 1` and is
the persisted sticky protection floor for bindings that reference the payload.
`instance_id`, `payload_id`, and `instance_state_version` are independently
generated opaque 128-bit values. A payload identifier MUST be instance-scoped,
MUST NOT be derived from bytes, and MUST NOT be reused for a replacement.

One payload header plus its chunks is the only persistent payload
representation. An empty payload has `byte_length = 0` and no chunk rows. A
non-empty payload has chunk indices contiguous from zero; every non-final chunk
has exactly 1,048,576 bytes, the final chunk has 1 through 1,048,576 bytes, and
their concatenated length equals the header length. The maximum is
536,870,912 bytes. The row-local constraints above reject duplicates,
negative indices, empty chunks, oversize chunks, and oversize headers. Because
SQLite row checks cannot prove cross-row contiguity and aggregate length, the
typed repository MUST validate those invariants before commit and on every
logical load. A gap, non-final short chunk, length mismatch, impossible
protection rank, or invalid Domain string is corruption, not another
representation.

Logical load MUST also compare a binding with the strict-decoded active
Revision. If the Input is active and declared Secret, a referenced Normal
payload is corruption because the required promotion was not durably
published. Active Normal with a Secret payload is valid sticky state, not a
mismatch. An Input identity absent from the active Revision is retained; its
payload rank alone defines effective protection and MUST NOT be compared with a
nonexistent declaration. This is the complete V3 binding/payload protection
validation rule.

Payload headers and chunks are immutable after publication. Replacement MUST
insert a fresh payload and atomically change the binding pointer; it MUST NOT
rewrite an existing payload, deduplicate equal bytes, or persist a content
digest. `Active`, `retained`, `required`, and readiness are derived from the
strict-decoded active Revision and MUST NOT appear as columns. Binding absence
is represented only by no binding row.

Initial publication stores the active declaration's protection. Replacement
stores the stronger of the previous payload protection and the active
declaration. Any operation that promotes an existing binding MUST publish a
fresh Secret payload because it cannot mutate an existing payload header.
Future Migration coordination must establish the same committed invariant
before target publication, without this V3 contract defining M5 execution.

Every deterministic Instance enumeration MUST order by exact
`instance_name`, then `instance_id`, using BLOB byte order. Binding enumeration
MUST order by `input_identity`; chunk reads MUST state `ORDER BY chunk_index`.
Domain code MUST verify SQL/Domain ordering parity after load. Payload lookup
is by the complete `(instance_id, payload_id)` key and never by physical row
order.

The active Revision foreign key prevents deletion while a live Instance refers
to it. Instance deletion remains M7 behavior; the cascades define relational
cleanup only after a separately authorized Instance deletion transaction.
Current bindings, in-progress ExportInput observations, and future durable pin
or recovery references govern payload reachability. V3 does not add those
future reference kinds or a generic GC table.

**Verification: Pending automated coverage.**

### PR-REQ-0270 - Persistence migration to V3

Opening persistence MUST classify the database as pristine, exact V1, exact
V2, exact V3, or inadmissible. Exact versions have Pactrun's application ID,
their matching user version, and the complete schema manifest for that version.
Foreign, unmarked non-empty, partial, drifted, and newer databases MUST be
rejected rather than repaired or guessed.

After establishing the existing WAL and connection profile, Pactrun MUST use
`BEGIN IMMEDIATE` and re-read ownership markers and schema. A pristine database
MUST create exact V3 directly. Exact V1 MUST create the unchanged V2 relations
and then the V3 relations in the same transaction. Exact V2 MUST add only the
four V3 tables. Exact V3 is validate-only. Schema validation MUST compare table
names, columns, declared types, nullability, defaults, checks, primary and
unique keys, foreign keys and delete actions, `STRICT`, `WITHOUT ROWID`, and
every correctness-bearing index or trigger in the version manifest.

The version marker MUST advance to `3` only after the complete resulting schema
has been validated inside the transaction. A failure or crash before commit
leaves the exact previous version; a crash after commit exposes exact V3.
Reopen and retry MUST converge, and concurrent initializers or migrators MUST
use SQLite locking rather than a second cross-process migration lock.

Migration MUST preserve every Package and Revision identity, canonical
Revision component byte string, runtime-content reference, and M1-D metadata
value. It MUST NOT infer an Instance, binding, payload, source observation, or
local installation event from existing rows, and it MUST NOT rewrite Frozen
canonical content.

**Verification: Pending automated coverage.**

## Planned crate-private repository contract

The M2 implementation derives typed operations equivalent to:

```text
create_instance(name, revision, initial_payloads)
list_instances() -> ordered InstanceSummary[]
load_instance(instance) -> InstanceView
list_inputs(instance) -> ordered ManagedInputView[]
set_input(instance, input, expected_version, staged_payload)
delete_input(instance, input, expected_version)
acquire_export_observation(instance, input, authorization) -> staged payload
```

Creation and mutation use `BEGIN IMMEDIATE`. The repository revalidates the
exact active Revision, Input declaration, role, stored/effective protection
invariant, and expected state version inside the transaction. It streams one-
megabyte chunks from transient staging and does not require one giant SQLite
bind buffer. A transaction failure publishes no header, chunk, binding,
Instance, or new state version. The concrete Rust names remain crate-private
and are not a stable API.

## Deferred work

V3 does not define Action execution, durable pins, Snapshot persistence,
Migration execution, recovery state, Instance deletion UX, ServiceStorage,
stable public APIs, payload encryption, secure erasure, generic garbage
collection, or a backup format.
