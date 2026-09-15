---
title: Persistence Schema V6
---

# Persistence Schema V6

**Status: Implemented normative internal schema and explicit V5-to-V6 upgrade. Non-Frozen and non-public.**

<!-- spec-navigation:start -->
## Reading map (informative)

Read the current schema and upgrade boundary alongside the historical
[V5 contract](./persistence-schema-v5.md) and [M5 execution](../execution/m5-migration-execution.md).
The [implementation record](../../development/m5-implementation-status.md)
records Migration runtime coverage and the remaining verification gates.
<!-- spec-navigation:end -->

V6 is the historical integrated M5/M6 schema. A V6 writer initializes V6 and
accepts exact V5 for its explicit upgrade. The current M6.5 writer instead uses
[V7](./persistence-schema-v7.md) and accepts exact V6; this page does not authorize
an implicit multi-version upgrade. Declarative and Hook Migration execution and
edge publication are implemented under PR-REQ-0313 and the M5 execution contract.

### PR-REQ-0308 - V6 Migration persistence and upgrade boundary

V6 MUST preserve existing operation ranks and historical data meaning, adding
a distinct Migration discriminator. Durable Migration state MUST identify the
exact invocation, ordered edges, current edge/step, committed-edge evidence,
binding references, and the last committed recovery boundary. The final edge
and Run success share the atomic boundary of PR-REQ-0306. Recovery references
MUST remain strong roots while reconciliation or an unresolved obligation needs
them; temporary file presence is not durable publication evidence.

The M5 writer MUST initialize pristine storage directly as exact V6. Ordinary
V5 opening MUST require explicit storage upgrade. Only exact V5 may upgrade
directly to V6; exact V6 is validation and no-op. V4 and earlier MUST require a
compatible build to reach exact V5 first. Foreign, unmarked nonempty, partial,
drifted, and newer schemas MUST be rejected. No implicit version chain exists.

Upgrade MUST revalidate exact schema and committed writer-admission evidence
under the serialized SQLite write boundary. Live or inconclusive writer owners
MUST block upgrade; unadmitted sessions MUST NOT be treated as admitted writers.
An old V5 writer arriving later MUST fail its transaction-bound version check
before performing a V5 schema-dependent write. Upgrade MUST preserve identities,
bytes, bindings, metadata, Runs, owners, pins, risk, guards, completions, failures,
Snapshots, and Artifacts. It MUST NOT infer admission, replay, reconciliation,
or terminal disposition. Removing proven-lost writer admission does not terminate
its Running Run. Failure leaves exact V5, success publishes exact V6 atomically.

**Verification: Pending automated coverage.**

The broad execution and recovery promises above remain pending. The following
requirements cover exact storage representation and upgrade, not completion of
PR-REQ-0306 or the complete M5 runtime.

### PR-REQ-0311 - Exact PersistenceSchemaV6 representation

V6 MUST retain application_id 0x50414354 and publish user_version 6. It preserves
the V5 schema except for writable_admissions, the operation discriminator and
its two dependent invocation-table rebuilds, and the seven added Migration
tables in the SQL below. All tables remain STRICT and WITHOUT ROWID. Existing
operation ranks 0/1/2 retain Action/Capture/Restore meaning; rank 3 is Migration.
No temporary rebuild table may remain after publication. Actual foreign keys
stay enabled during production upgrade; this group of dependent tables is
copied before its old children and parent are removed.

The SQL is used only inside pristine construction or after PR-REQ-0312's exact
V5 quiescence gate. It is not a public manual migration script. Application code
validates the final schema and foreign keys before publishing the version.

