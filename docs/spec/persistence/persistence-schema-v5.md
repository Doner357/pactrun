---
title: Persistence Schema V5
---

# Persistence Schema V5

**Status: Implemented normative internal schema, integrated into develop with
M4 Snapshot execution; non-Frozen and non-public.**

<!-- spec-navigation:start -->
## Reading map (informative)

Read the current M4 internal persistence contract, including exact writable admission and V4 bootstrap. This is non-Frozen and not a public database API.

Start with the [specification map](../index.md)
and [shared vocabulary](../glossary.md) if a term is unfamiliar.
Check [implementation status and remaining decisions](../../development/next-milestone.md)
before treating an approved contract as available runtime behavior.
The original status, rules, exceptions, and verification declarations below retain their meaning.
<!-- spec-navigation:end -->

### PR-REQ-0298 - Exact PersistenceSchemaV5

V5 MUST preserve the exact V4 tables and add exactly the ten tables below.
It retains application_id 0x50414354 and sets user_version to 5 only as part of
successful schema publication. Every table is STRICT and WITHOUT ROWID. Rowid,
clock time, insertion order, and query plans MUST NOT determine correctness.

```sql
CREATE TABLE writable_admissions (
    owner_session BLOB NOT NULL CHECK(length(owner_session) = 40),
    admitted_schema_version INTEGER NOT NULL CHECK(admitted_schema_version = 5),
    PRIMARY KEY (owner_session)
) STRICT, WITHOUT ROWID;

CREATE TABLE run_operation_kinds (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    operation_kind INTEGER NOT NULL CHECK(operation_kind IN (0, 1, 2)),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES runs(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_capture_invocations (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES run_operation_kinds(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_restore_invocations (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    snapshot_id BLOB NOT NULL CHECK(length(snapshot_id) = 16),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES run_operation_kinds(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE instance_recovery_consequence_versions (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    consequence_version INTEGER NOT NULL CHECK(consequence_version >= 0),
    PRIMARY KEY (instance_id),
    FOREIGN KEY (instance_id) REFERENCES instances(instance_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE snapshots (
    snapshot_id BLOB NOT NULL CHECK(length(snapshot_id) = 16),
    integrity_format INTEGER NOT NULL CHECK(integrity_format IN (1, 2)),
    integrity_digest BLOB NOT NULL CHECK(length(integrity_digest) = 32),
    canonical_manifest BLOB NOT NULL CHECK(length(canonical_manifest) > 0),
    PRIMARY KEY (snapshot_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE snapshot_blobs (
    snapshot_id BLOB NOT NULL CHECK(length(snapshot_id) = 16),
    blob_digest BLOB NOT NULL CHECK(length(blob_digest) = 32),
    byte_length INTEGER NOT NULL CHECK(byte_length >= 0),
    PRIMARY KEY (snapshot_id, blob_digest),
    FOREIGN KEY (snapshot_id) REFERENCES snapshots(snapshot_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE snapshot_blob_chunks (
    snapshot_id BLOB NOT NULL CHECK(length(snapshot_id) = 16),
    blob_digest BLOB NOT NULL CHECK(length(blob_digest) = 32),
    chunk_index INTEGER NOT NULL CHECK(chunk_index >= 0),
    chunk_bytes BLOB NOT NULL CHECK(length(chunk_bytes) BETWEEN 1 AND 1048576),
    PRIMARY KEY (snapshot_id, blob_digest, chunk_index),
    FOREIGN KEY (snapshot_id, blob_digest)
        REFERENCES snapshot_blobs(snapshot_id, blob_digest) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_restore_admissions (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    snapshot_id BLOB NOT NULL CHECK(length(snapshot_id) = 16),
    admitted_state_version BLOB NOT NULL CHECK(length(admitted_state_version) = 16),
    admitted_consequence_version INTEGER NOT NULL CHECK(admitted_consequence_version >= 0),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES run_executions(run_id) ON DELETE CASCADE,
    FOREIGN KEY (snapshot_id) REFERENCES snapshots(snapshot_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE run_capture_results (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    snapshot_id BLOB NOT NULL CHECK(length(snapshot_id) = 16),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES run_outcomes(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
```

### Logical loading and mutation invariants

- operation_kind is Action=0, Capture=1, Restore=2. Every Run MUST have exactly
  one discriminator and exactly one matching invocation row; extra or missing
  variants are corruption. Existing Action ranks and optional-value encodings
  remain unchanged. Failed-step ranks 0..5 retain their Action meanings and map
  to Admission, establish Session, launch Hook, accept completion, publish
  managed result, and finalize for Snapshot operations.
