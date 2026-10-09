---
title: Revision Persistence and Publication
---

# Revision Persistence and Publication

Revision publication joins durable blob availability with an atomic database record. A physical blob and a database reference have distinct validation and commit boundaries.

### PR-REQ-0231 - SQLite persistence ownership and bootstrap

Pactrun MUST use the independently versioned
[Persistence baseline](../persistence/persistence-baseline.md) in a
caller-selected, dedicated local storage root. Before opening the database,
the Store preparation layer MUST establish the required directory tree as
defined by [PR-REQ-0374](./store-opening.md#pr-req-0374---automatic-preparation-of-new-stores).
On Windows the supported profile
is local fixed NTFS; on Linux it is limited to the ext4, XFS, Btrfs, and ZFS
profiles supported by the blob store. Preparation MUST complete required
directory durability barriers before publishing the new Store.

The SQLite database MUST use application ID `0x50414354`, private user version `2`, WAL
journal mode, `synchronous=FULL`, foreign-key enforcement, and a five-second
busy timeout. In WAL mode, the main database and a live WAL MAY together carry
committed database state. The SHM WAL index is reconstructible coordination and
cache state, not authoritative Pactrun Domain state. Copying live database
files is not a supported backup procedure. A backup facility, if provided, MUST
use a SQLite-supported consistent backup or checkpoint mechanism.

Before claiming a database, Pactrun MUST inspect its application ID, private user
version, supported `pactrun_metadata.format_version`, and non-SQLite schema objects.
Only the exact supported metadata and schema or a pristine database with
application ID zero, user version zero, and no user objects is admissible.
WAL establishment MUST return exactly `wal`. Pactrun MUST then use
`BEGIN IMMEDIATE`, re-read the ownership markers and schema while holding the
write transaction, and either validate the supported baseline or atomically create
its complete schema and markers. Foreign databases, unmarked non-empty stores, unsupported schemas and schema
drift MUST be rejected without conversion or data deletion. Two concurrent initializers MUST
converge on one exact schema through SQLite locking; the persistence adapter MUST NOT add a second
cross-process lock protocol.

Every baseline table MUST be `STRICT` and `WITHOUT ROWID`. Package IDs MUST be BLOBs
of exactly 16 bytes; Revision and runtime blob digests MUST be BLOBs of exactly
32 bytes. Schema validation MUST verify the actual tables, columns, constraints,
primary keys, foreign keys, and table options rather than trusting version
markers alone. Any supported internal persistence evolution is independent from Pack-defined
Revision Migration and MUST preserve existing Pactrun identities.

**Verification: PR-TEST-0052, PR-TEST-0056.**

### PR-REQ-0232 - Exact persisted Revision content

Pactrun MUST persist a Revision as its structured `PackageId` plus
`RevisionContentDigest`, exact canonical `RevisionCoreV1` bytes, and exact
canonical `RuntimeContentClosureIdentityV1` bytes. It MUST NOT persist the
dual-component frame as a third representation. The
`revision_runtime_content_refs` relation MUST be a derived index of the
canonical runtime-content component: every row is the exact canonical
`ContentId` and blob digest declared by that component. Publication tokens MUST
NOT supply ContentIds, blob mappings, runtime paths, executable semantics, or
other Revision meaning.

Logical load MUST strict-decode both canonical components, reconstruct the
validated sibling pair, recompute the Revision content digest, and compare the
complete closure-derived reference relation with the persisted index. Any
mismatch is corruption. Logical load MUST NOT require the referenced physical
blobs to be currently available; physical availability remains a separate blob-store
verification operation.

**Verification: PR-TEST-0053, PR-TEST-0054, PR-TEST-0055, PR-TEST-0057.**

### PR-REQ-0233 - Durable content before new database references

`StoredRuntimeBlob` MUST carry a private, process-local, non-serializable
store-instance witness issued only after durable blob publication succeeds. A
new Revision record MUST be authorized by exactly one current same-store witness
for every distinct blob digest required by its runtime-content closure. Missing,
extra, duplicate, or wrong-store witnesses MUST be rejected. Multiple
ContentIds that share one blob require one witness for that physical digest.
Witnesses authorize durable-content-before-reference ordering only and MUST NOT
be persisted or affect any Pactrun identity.

`InstallPackSource` MUST obtain the current same-store witness for every
distinct staged runtime blob before entering the installation transaction in
PR-REQ-0261. The additional metadata publication does not weaken or replace a
witness.

An exact Revision record that was already committed MAY be returned
idempotently without an old witness, including after a process crash and store
reopen. If blob publication succeeds but the database transaction does not,
the unreferenced immutable blob is a safe orphan. Pactrun MUST never publish a
database reference before the required durable blob publication succeeds.

**Verification: PR-TEST-0055, PR-TEST-0056.**

### PR-REQ-0234 - Immutable and recoverable Revision records

All schema and Revision writes, apart from SQLite's required out-of-
transaction WAL mode establishment, MUST use `BEGIN IMMEDIATE`. An exact retry
MUST be idempotent. An existing Revision identity with different canonical
component bytes or a different closure-derived reference index MUST be rejected
as corruption or collision and MUST NOT be overwritten or repaired.

A crash before COMMIT MUST leave no partial Revision record. A crash after
COMMIT but before the API response MAY be retried after restart and MUST recover
the exact committed record without requiring the expired process-local witness.
A successful SQLite COMMIT under WAL and `synchronous=FULL`, after successful
SQLite/VFS persistence operations, is the durable reference-publication
point. The persistence adapter MUST NOT manually sync the database, WAL, or SHM around each
transaction, and its durability guarantee assumes the platform storage stack
honors successful persistence operations. Process-crash tests do not certify
arbitrary hardware power-loss behavior.

Installation includes source-projected portable metadata and any
explicit crate-private local metadata in that same transaction. The exact
retry and no-partial-publication rules in PR-REQ-0261 preserve this boundary.

**Verification: PR-TEST-0052, PR-TEST-0054, PR-TEST-0056, PR-TEST-0057.**
