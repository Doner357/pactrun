---
title: Persistence Schema V7
---

# Persistence Schema V7

**Status: Implemented normative internal schema and explicit V6-to-V7 upgrade. Non-Frozen and non-public.**

The integrated M6.5 build initializes V7 and implements explicit exact-V6
upgrade. V6 remains a historical schema, not the current writer's target.
This document's DDL is mirrored by the internal implementation, not a public
manual upgrade entry point. Live ServiceStorage allocation and logical runtime
invariants have separate evidence below; schema shape alone is not runtime proof.

<!-- spec-navigation:start -->
## Reading map (informative)

Read [V6](./persistence-schema-v6.md), [Core V2](../contracts/revision-core-format-v2.md),
[execution/lifecycle](../execution/m6-5-service-storage-execution.md), and
[S0 review](../../development/design-notes/m6-5-servicestorage-baseline.md).
<!-- spec-navigation:end -->

### PR-REQ-0323 - Exact V7 additions

V7 preserves every V6 table and rank except writable_admissions.
Keep application_id 0x50414354; the upgrade wrapper publishes user_version 7
only after exact-schema and foreign-key validation. The following is the complete
DDL delta. No transaction wrapper, user_version write, filesystem work,
data migration or admission selection is delegated to this SQL block.

```sql
DROP TABLE writable_admissions;
CREATE TABLE writable_admissions (
    owner_session BLOB NOT NULL CHECK(length(owner_session) = 40),
    admitted_schema_version INTEGER NOT NULL CHECK(admitted_schema_version = 7),
    PRIMARY KEY (owner_session)
) STRICT, WITHOUT ROWID;

CREATE TABLE service_storage_allocations (
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    origin_revision_digest BLOB NOT NULL CHECK(length(origin_revision_digest) = 32),
    origin_storage_identity BLOB NOT NULL CHECK(length(origin_storage_identity) BETWEEN 1 AND 128),
    PRIMARY KEY (allocation_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE service_storage_preparations (
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    owner_session BLOB NOT NULL CHECK(length(owner_session) = 40),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    PRIMARY KEY (allocation_id),
    FOREIGN KEY (allocation_id) REFERENCES service_storage_allocations(allocation_id) ON DELETE CASCADE,
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE service_storage_protections (
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    PRIMARY KEY (allocation_id),
    FOREIGN KEY (allocation_id) REFERENCES service_storage_allocations(allocation_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE service_storage_run_origins (
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    origin_run_id BLOB NOT NULL CHECK(length(origin_run_id) = 16),
    PRIMARY KEY (allocation_id),
    FOREIGN KEY (allocation_id) REFERENCES service_storage_allocations(allocation_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE instance_service_storages (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    storage_identity BLOB NOT NULL CHECK(length(storage_identity) BETWEEN 1 AND 128),
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    declaration_package_id BLOB NOT NULL CHECK(length(declaration_package_id) = 16),
    declaration_revision_digest BLOB NOT NULL CHECK(length(declaration_revision_digest) = 32),
    PRIMARY KEY (instance_id, storage_identity),
    FOREIGN KEY (instance_id) REFERENCES instances(instance_id) ON DELETE RESTRICT,
    FOREIGN KEY (allocation_id) REFERENCES service_storage_allocations(allocation_id) ON DELETE RESTRICT,
    FOREIGN KEY (declaration_package_id, declaration_revision_digest)
        REFERENCES revisions(package_id, revision_content_digest) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE instance_service_resources (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    resource_identity BLOB NOT NULL CHECK(length(resource_identity) BETWEEN 1 AND 128),
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    declaration_package_id BLOB NOT NULL CHECK(length(declaration_package_id) = 16),
    declaration_revision_digest BLOB NOT NULL CHECK(length(declaration_revision_digest) = 32),
    PRIMARY KEY (instance_id, resource_identity),
    FOREIGN KEY (instance_id) REFERENCES instances(instance_id) ON DELETE RESTRICT,
    FOREIGN KEY (allocation_id) REFERENCES service_storage_allocations(allocation_id) ON DELETE RESTRICT,
    FOREIGN KEY (declaration_package_id, declaration_revision_digest)
        REFERENCES revisions(package_id, revision_content_digest) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE run_service_storage_pins (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    PRIMARY KEY (run_id, allocation_id),
    FOREIGN KEY (run_id) REFERENCES run_executions(run_id) ON DELETE CASCADE,
    FOREIGN KEY (allocation_id) REFERENCES service_storage_allocations(allocation_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE run_service_storage_targets (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    edge_index INTEGER NOT NULL CHECK(edge_index >= 0),
    storage_identity BLOB NOT NULL CHECK(length(storage_identity) BETWEEN 1 AND 128),
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    PRIMARY KEY (run_id, edge_index, storage_identity),
    FOREIGN KEY (run_id) REFERENCES run_executions(run_id) ON DELETE CASCADE,
    FOREIGN KEY (run_id, edge_index) REFERENCES run_migration_edges(run_id, edge_index) ON DELETE CASCADE,
    FOREIGN KEY (allocation_id) REFERENCES service_storage_allocations(allocation_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE run_service_resource_targets (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    edge_index INTEGER NOT NULL CHECK(edge_index >= 0),
    resource_identity BLOB NOT NULL CHECK(length(resource_identity) BETWEEN 1 AND 128),
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    PRIMARY KEY (run_id, edge_index, resource_identity),
    FOREIGN KEY (run_id) REFERENCES run_executions(run_id) ON DELETE CASCADE,
    FOREIGN KEY (run_id, edge_index) REFERENCES run_migration_edges(run_id, edge_index) ON DELETE CASCADE,
    FOREIGN KEY (allocation_id) REFERENCES service_storage_allocations(allocation_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE run_service_edge_commits (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    edge_index INTEGER NOT NULL CHECK(edge_index >= 0),
    PRIMARY KEY (run_id, edge_index),
    FOREIGN KEY (run_id, edge_index) REFERENCES run_migration_boundaries(run_id, edge_index) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
```

