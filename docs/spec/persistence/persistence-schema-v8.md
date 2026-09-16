---
title: Persistence Schema V8
---

# PersistenceSchemaV8

**Status: Approved M7 internal contract; non-Frozen.** Runtime delivery is tracked
in the [M7 implementation record](../../development/m7-implementation-status.md).
Frozen external identities and formats are unchanged.

<!-- spec-navigation:start -->
Predecessor: [exact V7](./persistence-schema-v7.md).
Lifecycle owner: [M7 Instance retirement](../execution/m7-instance-retirement.md).
<!-- spec-navigation:end -->

### PR-REQ-0338 - Exact V8 lifecycle records

V8 preserves the preceding schema except the exact replacements below. Run
identity, outcome, child records and Artifacts MUST survive Instance removal.
The historical identity relation retains the last managed name as provenance,
not as a unique lookup key or permanent Revision pin. Creating a new Instance
MUST atomically create its history identity; reusing a name does not reuse an ID.

Deletion mode ranks are 0 ManagedCleanup and 1 AbandonManagement. Managed
execution kind 4 is DeleteInstance; ranks 0 through 3 keep their meanings.
Obligation phases are 0 launch authorized, 1 result unresolved and 2 finalization
authorized. Authority sources are 0 Hook completed, 1 operator confirmed and
2 no Cleanup declared. These are private persistence discriminants, not public
Instance states. Unknown ranks are corruption, not a fallback.

An obligation's attempt belongs to its Instance and has deletion mode 0. Phase 2
has exactly one authorization; phases 0/1 have none. A frozen work set belongs
to that authority and every allocation has the same immutable Instance owner.
An allocation may not be in two finalizations. No service bytes or native path
becomes authoritative database state. Receipt references preserve sufficient
provenance without permanently pinning obsolete Revision installation.

Retirement mode 0 requires all storage-lifetime obligations finished. Mode 1
requires explicit Abandon history and detached custody for surviving protected
allocations. Detached ownership must agree with allocation provenance. A
missing live owner without valid retirement/detached or completed-finalization
evidence remains corruption. Existing protected custody is never silently
converted into discard authority. Discard authority requires detached custody
and no active or unresolved resource/execution references.

`root_identity` records the physical incarnation before destructive I/O, not a
native path or content digest. Its 40 bytes contain five little-endian u64 words.
Tag 1 records the NTFS volume serial, file index, creation FILETIME and reserved
zero. Tag 2 records the Linux device major/minor pair (major in the high 32 bits),
inode, birth-time seconds and birth-time nanoseconds. Tags 1 and 2 are legacy
incarnation representations. Tag 3 has tag 2's native fields and requires the
Linux claim-isolated retirement backend; tag 4 has tag 1's native fields and
requires Windows namespace-pinned retirement. Tags 1/4 require the reserved
word to be zero; tags 2/3 require nanoseconds below one billion.

A legacy marker may be promoted only after qualifying the same native
incarnation, under the existing serialized operation. Commit the new marker
before relocation or removal. This makes an older reader reject an unsupported
backend requirement rather than mistake a relocated root for an absent one.
Changing the marker never changes the native identity or authorizes another
object. Cross-platform evidence is not an absence result. A platform without
reliable incarnation evidence MUST fail closed. Unknown tags are corruption. Initial
finalization work rows may have no identity before inspection; an observed absent
root is completed atomically instead of leaving unqualified authority that could
adopt later bytes. A pending discard requires a recorded physical identity.
An incarnation qualified during partial finalization remains binding after
Abandon and cannot be replaced by a later discard's newly observed identity.

The Linux `retirement-v1` area is durable physical progress for the same
AllocationId, not Workspace, a Snapshot, or a new source of deletion authority.
Every journal is bound to the V8-qualified root incarnation. Its versioned,
closed records retain native component names, parent provenance, qualified
object identity, original/claimed position and completion/move receipts.
Publish complete initial records atomically; publish claim intent before rename,
qualify what was actually captured, and establish durability before deletion.
Recovery must not fall back from a captured/completed position to a replacement
at its former name. Missing, malformed, foreign or contradictory progress must
fail closed; startup and ordinary maintenance must not execute it.