- Every Instance MUST have one nonnegative signed-64-bit consequence counter.
  Increment is checked; exhaustion fails closed, never wraps. Only a newly
  committed open-risk terminal consequence advances it. Risk requests alone,
  successful Restore, and migration do not. Counter advancement, outcome, and
  guard consequence are one transaction even when an existing guard remains.
- A Running Run remains Accepted or Admitted as established by its exact
  Revision pin. Accepted+Open is corrupt. Access comes from its exact/pinned
  authoritative capability, not operation_kind or a caller. Action pins remain
  active-only; Capture pins the complete admitted registry. Roles/absence are
  derived from that set and the immutable pinned Revision, not current rows.
- An Admitted Running Restore MUST have exactly one run_restore_admissions row;
  other operations and Accepted-only Restore MUST have none. Its SnapshotId
  equals its invocation, state token equals the accepted/revalidated token,
  producer matches the pinned Revision, and its Snapshot foreign key is the
  execution strong pin. No old-target payload pin is required for rollback:
  Restore changes no target bindings before terminal commit and never rolls
  back concurrent management changes.
- Restore's terminal transaction MUST compare both retained tokens before any
  successful target publication. Snapshot content may exceed target Managed
  Input capability; check before launch, never truncate or silently promote.
  Restore creates fresh target-instance-scoped payload IDs under the existing
  Managed Input contract. Terminal execution-row deletion releases the
  Snapshot admission pin in the same transaction.
- A successful Capture MUST publish exactly one result reference together with
  its Snapshot and outcome. Unsuccessful Capture MUST publish none. The result
  and Restore invocation SnapshotIds are historical references, not lifecycle
  ownership: no foreign key may permanently retain the Snapshot through them.
- Snapshot root identity, format, and digest MUST agree with its exact canonical
  manifest. The manifest is authority for producer, origin, capture time,
  bindings, and service descriptors. Do not add a divergent authoritative
  registry. Snapshot roots have no lifecycle foreign key to Instance, Run,
  or installed Revision.
- The blob keys MUST equal the exact manifest closure, with one header per
  digest in that Snapshot. Chunks are contiguous from zero; each non-final
  chunk is 1,048,576 bytes, the final nonempty chunk is 1..1,048,576 bytes, and
  aggregate length equals the header. Empty blobs have no chunks. Check digest
  and exact closure before publication; full verification streams all bytes.
  All chunk reads specify ORDER BY chunk_index. Snapshot enumeration orders by
  SnapshotId BLOB bytes. Cross-Snapshot/Instance/Artifact CAS is not introduced.
- Capability limits are checked independently of these storage invariants.
  Do not add the current 8 GiB ceiling to the schema or call an otherwise-valid
  over-capability object corrupt. Managed Input and Artifact V4 limits remain.
- Canonical manifests and digests may contain sensitive authoritative material.
  Run records MUST NOT copy this content. Inspection projects safe structural
  fields rather than dumping rows or library errors.
- Durable staging is not a result or a reconciler-committable PreparedCommit.
  Owner loss before terminal commit finishes Interrupted, never success.
  Orphan recovery depends on operation, boundary, risk, and references, not a
  persisted Plan. Corrupt states are not normalized into valid ones.

**Verification: PR-TEST-0182, PR-TEST-0183, PR-TEST-0195, PR-TEST-0202, PR-TEST-0203, PR-TEST-0204, PR-TEST-0208, PR-TEST-0209, PR-TEST-0210, PR-TEST-0212, PR-TEST-0213, PR-TEST-0215, PR-TEST-0222, PR-TEST-0223, PR-TEST-0224, PR-TEST-0226, PR-TEST-0228, PR-TEST-0229, PR-TEST-0230, PR-TEST-0232, PR-TEST-0233, PR-TEST-0234, PR-TEST-0236, PR-TEST-0239, PR-TEST-0240, PR-TEST-0246, PR-TEST-0247, PR-TEST-0252, PR-TEST-0255, PR-TEST-0258, PR-TEST-0259, PR-TEST-0260, PR-TEST-0263, PR-TEST-0264, PR-TEST-0267, PR-TEST-0275.**