All identifiers stored as BLOB are strict-decoded Domain IDs, not arbitrary
bytes that merely pass SQL lengths. Associations do not duplicate locator,
kind, storage identity (for resources), exposure or mutable presence: their
exact declaration Revision plus typed identity supplies that contract. This is
a dedicated typed service-resource relation, not ManagedInputBindings or EAV.

### Logical invariants and readers

- Every allocation has exactly one preparation or protection row, never both
  and never neither at a committed boundary. Instance publication, target
  association publication or any external path grant requires protection.
  A preparation's Revision identity equals its allocation's origin identity
  and pins that exact declaration until protection/publication; after promotion,
  Instance associations or the Running Migration's path pins own needed contracts.
- The immutable allocation owner InstanceId is intentionally not an Instance FK:
  pre-Instance allocation and future non-destructive abandonment must not erase
  physical custody. At creation its exact origin Core must declare the storage
  in that Package. Origin Revision identity is thereafter audit provenance,
  not an installation-lifetime FK. Physical paths and service bytes are absent
  from SQL. Removing an origin Revision cannot remove allocation custody.
  In V7 a protected allocation still requires an extant owner Instance; only
  an unexposed creation preparation may precede that Instance. A missing owner
  is corruption, not permission to delete bytes. The later M7 schema must add
  explicit detached-custody evidence before making owner removal a valid state.
- Each published association belongs to an extant Instance with the same Package
  and to an allocation with that same immutable owner. Its declaration Revision
  must contain that typed declaration. Current declared identities have exactly
  one association referring to the current Core; unmapped undeclared identities
  retain the last published contract. Mapped sources are consumed/replaced
  atomically under PR-REQ-0319. Never infer an association from a path.
- An active resource's allocation equals its active storage association's
  allocation. A retained resource directly retains its historical allocation,
  even if a later storage association changes. Distinct active storages cannot
  alias one allocation; a Storage identity rename consumes the source association
  rather than preserving a second Storage alias. Resource references can still
  keep an allocation alive independently of its current Storage association.
- run_service_storage_pins attach only to Admitted Runs. They include every
  granted/read prerequisite/source dependency allocation and all prepared targets
  of the currently prepared Migration edge. Admitted target rows belong only to
  that current uncommitted edge and exactly cover its target declarations.
  Each resource target matches the allocation selected for its declared storage.
