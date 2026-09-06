---
title: Persistence Schema V2
---

# Persistence Schema V2

**Status: Implemented M1-D internal persistence schema and normative internal
contract; superseded as the current schema by PersistenceSchemaV4; non-Frozen,
non-public, and not a stable public support contract.**

This page defines the persistence contract followed by the integrated M1-D
implementation. It does not expose a public API, define an Export Bundle Format,
or change any Frozen identity or wire format. The metadata Domain semantics
remain owned by
[Identity and State](./identity-and-state.md); this page owns their exact
relational representation, repository transaction boundary, and V1-to-V2
internal migration.

### PR-REQ-0255 - Typed metadata mutation and repository contract

The crate-private repository MUST accept only typed metadata values and a
`RevisionMetadataMutationBatch` addressed to one already-persisted exact
Revision. A batch MAY contain idempotent reference-label and provenance add or
remove operations and semantic compare-and-set operations for presentation,
local aliases, the local current note, and the local current trust assessment.
It MUST NOT accept a generic metadata key, arbitrary JSON, a persistence row,
or a serialized Rust object as a Domain value.

Batch meaning MUST be independent of caller order. Completely identical
operations MAY coalesce. Adding and removing the same set-like tuple in one
batch is invalid, as are different compare-and-set mutations for the same typed
semantic key. Identical compare-and-set operations MAY coalesce. Pactrun MUST
reject a contradictory batch before publishing any part of it; it MUST NOT
interpret caller order as a sequence that resolves the contradiction.

Every current-state mutation MUST carry `expected` and `desired` typed states,
each exactly `Absent | Present(value)`. While holding the database write
transaction, Pactrun MUST compare the current typed semantic state as follows:

1. if current equals desired, the operation is an idempotent success;
2. otherwise, if current equals expected, Pactrun applies desired;
3. otherwise, the complete batch conflicts and MUST roll back.

This comparison intentionally has no history-sensitive or ABA guarantee. M1-D
has no metadata generation, version token, event history, or audit order. If a
value changes from A to B and back to A, a later operation expecting A cannot
distinguish that history from no intervening mutation. Stronger concurrency is
a future design gate.

The repository MUST use `BEGIN IMMEDIATE`. Validation failure, a missing target
Revision, a malformed or absent presentation target, alias conflict, CAS
conflict, constraint failure, or process loss before commit MUST publish no
part of the batch. A successful commit is the durable metadata boundary. Loss
after commit but before the response MAY be retried: set-like operations and
current-equals-desired handling MUST converge on the committed semantic state.
The batch is not required to share a transaction with initial Revision
publication; future installation or import orchestration may coordinate those
boundaries without changing this contract.

Logical load MUST validate every stored value against its typed Domain syntax,
validate every presentation target against the strict-decoded canonical
`RevisionCoreV1`, reject non-canonical optional encodings, and return the same
typed semantic state that was committed. Any impossible enum rank, invalid
UTF-8, malformed URI, empty present value, invalid target, duplicate semantic
tuple, or derived-state mismatch is corruption rather than another tolerated
representation.

Every deterministic lookup or enumeration MUST use the comparators in
PR-REQ-0249 through PR-REQ-0253. SQL queries MUST state every correctness-
relevant `ORDER BY` component explicitly. Cross-table presentation and metadata
enumeration MUST merge results with the same Domain comparator. SQL and Domain
ordering MUST be parity-equivalent and MUST NOT use rowid, insertion order,
timestamps, query-plan order, locale collation, or SQLite's NULL ordering.

The Revision-wide metadata-kind rank is, in order: reference-label binding,
presentation, source-URI claim, publisher-attribution claim, attribution claim,
local alias, local current note, and local current trust. Within each kind, the
complete type comparator is:

- reference label: label bytes, Revision identity, source rank, then the source
  tuple in declaration order;
- presentation: Revision identity, target rank, target components in declaration
  order, then field rank;
- each provenance relation: Revision identity, claim rank, then its complete
  typed claim tuple in declaration order;
- local alias: alias bytes, then target Revision identity;
- local current note and trust: their singleton Revision identity only.

Optional tuple components compare presence rank before present payload. Exact
text uses BLOB byte order, and Revision identity compares `package_id` before
`revision_content_digest`. A label lookup fixes the exact label, deduplicates
source rows by Revision identity, and orders only the resulting distinct
Revision identities. These ranks and components are semantic constants rather
than implementation enumeration layouts.