```sql
DROP TABLE writable_admissions;
CREATE TABLE writable_admissions (
    owner_session BLOB NOT NULL CHECK(length(owner_session) = 40),
    admitted_schema_version INTEGER NOT NULL CHECK(admitted_schema_version = 6),
    PRIMARY KEY (owner_session)
) STRICT, WITHOUT ROWID;

CREATE TABLE v6_run_operation_kinds (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    operation_kind INTEGER NOT NULL CHECK(operation_kind IN (0, 1, 2, 3)),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES runs(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
CREATE TABLE v6_run_capture_invocations (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES v6_run_operation_kinds(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
CREATE TABLE v6_run_restore_invocations (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    snapshot_id BLOB NOT NULL CHECK(length(snapshot_id) = 16),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES v6_run_operation_kinds(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
INSERT INTO v6_run_operation_kinds SELECT * FROM run_operation_kinds;
INSERT INTO v6_run_capture_invocations SELECT * FROM run_capture_invocations;
INSERT INTO v6_run_restore_invocations SELECT * FROM run_restore_invocations;
DROP TABLE run_capture_invocations;
DROP TABLE run_restore_invocations;
DROP TABLE run_operation_kinds;
ALTER TABLE v6_run_operation_kinds RENAME TO run_operation_kinds;
ALTER TABLE v6_run_capture_invocations RENAME TO run_capture_invocations;
ALTER TABLE v6_run_restore_invocations RENAME TO run_restore_invocations;

CREATE TABLE run_migration_invocations (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    source_revision_digest BLOB NOT NULL CHECK(length(source_revision_digest) = 32),
    target_revision_digest BLOB NOT NULL CHECK(length(target_revision_digest) = 32),
    authorize_declassification INTEGER NOT NULL CHECK(authorize_declassification IN (0, 1)),
    PRIMARY KEY (run_id),
    CHECK(source_revision_digest <> target_revision_digest),
    FOREIGN KEY (run_id) REFERENCES run_operation_kinds(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
CREATE TABLE run_migration_edges (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    edge_index INTEGER NOT NULL CHECK(edge_index >= 0),
    source_revision_digest BLOB NOT NULL CHECK(length(source_revision_digest) = 32),
    target_revision_digest BLOB NOT NULL CHECK(length(target_revision_digest) = 32),
    PRIMARY KEY (run_id, edge_index),
    CHECK(source_revision_digest <> target_revision_digest),
    FOREIGN KEY (run_id) REFERENCES run_migration_invocations(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
CREATE TABLE run_migration_progress (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    committed_edge_count INTEGER NOT NULL CHECK(committed_edge_count >= 0),
    step_rank INTEGER NOT NULL CHECK(step_rank BETWEEN 0 AND 4),
    boundary_revision_digest BLOB NOT NULL CHECK(length(boundary_revision_digest) = 32),
    boundary_state_version BLOB NOT NULL CHECK(length(boundary_state_version) = 16),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES run_migration_invocations(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
CREATE TABLE run_migration_boundaries (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    edge_index INTEGER NOT NULL CHECK(edge_index >= 0),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    instance_state_version BLOB NOT NULL CHECK(length(instance_state_version) = 16),
    PRIMARY KEY (run_id, edge_index),
    FOREIGN KEY (run_id, edge_index) REFERENCES run_migration_edges(run_id, edge_index) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
CREATE TABLE run_migration_revision_pins (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    PRIMARY KEY (run_id, package_id, revision_content_digest),
    FOREIGN KEY (run_id) REFERENCES run_migration_invocations(run_id) ON DELETE CASCADE,
    FOREIGN KEY (package_id, revision_content_digest) REFERENCES revisions(package_id, revision_content_digest) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;
CREATE TABLE run_migration_payload_pins (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    payload_id BLOB NOT NULL CHECK(length(payload_id) = 16),
    PRIMARY KEY (run_id, instance_id, payload_id),
    FOREIGN KEY (run_id) REFERENCES run_migration_invocations(run_id) ON DELETE CASCADE,
    FOREIGN KEY (instance_id, payload_id) REFERENCES managed_input_payloads(instance_id, payload_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;
CREATE TABLE run_migration_checkpoint_bindings (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    input_identity BLOB NOT NULL CHECK(length(input_identity) > 0),
    payload_id BLOB NOT NULL CHECK(length(payload_id) = 16),
    PRIMARY KEY (run_id, input_identity),
    FOREIGN KEY (run_id) REFERENCES run_migration_progress(run_id) ON DELETE CASCADE,
    FOREIGN KEY (instance_id, payload_id) REFERENCES managed_input_payloads(instance_id, payload_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;
```