- Preparing a later edge happens after the preceding edge commit. Initial
  whole-chain symbolic preflight does not expose or allocate all future roots.
  Existing whole-path Revision pins remain; retained resource declaration
  Revisions are strongly held by their published associations throughout the
  continuing Mutate Run. Instance management cannot change them behind the Run.
- On edge commit, remove consumed source associations, apply target associations and mark all target allocations
  protected before removing that edge's target rows. Retain execution pins until
  terminal publication; their release never removes protection/custody rows.
- A committed transform edge has one run_service_edge_commits row, inserted in
  the same transaction as run_migration_boundaries. This is historical committed
  evidence, not an uncommitted target-ready/replay flag. It cannot authorize
  completion or clear risk when loaded by a new owner. Non-transform edges have
  no row. While Running, exact pinned Core validates this relationship; historical
  validation after Revision removal is structural unless that Core is available.
- No synthetic V1 successful Hook completion is stored for a target proposal.
  V7's transform evidence plus the committed boundary explains it. Ordinary
  completion records retain their historical meaning; terminal risk remains
  Clear after committed successful transformation. Any pre-existing guard remains.
- origin_run_id is audit identity only and intentionally has no Run FK; allocation
  custody must survive later Run retention policy. It is set only at creation
  by that accepted Migration, never reassigned. Unresolved recovery guards retain
  their existing strong Run roots independently. Published association Revision
  FKs intentionally pin the exact contracts needed for access/reattachment;
  origin Revision/Run audit IDs alone do not pin obsolete installations/history.

Strict readers reject drift, wrong owners, mixed associations, invalid typed
IDs, missing protection and impossible operation/boundary matrices. They must
not repair them by scanning files or normalizing into Interrupted. Presence is
always a fresh observation outside the authoritative database; changing live
bytes or protection bookkeeping alone does not advance InstanceStateVersion.

**Verification: PR-TEST-0340, PR-TEST-0346, PR-TEST-0347, PR-TEST-0349, PR-TEST-0350, PR-TEST-0353, PR-TEST-0368, PR-TEST-0374, PR-TEST-0375, PR-TEST-0376, PR-TEST-0377, PR-TEST-0378, PR-TEST-0380, PR-TEST-0382, PR-TEST-0383.**

PR-TEST-0340 covers exact SQL and structural FK behavior. The allocation,
association and real operation tests separately cover Domain invariants;
SQL shape alone is not proof of those invariants.

### PR-REQ-0324 - Allocation and protection protocol

Generate a cryptographically random, non-reused 128-bit allocation ID. In this
initial implementation profile its physical root is
`<Pactrun storage root>/service-storage/alloc-<32-lowercase-hex>`; no user path is
accepted as an allocation source. This mapping is a local persistence/runtime
layout, not Revision identity. Keep the path for the allocation lifetime;
moving the Pactrun storage root or attaching existing directories is unsupported
without a separately designed operation.

1. Under serialized writer admission, persist allocation plus preparation owner
   before filesystem creation. For a Migration, persist its audit Run origin and
   the target selection under the accepted, admitted owner.
2. Create only the private allocation root and complete the supported platform's
   directory persistence barriers. Never overwrite or adopt a pre-existing entry
   at a newly generated allocation path; fail safely and retain diagnosis.
3. For Instance creation, publish the Instance, all storage/resource associations
   and protection rows atomically, removing their preparation rows. The physical
   roots must already exist. For a Migration, protect newly exposed targets before
   Session disclosure; published source/retained allocations are already protected.
4. Any protection promotion is monotonic in M6.5. Deleting an execution, stage
   row, association or stale writer cannot remove the protection row or allocation.
   No service contents are moved into or mirrored by managed_input_payloads.

No SQLite transaction is held across service execution. The protocol is ordered
filesystem/SQLite publication, not a cross-system transaction. A missing or unsafe
root for a protected allocation is an operational failure; never silently recreate
it and thereby claim original live state was restored. Whole-storage write grants
authorize contents, not renaming/deleting the allocation root itself.