An authorized exact Revision deletion MUST remove all of that Revision's M1-D
subordinate rows in the same database transaction. Metadata MUST NOT pin a
Revision or require a separate purge. Cascading one target's label bindings
MUST NOT remove bindings that target another Revision.

**Verification: PR-TEST-0062, PR-TEST-0063, PR-TEST-0064, PR-TEST-0065,
PR-TEST-0066, PR-TEST-0067.**

### PR-REQ-0256 - Exact PersistenceSchemaV2

PersistenceSchemaV2 MUST retain the exact V1 `packages`, `revisions`, and
`revision_runtime_content_refs` definitions reproduced below, add exactly the
new tables below them, use application ID `0x50414354`, and use SQLite user
version `2`. All tables are `STRICT` and `WITHOUT ROWID`. Correctness does not
depend on a performance-only index, so no such index is part of the internal
contract.

Textual Domain values are stored as their exact UTF-8 bytes in BLOB columns.
Domain validation owns UTF-8 and type syntax. BLOB comparison therefore has the
same unsigned lexicographic byte ordering as the authoritative text comparator.
No semantic key contains SQL NULL. Optional tuple components use a NOT NULL
presence discriminator and a unique canonical zero-length BLOB for absence;
present values must be non-empty. Closed integer ranks are semantic schema
constants, not Rust enum discriminants.