Allocation-scoped exclusive ownership covers physical progress. Explicit
read-only handoff cannot expose current processing positions while that owner
is active. Every resumed attempt reclaims survivors into fresh parents before
destructive use, because an earlier handoff may have exposed their old positions.
The control records remain Pactrun-authoritative metadata, separate from the
service-owned objects they account for. They are not an authoring format, and
no protection against deliberate concurrent alteration of Pactrun's own control
metadata is added beyond the existing trusted-local-store model.

Unexposed preparations are not proof that an existing directory belongs to
Pactrun. Freeze only protected allocations for physical finalization. Retirement
must not promote preparations into protection or detached discard authority.
Their intent/origin records retain the preceding conservative maintenance
rules, including preservation of existing or uncertain paths. An unresolved
preparation reference is not a permanent pin solely for historical provenance.

**Verification: PR-TEST-0392, PR-TEST-0394, PR-TEST-0395, PR-TEST-0400, PR-TEST-0408, PR-TEST-0416, PR-TEST-0419, PR-TEST-0420, PR-TEST-0421, PR-TEST-0422, PR-TEST-0427, PR-TEST-0436, PR-TEST-0438, PR-TEST-0441, PR-TEST-0445.**

These tests prove exact DDL, referential preservation and rejection of corrupt
legacy references. Logical lifecycle writers/readers and finalization runtime
require their own coverage; schema tests do not certify their implementation.

## Exact changes

Data-bearing table replacement uses a dedicated maintenance connection with
foreign-key enforcement disabled before BEGIN; never disable it on an admitted
runtime connection. Validate the full graph before committing and restore
foreign-key enforcement before further connection use. The table replacements
must not cascade through existing Run children.

