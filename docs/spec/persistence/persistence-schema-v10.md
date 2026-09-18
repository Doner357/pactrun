---
title: Persistence Schema V10
---

# PersistenceSchemaV10

**Status: Implemented and verified normative internal contract; non-Frozen.**

<!-- spec-navigation:start -->
Read [V9](./persistence-schema-v9.md), [Snapshot capacity](../behavior/m4-runtime-capabilities.md)
and [object lifetimes](../behavior/managed-object-lifecycle.md) first.
Delivery is tracked in the [implementation record](../../development/snapshot-capacity-and-restore-status.md).
<!-- spec-navigation:end -->

### PR-REQ-0347 - Immutable Snapshot and restored Input data references

New Snapshot payload bytes MUST be durably verified in the existing private
opaque blob store before the single Snapshot metadata transaction commits.
The historical `runtime-content` physical namespace MAY hold these immutable
values; this does not grant runtime membership, service authority, or a Revision
ownership relation. Physical addressing remains SHA-256; Frozen Snapshot,
Revision, Hook and error wire formats remain unchanged.

```sql
ALTER TABLE snapshot_blobs ADD COLUMN storage_kind INTEGER NOT NULL DEFAULT 0
    CHECK(storage_kind IN (0, 1));
ALTER TABLE managed_input_payloads ADD COLUMN content_digest BLOB
    CHECK(content_digest IS NULL OR length(content_digest) = 32);
DROP TABLE writable_admissions;
CREATE TABLE writable_admissions (
    owner_session BLOB NOT NULL CHECK(length(owner_session) = 40),
    admitted_schema_version INTEGER NOT NULL CHECK(admitted_schema_version = 10),
    PRIMARY KEY (owner_session)
) STRICT, WITHOUT ROWID;
```

Snapshot `storage_kind=0` retains exact legacy chunk semantics. Kind 1 selects
the immutable file named by the existing blob digest and MUST have no inline
chunks. New Snapshot publication MUST use kind 1. Missing/corrupt files are
failures, not permission to fall back, repair or reinterpret another format.
Stored lengths and the bytes actually delivered MUST be checked independently.

A null Managed Input `content_digest` retains legacy inline semantics. A
non-null digest selects immutable bytes and MUST have no inline chunks. Restore
MUST publish fresh Instance-scoped payload IDs, complete binding replacement,
protection, both guarded conflict-token consequences and its terminal outcome
in the existing atomic boundary; it MUST NOT copy service-sized data into WAL.
Legacy bound Snapshot values MAY be privately staged into the immutable store
before that boundary. Managed Input limits and sticky protection still apply.
Migration copying a payload with a changed protection floor MUST preserve its
representation and value while retaining the existing authorization rules.

The content-coordination guard MUST span blob publication through reference
commit, as it already does for runtime publication. A pre-commit crash MAY leave
unreferenced immutable files, never an authoritative partial Snapshot or Input.
GC MUST include Snapshot file references and all live/pinned Managed Input
payload references as strong roots, independently of producer/origin lifetime.
Deletion releases the logical reference; physical reclamation remains explicit
foreground GC. An unreliable reference catalog stops collection. Missing,
corrupt, conflicting or ambiguous data references MUST NOT be silently repaired.
Ordinary data-facing errors MUST NOT expose secret-derived digests or paths.
The trusted dedicated storage-root and host-access assumptions remain unchanged;
this adds neither encryption, secure erasure nor a secret vault.
A SQLite-only copy does not include referenced physical content. Snapshot export
still includes its exact payload closure; no whole-store backup facility is added.

Only explicit upgrade from exact V9 or the previously supported exact V8 is
allowed. V8 first receives the exact V9 coordination additions inside the same
upgrade transaction. The existing exclusive guard and writer-quiescence checks
MUST precede schema activation; version-bound admissions and exact validation
remain mandatory. Ordinary and read-only operations MUST report upgrade required,
not upgrade implicitly. Legacy rows and bytes remain inline without a bulk
rewrite. Failure before commit leaves the exact source schema; success publishes
V10 atomically. Older/foreign/drifted/newer sources remain refused.

**Verification: PR-TEST-0483, PR-TEST-0484, PR-TEST-0485, PR-TEST-0479, PR-TEST-0212, PR-TEST-0301, PR-TEST-0480.**