```sql
CREATE TABLE packages (
    package_id BLOB NOT NULL
        CHECK(length(package_id) = 16),

    PRIMARY KEY (package_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE revisions (
    package_id BLOB NOT NULL
        CHECK(length(package_id) = 16),

    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),

    core_jcs BLOB NOT NULL,
    runtime_content_jcs BLOB NOT NULL,

    PRIMARY KEY (
        package_id,
        revision_content_digest
    ),

    FOREIGN KEY (package_id)
        REFERENCES packages(package_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_runtime_content_refs (
    package_id BLOB NOT NULL
        CHECK(length(package_id) = 16),

    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),

    content_id TEXT NOT NULL COLLATE BINARY,

    blob_digest BLOB NOT NULL
        CHECK(length(blob_digest) = 32),

    PRIMARY KEY (
        package_id,
        revision_content_digest,
        content_id
    ),

    FOREIGN KEY (
        package_id,
        revision_content_digest
    )
    REFERENCES revisions(
        package_id,
        revision_content_digest
    )
) STRICT, WITHOUT ROWID;

-- PersistenceSchemaV2 additions begin here.
CREATE TABLE revision_reference_label_bindings (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    label_utf8 BLOB NOT NULL CHECK(length(label_utf8) > 0),
    source_rank INTEGER NOT NULL CHECK(source_rank BETWEEN 0 AND 3),
    publisher_name_utf8 BLOB NOT NULL,
    publisher_namespace_present INTEGER NOT NULL
        CHECK(publisher_namespace_present IN (0, 1)),
    publisher_namespace_utf8 BLOB NOT NULL,
    source_uri_ascii BLOB NOT NULL,

    CHECK (
        (source_rank = 0
            AND length(publisher_name_utf8) = 0
            AND publisher_namespace_present = 0
            AND length(publisher_namespace_utf8) = 0
            AND length(source_uri_ascii) = 0)
        OR
        (source_rank = 1
            AND length(publisher_name_utf8) = 0
            AND publisher_namespace_present = 0
            AND length(publisher_namespace_utf8) = 0
            AND length(source_uri_ascii) > 0)
        OR
        (source_rank = 2
            AND length(publisher_name_utf8) > 0
            AND (
                (publisher_namespace_present = 0
                    AND length(publisher_namespace_utf8) = 0)
                OR
                (publisher_namespace_present = 1
                    AND length(publisher_namespace_utf8) > 0)
            )
            AND length(source_uri_ascii) = 0)
        OR
        (source_rank = 3
            AND length(publisher_name_utf8) > 0
            AND (
                (publisher_namespace_present = 0
                    AND length(publisher_namespace_utf8) = 0)
                OR
                (publisher_namespace_present = 1
                    AND length(publisher_namespace_utf8) > 0)
            )
            AND length(source_uri_ascii) > 0)
    ),

    PRIMARY KEY (
        package_id,
        revision_content_digest,
        label_utf8,
        source_rank,
        publisher_name_utf8,
        publisher_namespace_present,
        publisher_namespace_utf8,
        source_uri_ascii
    ),

    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_presentation_current (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    display_name_utf8 BLOB,
    summary_utf8 BLOB,
    description_utf8 BLOB,
    help_utf8 BLOB,

    CHECK(display_name_utf8 IS NULL OR length(display_name_utf8) > 0),
    CHECK(summary_utf8 IS NULL OR length(summary_utf8) > 0),
    CHECK(description_utf8 IS NULL OR length(description_utf8) > 0),
    CHECK(help_utf8 IS NULL OR length(help_utf8) > 0),
    CHECK(display_name_utf8 IS NOT NULL OR summary_utf8 IS NOT NULL
        OR description_utf8 IS NOT NULL OR help_utf8 IS NOT NULL),

    PRIMARY KEY (package_id, revision_content_digest),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_input_presentation_current (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    input_id_utf8 BLOB NOT NULL CHECK(length(input_id_utf8) > 0),
    display_name_utf8 BLOB,
    summary_utf8 BLOB,
    description_utf8 BLOB,
    help_utf8 BLOB,

    CHECK(display_name_utf8 IS NULL OR length(display_name_utf8) > 0),
    CHECK(summary_utf8 IS NULL OR length(summary_utf8) > 0),
    CHECK(description_utf8 IS NULL OR length(description_utf8) > 0),
    CHECK(help_utf8 IS NULL OR length(help_utf8) > 0),
    CHECK(display_name_utf8 IS NOT NULL OR summary_utf8 IS NOT NULL
        OR description_utf8 IS NOT NULL OR help_utf8 IS NOT NULL),

    PRIMARY KEY (package_id, revision_content_digest, input_id_utf8),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_action_presentation_current (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    action_id_utf8 BLOB NOT NULL CHECK(length(action_id_utf8) > 0),
    display_name_utf8 BLOB,
    summary_utf8 BLOB,
    description_utf8 BLOB,
    help_utf8 BLOB,

    CHECK(display_name_utf8 IS NULL OR length(display_name_utf8) > 0),
    CHECK(summary_utf8 IS NULL OR length(summary_utf8) > 0),
    CHECK(description_utf8 IS NULL OR length(description_utf8) > 0),
    CHECK(help_utf8 IS NULL OR length(help_utf8) > 0),
    CHECK(display_name_utf8 IS NOT NULL OR summary_utf8 IS NOT NULL
        OR description_utf8 IS NOT NULL OR help_utf8 IS NOT NULL),

    PRIMARY KEY (package_id, revision_content_digest, action_id_utf8),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_action_parameter_presentation_current (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    action_id_utf8 BLOB NOT NULL CHECK(length(action_id_utf8) > 0),
    parameter_id_utf8 BLOB NOT NULL CHECK(length(parameter_id_utf8) > 0),
    display_name_utf8 BLOB,
    summary_utf8 BLOB,
    description_utf8 BLOB,
    help_utf8 BLOB,

    CHECK(display_name_utf8 IS NULL OR length(display_name_utf8) > 0),
    CHECK(summary_utf8 IS NULL OR length(summary_utf8) > 0),
    CHECK(description_utf8 IS NULL OR length(description_utf8) > 0),
    CHECK(help_utf8 IS NULL OR length(help_utf8) > 0),
    CHECK(display_name_utf8 IS NOT NULL OR summary_utf8 IS NOT NULL
        OR description_utf8 IS NOT NULL OR help_utf8 IS NOT NULL),

    PRIMARY KEY (
        package_id,
        revision_content_digest,
        action_id_utf8,
        parameter_id_utf8
    ),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_managed_output_presentation_current (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    action_id_utf8 BLOB NOT NULL CHECK(length(action_id_utf8) > 0),
    output_id_utf8 BLOB NOT NULL CHECK(length(output_id_utf8) > 0),
    display_name_utf8 BLOB,
    summary_utf8 BLOB,
    description_utf8 BLOB,
    help_utf8 BLOB,

    CHECK(display_name_utf8 IS NULL OR length(display_name_utf8) > 0),
    CHECK(summary_utf8 IS NULL OR length(summary_utf8) > 0),
    CHECK(description_utf8 IS NULL OR length(description_utf8) > 0),
    CHECK(help_utf8 IS NULL OR length(help_utf8) > 0),
    CHECK(display_name_utf8 IS NOT NULL OR summary_utf8 IS NOT NULL
        OR description_utf8 IS NOT NULL OR help_utf8 IS NOT NULL),

    PRIMARY KEY (
        package_id,
        revision_content_digest,
        action_id_utf8,
        output_id_utf8
    ),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_snapshot_operation_presentation_current (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    operation_rank INTEGER NOT NULL CHECK(operation_rank IN (0, 1)),
    display_name_utf8 BLOB,
    summary_utf8 BLOB,
    description_utf8 BLOB,
    help_utf8 BLOB,

    CHECK(display_name_utf8 IS NULL OR length(display_name_utf8) > 0),
    CHECK(summary_utf8 IS NULL OR length(summary_utf8) > 0),
    CHECK(description_utf8 IS NULL OR length(description_utf8) > 0),
    CHECK(help_utf8 IS NULL OR length(help_utf8) > 0),
    CHECK(display_name_utf8 IS NOT NULL OR summary_utf8 IS NOT NULL
        OR description_utf8 IS NOT NULL OR help_utf8 IS NOT NULL),

    PRIMARY KEY (package_id, revision_content_digest, operation_rank),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_snapshot_parameter_presentation_current (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    operation_rank INTEGER NOT NULL CHECK(operation_rank IN (0, 1)),
    parameter_id_utf8 BLOB NOT NULL CHECK(length(parameter_id_utf8) > 0),
    display_name_utf8 BLOB,
    summary_utf8 BLOB,
    description_utf8 BLOB,
    help_utf8 BLOB,

    CHECK(display_name_utf8 IS NULL OR length(display_name_utf8) > 0),
    CHECK(summary_utf8 IS NULL OR length(summary_utf8) > 0),
    CHECK(description_utf8 IS NULL OR length(description_utf8) > 0),
    CHECK(help_utf8 IS NULL OR length(help_utf8) > 0),
    CHECK(display_name_utf8 IS NOT NULL OR summary_utf8 IS NOT NULL
        OR description_utf8 IS NOT NULL OR help_utf8 IS NOT NULL),

    PRIMARY KEY (
        package_id,
        revision_content_digest,
        operation_rank,
        parameter_id_utf8
    ),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_migration_presentation_current (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    source_revision_content_digest BLOB NOT NULL
        CHECK(length(source_revision_content_digest) = 32),
    display_name_utf8 BLOB,
    summary_utf8 BLOB,
    description_utf8 BLOB,
    help_utf8 BLOB,

    CHECK(display_name_utf8 IS NULL OR length(display_name_utf8) > 0),
    CHECK(summary_utf8 IS NULL OR length(summary_utf8) > 0),
    CHECK(description_utf8 IS NULL OR length(description_utf8) > 0),
    CHECK(help_utf8 IS NULL OR length(help_utf8) > 0),
    CHECK(display_name_utf8 IS NOT NULL OR summary_utf8 IS NOT NULL
        OR description_utf8 IS NOT NULL OR help_utf8 IS NOT NULL),

    PRIMARY KEY (
        package_id,
        revision_content_digest,
        source_revision_content_digest
    ),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_cleanup_presentation_current (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    display_name_utf8 BLOB,
    summary_utf8 BLOB,
    description_utf8 BLOB,
    help_utf8 BLOB,

    CHECK(display_name_utf8 IS NULL OR length(display_name_utf8) > 0),
    CHECK(summary_utf8 IS NULL OR length(summary_utf8) > 0),
    CHECK(description_utf8 IS NULL OR length(description_utf8) > 0),
    CHECK(help_utf8 IS NULL OR length(help_utf8) > 0),
    CHECK(display_name_utf8 IS NOT NULL OR summary_utf8 IS NOT NULL
        OR description_utf8 IS NOT NULL OR help_utf8 IS NOT NULL),

    PRIMARY KEY (package_id, revision_content_digest),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_source_uri_claims (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    source_uri_ascii BLOB NOT NULL CHECK(length(source_uri_ascii) > 0),

    PRIMARY KEY (
        package_id,
        revision_content_digest,
        source_uri_ascii
    ),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_publisher_attribution_claims (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    publisher_name_utf8 BLOB NOT NULL
        CHECK(length(publisher_name_utf8) > 0),
    publisher_namespace_present INTEGER NOT NULL
        CHECK(publisher_namespace_present IN (0, 1)),
    publisher_namespace_utf8 BLOB NOT NULL,
    source_uri_present INTEGER NOT NULL CHECK(source_uri_present IN (0, 1)),
    source_uri_ascii BLOB NOT NULL,

    CHECK (
        (publisher_namespace_present = 0
            AND length(publisher_namespace_utf8) = 0)
        OR
        (publisher_namespace_present = 1
            AND length(publisher_namespace_utf8) > 0)
    ),
    CHECK (
        (source_uri_present = 0 AND length(source_uri_ascii) = 0)
        OR
        (source_uri_present = 1 AND length(source_uri_ascii) > 0)
    ),

    PRIMARY KEY (
        package_id,
        revision_content_digest,
        publisher_name_utf8,
        publisher_namespace_present,
        publisher_namespace_utf8,
        source_uri_present,
        source_uri_ascii
    ),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_attribution_claims (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    attribution_text_utf8 BLOB NOT NULL
        CHECK(length(attribution_text_utf8) > 0),
    source_uri_present INTEGER NOT NULL CHECK(source_uri_present IN (0, 1)),
    source_uri_ascii BLOB NOT NULL,

    CHECK (
        (source_uri_present = 0 AND length(source_uri_ascii) = 0)
        OR
        (source_uri_present = 1 AND length(source_uri_ascii) > 0)
    ),

    PRIMARY KEY (
        package_id,
        revision_content_digest,
        attribution_text_utf8,
        source_uri_present,
        source_uri_ascii
    ),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_local_aliases (
    alias_utf8 BLOB NOT NULL CHECK(length(alias_utf8) > 0),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),

    PRIMARY KEY (alias_utf8),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_local_current_notes (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    note_utf8 BLOB NOT NULL CHECK(length(note_utf8) > 0),

    PRIMARY KEY (package_id, revision_content_digest),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_local_current_trust (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    trust_rank INTEGER NOT NULL CHECK(trust_rank IN (0, 1)),

    PRIMARY KEY (package_id, revision_content_digest),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
```