Directory creation success alone is not the durability barrier. If the supported
Windows fixed-NTFS or existing POSIX storage backend cannot complete its required
namespace persistence primitives, allocation must fail before publication/path
exposure; no weaker filesystem fallback is allowed. S2 must demonstrate the
actual adapter barriers with filesystem crash tests rather than infer them from
the in-memory DDL check or a successful mkdir call.

After confirmed preparation-owner loss, recheck under the allocation's internal
maintenance exclusion that it has no protection, published association, Running
pin or target reference. Remove only a known empty directory via non-traversing
rmdir and required barriers; then remove preparation/origin/allocation records
atomically. A missing directory is an idempotent empty-preparation case.
Unexpected contents, linked/unsafe paths or inconclusive ownership preserve the
records. Unknown/unrecorded directories are not automatically adopted or deleted.
No startup cleanup traverses protected storage. M7 finalization is the only future
destructive lifecycle, under its separately approved receipt protocol.

### Crash matrix

| Last durable event | Recovery |
| --- | --- |
| allocation intent; no directory yet | owner-live/inconclusive: leave; confirmed loss and no references: remove empty preparation records |
| directory created; no Instance publication or protection | remove only proven empty preparation after confirmed loss; otherwise preserve |
| Instance publication | all declared associations and protection survive; never expose a partial Instance |
| target protection/path exposure; no target commit | preserve allocation and origin evidence; no candidate adoption, target commit or service-byte cleanup |
| target_ready received; owner lost | no durable proposal authority; apply Open-risk manual recovery using last committed boundary |
| before edge transaction commit | all source/current associations remain; preserve exposed target allocation, do not replay |
| after edge transaction commit | target associations/boundary/risk clear agree; final edge is already Succeeded, otherwise later loss interrupts at the new boundary |
| empty directory removed; preparation record still exists | repeat absent-directory reconciliation; never promote to protected or infer a Run outcome |

**Verification: PR-TEST-0345, PR-TEST-0346, PR-TEST-0347, PR-TEST-0348, PR-TEST-0351, PR-TEST-0352, PR-TEST-0376.**

This is partial runtime evidence: no-replace, root-relative creation with adapter
barriers, SQL intent/protection, eager isolated Instance allocation and atomic
association publication under process interruption, and rejection of missing or
replaced prepared namespaces. Preparation maintenance tests cover confirmed-loss
retirement of absent unreferenced intents and conservative preservation for live
or inconclusive owners and existing objects. An intent alone does not distinguish
a created root from a failed no-replace collision, so these tests do not claim
post-crash ownership proof or automatic removal of existing empty directories.
They do not establish Hook access, Migration publication or power-loss safety.

### PR-REQ-0325 - Explicit V6-to-V7 upgrade

The writer initializes pristine storage as V7. Ordinary V6 opening must
require `pactrun storage upgrade`; only exact V6 upgrades directly. Exact V7 is
validation/no-op. V5 and earlier require a compatible build to reach V6 first.
Foreign, partial, drifted, unmarked nonempty and newer schemas are rejected.

Reuse the V6 transaction-bound admitted-writer quiescence protocol. Under the
serialized upgrade boundary, verify exact V6 and re-probe all committed writer
admissions. Live or inconclusive owners block; remove only confirmed-lost writer
admissions, not Running Runs. The old admission table must be empty before its
replacement; the upgrade owner publishes its V7 admission with the version.
Late V6 writers fail their transaction-bound schema check before old-schema work.

The transaction executes the DDL delta, validates its exact schema/FKs, and
publishes user_version 7 and the upgrader's admission. Failure leaves exact V6;
success leaves exact V7 with empty service tables. Preserve all existing Revision
bytes/digests, Instances, Inputs, protection floors, Runs, risk, guards, pins,
Snapshots, Artifacts and metadata. No storage directory is allocated for a V1
Instance during upgrade; no replay, reconciliation, rehash or implicit service
adoption occurs. Schema upgrade does not imply service allocation or recovery.

**Verification: PR-TEST-0205, PR-TEST-0298, PR-TEST-0300, PR-TEST-0341, PR-TEST-0342, PR-TEST-0343, PR-TEST-0344.**