```sql
DROP TABLE writable_admissions;
CREATE TABLE writable_admissions (
    owner_session BLOB NOT NULL CHECK(length(owner_session) = 40),
    admitted_schema_version INTEGER NOT NULL CHECK(admitted_schema_version = 8),
    PRIMARY KEY (owner_session)
) STRICT, WITHOUT ROWID;

CREATE TABLE instance_history_identities (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    instance_name BLOB NOT NULL CHECK(length(instance_name) BETWEEN 1 AND 128),
    PRIMARY KEY (instance_id)
) STRICT, WITHOUT ROWID;
INSERT INTO instance_history_identities SELECT instance_id, instance_name FROM instances;

CREATE TABLE v8_runs (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    accepted_state_version BLOB NOT NULL CHECK(length(accepted_state_version) = 16),
    accepted_at_unix_ms INTEGER NOT NULL CHECK(accepted_at_unix_ms >= 0),
    PRIMARY KEY (run_id),
    FOREIGN KEY (instance_id) REFERENCES instance_history_identities(instance_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;
INSERT INTO v8_runs SELECT * FROM runs;
DROP TABLE runs;
ALTER TABLE v8_runs RENAME TO runs;

CREATE TABLE v8_run_operation_kinds (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    operation_kind INTEGER NOT NULL CHECK(operation_kind IN (0, 1, 2, 3, 4)),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES runs(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
INSERT INTO v8_run_operation_kinds SELECT * FROM run_operation_kinds;
DROP TABLE run_operation_kinds;
ALTER TABLE v8_run_operation_kinds RENAME TO run_operation_kinds;

CREATE TABLE run_deletion_invocations (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    deletion_mode INTEGER NOT NULL CHECK(deletion_mode IN (0, 1)),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES run_operation_kinds(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE instance_deletion_obligations (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    attempt_run_id BLOB NOT NULL CHECK(length(attempt_run_id) = 16),
    phase INTEGER NOT NULL CHECK(phase IN (0, 1, 2)),
    PRIMARY KEY (instance_id),
    UNIQUE (attempt_run_id),
    FOREIGN KEY (instance_id) REFERENCES instances(instance_id) ON DELETE RESTRICT,
    FOREIGN KEY (attempt_run_id) REFERENCES runs(run_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE deletion_finalization_authorizations (
    attempt_run_id BLOB NOT NULL CHECK(length(attempt_run_id) = 16),
    authority_source INTEGER NOT NULL CHECK(authority_source IN (0, 1, 2)),
    authorized_state_version BLOB NOT NULL CHECK(length(authorized_state_version) = 16),
    authorized_at_unix_ms INTEGER NOT NULL CHECK(authorized_at_unix_ms >= 0),
    PRIMARY KEY (attempt_run_id),
    FOREIGN KEY (attempt_run_id) REFERENCES runs(run_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE deletion_finalization_allocations (
    attempt_run_id BLOB NOT NULL CHECK(length(attempt_run_id) = 16),
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    finished INTEGER NOT NULL CHECK(finished IN (0, 1)),
    root_identity BLOB CHECK(root_identity IS NULL OR length(root_identity) = 40),
    PRIMARY KEY (attempt_run_id, allocation_id),
    UNIQUE (allocation_id),
    FOREIGN KEY (attempt_run_id) REFERENCES deletion_finalization_authorizations(attempt_run_id) ON DELETE RESTRICT,
    FOREIGN KEY (allocation_id) REFERENCES service_storage_allocations(allocation_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE instance_retirement_receipts (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    retirement_run_id BLOB NOT NULL CHECK(length(retirement_run_id) = 16),
    retirement_mode INTEGER NOT NULL CHECK(retirement_mode IN (0, 1)),
    partially_finalized INTEGER NOT NULL CHECK(partially_finalized IN (0, 1)),
    retired_at_unix_ms INTEGER NOT NULL CHECK(retired_at_unix_ms >= 0),
    PRIMARY KEY (instance_id),
    FOREIGN KEY (instance_id) REFERENCES instance_history_identities(instance_id) ON DELETE RESTRICT,
    FOREIGN KEY (retirement_run_id) REFERENCES runs(run_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE detached_service_allocations (
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    PRIMARY KEY (allocation_id),
    FOREIGN KEY (allocation_id) REFERENCES service_storage_allocations(allocation_id) ON DELETE RESTRICT,
    FOREIGN KEY (instance_id) REFERENCES instance_retirement_receipts(instance_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE allocation_discard_receipts (
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    authorized_at_unix_ms INTEGER NOT NULL CHECK(authorized_at_unix_ms >= 0),
    finished INTEGER NOT NULL CHECK(finished IN (0, 1)),
    root_identity BLOB CHECK(root_identity IS NULL OR length(root_identity) = 40),
    PRIMARY KEY (allocation_id),
    FOREIGN KEY (allocation_id) REFERENCES detached_service_allocations(allocation_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;
```

### PR-REQ-0339 - Explicit exact-V7 upgrade

New stores use exact V8. Existing exact V7 requires explicit storage upgrade;
normal writer or inspection opening does not migrate. Earlier versions require
a compatible intermediate build. Unknown or drifted schemas are rejected.

Upgrade MUST serialize through the existing immediate transaction and writer
admission mechanism. Every V7 admission must have confirmed owner loss; held
or inconclusive leases block upgrade. Removing stale writer admissions does
not terminate Runs. The upgrade atomically copies historical Instance identities,
preserves all existing state and unresolved obligations, checks exact V8 schema
and all foreign keys, and advances the version marker. It creates no inferred
Cleanup result, authorization, retirement, detached custody or discard receipt.

A crash before commit leaves exact V7; after commit it leaves exact V8. Concurrent
upgraders observe the committed version. Failure to remove the upgrader's own
admission after commit does not turn successful upgrade into failure; its lease
provides the existing safe recovery mechanism.

**Verification: PR-TEST-0300, PR-TEST-0393, PR-TEST-0401, PR-TEST-0402, PR-TEST-0403.**

The current test proves transaction rollback and absence of inferred lifecycle
evidence. Cross-process owner exclusion and commit fault-injection evidence
remain pending until the V8 upgrade integration tests are completed.