For a pristine database, the complete block above is the exact implemented
schema. For an exact V1 database, migration executes only the additions after
the marked boundary. The presentation operation ranks are Capture `0` and Restore `1`; the trust
ranks are Trusted `0` and Distrusted `1`. Table identity supplies the remaining
presentation target ranks from PR-REQ-0251. A presentation row represents the
four current fields for one target. SQL NULL represents an absent field only
inside that unique target row. The all-NULL check ensures the row itself cannot
be a second representation of total absence; when all four fields are absent,
the row MUST not exist. Local note, local trust, and alias absence are likewise
represented only by row absence.

The source discriminators, presence flags, non-null payloads, and checks above
ensure that SQL's NULL-distinct uniqueness behavior cannot create duplicate
semantic tuples. A zero-length payload is a storage sentinel only in a branch
whose discriminator says `Absent`; it is never a valid present Domain value.

## Required repository ordering

Every ordered SQL read MUST include an explicit clause equivalent to the
applicable clause below. A query that fixes a leading component MAY omit only
that constant component; it MUST retain every remaining component. No textual
column uses a locale collation.

```sql
-- Complete reference-label binding enumeration
ORDER BY label_utf8, package_id, revision_content_digest, source_rank,
         publisher_name_utf8, publisher_namespace_present,
         publisher_namespace_utf8, source_uri_ascii

-- Distinct-target lookup for one exact label
ORDER BY package_id, revision_content_digest

-- Presentation rows, before Domain expansion by field rank
ORDER BY package_id, revision_content_digest                         -- Revision
ORDER BY package_id, revision_content_digest, input_id_utf8          -- Input
ORDER BY package_id, revision_content_digest, action_id_utf8         -- Action
ORDER BY package_id, revision_content_digest, action_id_utf8,
         parameter_id_utf8                                           -- Action parameter
ORDER BY package_id, revision_content_digest, action_id_utf8,
         output_id_utf8                                              -- Managed output
ORDER BY package_id, revision_content_digest, operation_rank         -- Snapshot operation
ORDER BY package_id, revision_content_digest, operation_rank,
         parameter_id_utf8                                           -- Snapshot parameter
ORDER BY package_id, revision_content_digest,
         source_revision_content_digest                              -- Migration edge
ORDER BY package_id, revision_content_digest                         -- Cleanup

-- Provenance relations; cross-table Domain merge adds claim rank
ORDER BY package_id, revision_content_digest, source_uri_ascii
ORDER BY package_id, revision_content_digest, publisher_name_utf8,
         publisher_namespace_present, publisher_namespace_utf8,
         source_uri_present, source_uri_ascii
ORDER BY package_id, revision_content_digest, attribution_text_utf8,
         source_uri_present, source_uri_ascii

-- Local metadata
ORDER BY alias_utf8, package_id, revision_content_digest
ORDER BY package_id, revision_content_digest                         -- Note or trust
```