Coverage includes exact DDL, initialization, operation discriminators,
consequence counters, legacy preservation, and Snapshot-owned byte loading,
verification, atomic import, and export. Partial S4 coverage additionally
checks competing operation/invocation consistency, exact Revision pins, and
owner-checked Snapshot interruption with atomic consequence advancement and
pin release. Further S4 tests exercise typed Snapshot acceptance, operation-aware
failed-step ranks, complete Capture registry pins, Restore admission-link
corruption, unknown/live/lost owners, and real-process Capture/Action admission
arbitration. Capture/Restore successful result publication and Hook execution
remain later-slice work. S4 additionally proves real Restore admission
qualification, before/after-commit crash boundaries, one selected immutable
Snapshot strong pin, exact state/consequence tokens (including nonzero counters),
and owner-held retry/cancellation without creating another Run. No DDL or rank
mapping changed to implement those paths.

### PR-REQ-0299 - Durable writable admission and serialized migration

An operation-scoped session directory and its lease are durable pre-admission
ownership evidence, not permission to write any schema version. In V5, only a
committed writable_admissions row grants writable qualification. Session
preparation MUST NOT require an admission row or block ordinary admission-aware
migration merely because its general lease is held.

Pre-admission preparation is limited to the current operation's resources.
Cross-session housekeeping and publisher maintenance MUST wait until the
supported exact schema is established and writable admission is committed.
An opener that refuses an incompatible schema does not gain authority to clean
that schema's abandoned operation data.

After acquiring the same SQLite serialized write boundary used for migration,
a writer MUST revalidate the exact supported schema before committing its
admission or performing schema-dependent writes. A pre-lock check is advisory.
The row MUST identify the exact generated session and schema version. The lease
MUST remain held for the entire writable qualification lifetime. Ending the
qualification MUST first prevent all further schema-dependent writes, then
revoke its admission and release connection/lease resources. If orderly
revocation fails, a stale record remains owner-lost rather than silently live.

Migration MUST inspect committed admissions under that serialized boundary:
live or unknown admitted owners block; confirmed owner loss does not. Missing
admission in admission-aware V5 MUST NOT trigger a fallback scan of all leases.
An orphan Run does not require a fabricated writer admission. Removing stale
writer qualification MUST NOT reconcile or infer any Run outcome.

**Verification: PR-TEST-0195, PR-TEST-0196, PR-TEST-0197, PR-TEST-0198,
PR-TEST-0201.**

### PR-REQ-0300 - Exact V4 to V5 legacy bootstrap

Only explicit storage upgrade may run this one-time exception:

```text
BEGIN IMMEDIATE
verify exact V4
inspect other legacy sessions, explicitly excluding this migrator
if any other owner is live or unknown: refuse and roll back
create exact V5 additions and migrate rows
verify exact V5 and publish user_version=5
COMMIT
```

V4 lacks writable-admission evidence, so this exception conservatively refuses
even an indistinguishable pre-admission legacy session. Diagnostics MUST say
legacy evidence is insufficient for safe migration, not claim that the session
is admitted. No PID, age, timeout, or daemon inference is allowed. A V4 process
that creates its lease after inspection but waits for the write boundary MUST
observe V5 at its existing transaction-bound schema revalidation and fail
closed before a V4 schema-dependent write. Real multi-process tests are required.

The migration MUST insert Action discriminators for every existing Run and
zero consequence baselines for every existing Instance. It MUST preserve all
existing identities, bytes, bindings, metadata, non-terminal Runs, owners,
pins, risk, guards, completions, failures, and Artifacts. It MUST NOT infer
Capture/Restore, writer admission, Snapshot, replay, or terminal disposition.
Confirmed-lost Running Runs remain Running until explicit reconciliation.

Schema comparison covers columns, types, nullability, defaults, checks, keys,
foreign keys/delete actions, STRICT/WITHOUT ROWID, and correctness-bearing
indexes/triggers. Failure before commit leaves exact V4; success leaves exact
V5. SQLite serialization is the only migration lock framework.

Pristine initialization builds V5 directly. Ordinary V4 opening reports upgrade
required. Exact V5 upgrade is validation plus no-op. V1/V2/V3 are refused with
instructions to first use a compatible release to reach exact V4; no implicit
upgrade chain is permitted. Foreign, unmarked nonempty, partial, drifted, or
newer schemas are rejected. The exception MUST NOT generalize to other source
versions or to missing admission records in V5.

**Verification: PR-TEST-0195, PR-TEST-0199, PR-TEST-0200, PR-TEST-0201,
PR-TEST-0202, PR-TEST-0205.**