Migration invocation stores an exact same-lineage source/target and the
declassification authorization, never a CLI path ID or host acquisition path.
Edges are zero-based and ordered; their source/target pairs describe the exact
accepted sequence. No FK to installed Revisions is inferred at acceptance;
execution pins belong to successful Admission.

The progress/checkpoint representation is established at Admission, not merely
acceptance. committed_edge_count counts published edge boundaries; step ranks
0..4 mean establish Session, launch Hook, accept completion, publish managed
result, and finalize. Boundary rows record the target and state version of each
successful edge. The progress boundary is the initial admitted source at count
zero or the latest committed edge afterward. It is historical boundary evidence,
not a claim that the Instance cannot subsequently change. The checkpoint holds
the complete present-binding registry at that boundary; absence and roles are
derived from the associated exact Revision and the single registry. Payload
protection remains owned by immutable managed_input_payloads, not duplicated
or downgraded in checkpoint metadata.

Revision, payload, and checkpoint references MUST prevent premature collection.
The payload reclaimer MUST check Migration pins and checkpoint bindings as well
as ordinary bindings and existing execution pins, instead of failing a whole
mutation on an avoidable FK violation. Necessary recovery references survive
terminalization while an unresolved obligation still needs them; logical
admission, per-edge commit, and lifetime transitions remain runtime work under
PR-REQ-0306, not claims established solely by these SQL constraints.

Matching rank-3 invocations MUST be decoded as Migration rather than inferred
as Action. Mixed invocation kinds and inconsistent path/boundary/reference data
MUST be rejected. Execution capability is separate from structural inspection;
Hook-backed execution remains unsupported by this slice.

**Verification: PR-TEST-0293, PR-TEST-0297, PR-TEST-0298, PR-TEST-0299.**

### PR-REQ-0312 - Exact V5-to-V6 admission-aware upgrade

Only explicit storage upgrade may change an existing exact V5 database to V6.
Pristine initialization creates V6 directly. Exact V6 upgrade validates the
schema and is a read-only no-op, even with active current writers. Ordinary V5
opening reports upgrade required; V1/V2/V3/V4 require a compatible build to
reach exact V5 first. Foreign, unmarked nonempty, drifted, partial, and newer
schemas MUST fail without an implicit upgrade chain or repair.

After an advisory read-only check, the upgrader prepares its own unadmitted
maintenance session and takes BEGIN IMMEDIATE. It MUST revalidate exact V5 and
inspect only committed writable_admissions there. Held or inconclusive leases
block the transaction; missing admission rows never authorize a fallback scan
of all staging sessions. Proven-lost writer qualifications may be revoked;
Running orphan Runs MUST remain Running until explicit reconciliation. The
upgrader's own preparation creates no writer-admission row.

Schema, data preservation, foreign-key validation, and version publication MUST
be one transaction. Failure or loss before commit leaves exact V5; loss after
commit leaves exact V6. A late V5 writer that was only prepared before upgrade
must reject V6 at its transaction-bound version check before a V5 write. No new
lock framework, PID/age-based owner inference, implicit reconciliation, or Hook
replay is introduced. All historical data other than revoked lost-writer tickets
is preserved, including operation kinds, captures/restores, bindings, payload
bytes, Run outcomes and artifacts, live recovery obligations, and snapshots.
Upgrade is not a global Snapshot-content or Run-semantic integrity certificate.

**Verification: PR-TEST-0294, PR-TEST-0295, PR-TEST-0296, PR-TEST-0297,
PR-TEST-0298.**

The current CLI path-ID preservation test PR-TEST-0300 now targets V6-to-V7.
The historical exact V5-to-V6 contract remains enforced by the tests above;
this does not create an implicit upgrade chain in the current binary.