For presentation, the repository MUST merge table families by the target rank
in PR-REQ-0251, compare their typed target components, and expand non-NULL
columns by field rank. For provenance, it MUST merge table families by the
claim rank in PR-REQ-0252. A Revision-wide read MUST then merge kinds by the
metadata-kind rank in PR-REQ-0255. Repository load MUST independently verify
that the final Domain sequence is ordered by the same comparator; physical
primary-key iteration is not a substitute for these clauses.

**Verification: PR-TEST-0059, PR-TEST-0063, PR-TEST-0067.**

### PR-REQ-0257 - V1-to-V2 migration and validation

Opening persistence MUST classify a database as pristine, exact V1, exact V2,
or inadmissible. A pristine database has application ID zero, user version zero,
and no non-SQLite schema objects. An exact V1 or V2 database has Pactrun's
application ID, its matching user version, and the complete exact schema for
that version. Foreign, unmarked non-empty, drifted, partial, and newer databases
MUST be rejected.

After WAL establishment, connection configuration, and the existing
caller-provisioned-root validation, Pactrun MUST acquire `BEGIN IMMEDIATE` and
re-read ownership markers and schema. A pristine database MUST create the full
V2 schema directly. An exact V1 database MUST execute all V2 additions above,
validate the resulting complete V2 schema, and update `user_version` to 2 in
that same transaction. An exact V2 database MUST be validated without a
mutation. Concurrent initializers or migrators MUST converge through SQLite
locking; M1-D MUST NOT add a second cross-process lock protocol.

