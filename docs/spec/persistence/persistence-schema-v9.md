---
title: Persistence Schema V9
---

# PersistenceSchemaV9

**Status: Implemented normative internal contract; non-Frozen.**

<!-- spec-navigation:start -->
Predecessor: [exact V8](./persistence-schema-v8.md).
Behavior owner: [managed object lifecycle](../behavior/managed-object-lifecycle.md).
Availability: [implementation record](../../development/managed-object-lifecycle-status.md).
<!-- spec-navigation:end -->

### PR-REQ-0345 - Lifecycle coordination activation

V9 preserves V8 objects and data while advancing `user_version` to 9 and replacing
the `writable_admissions.admitted_schema_version` CHECK from 8 to 9. All other
relational definitions remain exact V8. The independent boundary activates the
[lifecycle coordination contract](../behavior/managed-object-lifecycle.md).
It adds no tombstone, GC queue or compact historical-evidence format.

```sql
DROP TABLE writable_admissions;
CREATE TABLE writable_admissions (
    owner_session BLOB NOT NULL CHECK(length(owner_session) = 40),
    admitted_schema_version INTEGER NOT NULL CHECK(admitted_schema_version = 9),
    PRIMARY KEY (owner_session)
) STRICT, WITHOUT ROWID;
```

The private `.collection.lock` in runtime-content coordinates ordinary shared
store openings against exclusive collection/upgrade. It is non-authoritative OS
coordination, not an object, retention pin, saved Plan or GC work queue. Writable
bootstrap/upgrade durably establishes it before use; read-only opening never
creates it. A read-only schema preflight rejects unsupported stores before
coordination creation; SQL admission revalidates under the content guard. Bootstrap
establishes the durable guard before publishing V9. Content coordination always
precedes SQL transactions, including writer admission.

Only explicit exact-V8 to V9 upgrade is supported. Upgrade MUST check writer
quiescence, preserve objects, bytes, references and receipts, validate the exact
schema, and atomically publish the version. Ordinary commands and read-only
preview MUST NOT upgrade. Writable admissions are version-bound and revalidated
inside each write transaction; old sessions cannot continue writing across
activation. Unsupported or drifted sources fail without conversion.

Frozen Core, Hook, Snapshot and error contracts are unchanged. This is an internal
coordination boundary, not a release or promise to support every historical schema.

**Verification: PR-TEST-0300, PR-TEST-0456, PR-TEST-0457, PR-TEST-0468, PR-TEST-0473.**