A crash or error before commit MUST leave the earlier exact schema and marker;
a crash after commit MUST leave exact V2. No partial table set, premature
version marker, or best-effort repair is admissible. Reopening after either
boundary MUST be idempotent. Migration MUST preserve every V1 Package and
Revision identity, both canonical Revision component byte strings, and every
closure-derived runtime-content reference. It MUST NOT read, rewrite, or infer
new metadata from canonical Revision JSON.

Schema validation MUST compare tables, columns, declared types, NOT NULL and
CHECK constraints, primary keys, foreign keys and delete actions, `STRICT` and
`WITHOUT ROWID` options, and every correctness-bearing index that a separately
approved schema revision adds. Markers alone are insufficient. Database and WAL
durability retain the M1-C platform assumptions; migration does not claim
arbitrary hardware power-loss certification.

**Verification: PR-TEST-0060, PR-TEST-0061.**

## Planned crate-private typed interface

The implementation slice must derive, rather than redefine, an interface
equivalent to:

```text
CurrentState<T> = Absent | Present(T)

RevisionMetadataMutationBatch
|- AddReferenceLabel(ReferenceLabelBinding)
|- RemoveReferenceLabel(ReferenceLabelBinding)
|- CompareAndSetPresentation(target, field, expected, desired)
|- AddProvenance(ProvenanceClaim)
|- RemoveProvenance(ProvenanceClaim)
|- CompareAndSetLocalAlias(alias, expected_state, desired_state)
|- CompareAndSetLocalNote(expected, desired)
`- CompareAndSetLocalTrust(expected, desired)

apply_revision_metadata_batch(RevisionIdentity, RevisionMetadataMutationBatch)
load_revision_metadata(RevisionIdentity) -> RevisionMetadataView
lookup_reference_label(ReferenceLabel) -> ordered distinct RevisionIdentity[]
lookup_local_alias(LocalAlias) -> RevisionIdentity?
```

The concrete Rust names remain crate-private implementation detail. The typed
operations, states, equality, conflict, validation, atomicity, and ordering are
the required interface semantics. Alias states are
`CurrentState<RevisionIdentity>`; every `Present` identity involved in an
accepted replacement MUST already exist, and the batch's target Revision is the
desired target for a present binding or the current target for a clear. No
stable public Rust API, CLI spelling, or wire contract is created here.

## Deferred work

This internal schema does not define Export Bundle serialization or merge,
Package or Instance metadata, `LocalInstall`, generic timestamps, localization,
fuzzy lookup, additional provenance claims, trust policy or history, note history,
history-sensitive CAS or ABA detection, stable new error codes, cross-domain
Revision-plus-metadata publication, CLI, or ServiceStorage persistence.
