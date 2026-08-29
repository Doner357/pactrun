---
title: Identity and State
---

# Identity and State

**Status: Normative architecture.**

This page owns internal identity, canonicalization, metadata, Instance state,
and managed binding invariants. User-visible behavior is summarized in the
[product behavior](../product-behavior/packages-revisions-and-instances.md).

## Revision identity

### PR-REQ-0011 - Stable Package lineage

`PackageId` MUST be opaque, stable, and independent of content. It MUST survive
Revision changes, presentation or publisher changes, export, and import. A fork
that becomes a new lineage MUST be explicitly re-identified.

**Verification: Pending automated coverage.**

### PR-REQ-0012 - Operational content digest

`RevisionContentDigest` MUST identify Pactrun-managed operational semantics and
immutable owned runtime content. It MUST exclude `PackageId`, display content,
publisher claims, labels, source location, install time, local aliases, notes,
and local trust decisions.

**Verification: Pending automated coverage.**

### PR-REQ-0013 - Exact Revision identity

An exact `RevisionIdentity` MUST be the pair of `PackageId` and
`RevisionContentDigest`. Equal content digests under different Package IDs MUST
remain distinct Revision identities.

**Verification: Pending automated coverage.**

### PR-REQ-0014 - RevisionCore projection

`RevisionCore` MUST be a versioned, identity-bearing semantic projection of a
validated `NormalizedPackDefinition`. It MUST NOT be defined by serializing a
Rust type, database row, authoring AST, or the entire normalized authoring
model.

The projection MUST include every field that can affect invocation semantics,
parameter behavior, compiler or executor behavior, operation access, Hook
Session authority, Inputs, Snapshot, Migration, Cleanup, failures, recovery, or
service-facing runtime behavior. Presentation, attribution, provenance, and
local-management data MUST remain outside it.

**Verification: Pending automated coverage.**

### PR-REQ-0015 - Runtime content closure identity

Runtime content identity MUST encode logical runtime location or role, object
kind, immutable content identity, and runtime-semantic metadata. Hashing an
unordered set of blob digests is insufficient. Source-only paths, timestamps,
inodes, and ownership metadata MUST NOT affect identity unless Pactrun
explicitly defines them as runtime semantics.

**Verification: Pending automated coverage.**

### PR-REQ-0016 - Semantic normalization

Pactrun MUST apply semantic defaults, validate, and normalize equivalent forms
before projecting and encoding `RevisionCore`. Unordered domain collections
MUST be sorted by a stable semantic key; ordered collections MUST preserve their
order; duplicate semantic keys MUST be rejected.

**Verification: Pending automated coverage.**

### PR-REQ-0017 - RevisionCoreFormatV1 hash contract

`RevisionCoreFormatV1` MUST use a closed semantic schema, explicit domain
separation and framing, RFC 8785 JSON Canonicalization Scheme bytes, and a fixed
SHA-256 profile represented by a self-describing digest such as
`sha256:<value>`. JCS MUST NOT replace Pactrun semantic normalization.

The exact Frozen contract is defined by
[Revision Core Format V1](../package-contracts/revision-core-format-v1.md).

**Verification: Pending automated coverage.**

### PR-REQ-0018 - Stable published identity

Internal schema, type, serializer, library, and CLI changes MUST NOT alter the
identity of an already published RevisionCore format. Unknown or unsupported
format versions MUST be rejected instead of interpreted on a best-effort basis.

**Verification: Pending automated coverage.**

## Production identity and Revision Core foundation

### PR-REQ-0225 - Production opaque identity primitives

The production `PackageId`, `InstanceId`, and `InstanceStateVersion` types MUST
each contain 128 opaque bits and use exactly 32 lowercase hexadecimal
characters when rendered or parsed. Pactrun MUST generate new values using the
operating system cryptographic random source. `InstanceStateVersion` MUST NOT
expose arithmetic or ordering semantics.

`RevisionContentDigest` MUST contain a SHA-256 result and use `sha256:` followed
by 64 lowercase hexadecimal characters. `RevisionIdentity` MUST remain the
structured pair of `PackageId` and `RevisionContentDigest`; a flattened display
spelling is not a second identity. M1-A does not define persistence or a generic
Serde representation for these opaque identities.

**Verification: PR-TEST-0039.**

### PR-REQ-0226 - Production Revision content component boundary

The production Revision content model MUST preserve two sibling semantic and
hash components: `RevisionCoreV1` and
`RuntimeContentClosureIdentityV1`. `ValidatedRevisionContentV1` MUST contain the
validated pair. Neither component may contain the other, and neither is
persistence metadata. Cross-component Hook `ContentId` validation belongs to
the validated-pair boundary.

For M1-A, a Hook content reference exists when it resolves to exactly one entry
in the supplied semantic runtime-content closure. Direct launch additionally
requires that descriptor to be executable. Interpreter script launch does not
gain an executable-bit requirement. This validation MUST perform no filesystem,
content-store, materialization, permission, or launcher lookup.

Production projection MUST be deterministic and normalize the Frozen semantic
sets while preserving ordered Hook arguments. Semantic JSON decoding is an
internal codec and conformance facility, not a Pack authoring, import, or
persistence contract. M2 authoring MUST enter through
`NormalizedPackDefinition` and the typed projection boundary rather than this
internal parser.

**Verification: PR-TEST-0040, PR-TEST-0041, PR-TEST-0042.**

### PR-REQ-0227 - Production Revision Core codec conformance

The production wire view MUST contain exactly the identity-bearing fields in
Frozen `RevisionCoreFormatV1`. Typed invariants and implementation helpers MUST
NOT enter canonical JSON. In particular, runtime files serialize the Frozen
`id`, `path`, `kind: regular_file`, `blob_digest`, and `executable` fields;
runtime content has no independent format-version field.

Semantic decoding MUST reject duplicate decoded property names, unknown fields,
invalid UTF-8 or Unicode scalar sequences, invalid exact integers, non-finite
binary64 values, unsafe integers, and excessive input depth. Raw number tokens
MUST remain available until their schema type is known. Canonical production
decode MUST additionally require byte-for-byte equality with the re-encoded RFC
8785 JCS representation. The digest API MUST accept only a validated sibling
pair and apply the Frozen dual-component framing and SHA-256 profile.

Production code MUST NOT depend on or call Pactrun-owned xtask parser,
normalizer, or verifier implementation. Sharing suitable commodity Rust
dependencies does not violate this boundary. Every applicable Frozen negative
vector MUST be rejected, but an exact `PactrunErrorRefV1` is required only when
the vector's spelling is registered under `revision_core_format_v1` in the
Frozen error catalog. Internal or verifier-only spellings MUST remain
non-normative and MUST NOT be mapped to a speculative generic error.

**Verification: PR-TEST-0040, PR-TEST-0043, PR-TEST-0044, PR-TEST-0045,
PR-TEST-0046.**

### PR-REQ-0228 - Immutable runtime-content blob-store boundary

The production runtime-content store MUST remain crate-private and store opaque
bytes addressed only by their expected SHA-256 blob digest. Its flat physical
namespace, staging names, publication lock, and filesystem paths are local
implementation details and MUST NOT enter `RuntimeContentClosureIdentityV1`,
Revision identity, canonical bytes, or `RevisionContentDigest`. `ContentId`,
runtime path, executable role, and other semantic descriptor fields MUST NOT
participate in physical blob addressing.

Semantic runtime-content membership and physical blob availability are
separate validations. Once Pactrun publishes a correct blob under a digest,
Pactrun MUST NOT overwrite, mutate, replace, truncate, or repair it in place.
This immutability is a store-protocol and ownership invariant, not a filesystem
read-only-bit or ACL guarantee.

**Verification: PR-TEST-0047, PR-TEST-0048, PR-TEST-0049, PR-TEST-0051.**

### PR-REQ-0229 - Verified durable blob publication

Runtime-content publication MUST stream bytes into a create-new staging file in
the already-existing final directory, calculate SHA-256 and a checked `u64`
length, persist the staging file, and reject an incoming digest mismatch before
publication. The final publication operation MUST be no-replace. A correct
existing regular file is idempotent success only after Pactrun fully verifies
its digest and completes the supported platform persistence barriers; a corrupt
or unacceptable existing entry MUST be rejected and MUST NOT be repaired.

Publication success MUST be returned only after every documented persistence
primitive required by the supported platform profile has succeeded. On Windows
the supported profile is a local fixed NTFS volume and publication uses the
same open write-through staging handle for no-replace handle-based rename. On
supported Linux local filesystems, publication uses no-replace rename, file
synchronization, and root-directory synchronization; M1-B's initial Linux
profile is limited to ext4, XFS, Btrfs, and ZFS. Other Windows and Linux
filesystem profiles MUST be rejected unless their persistence protocol is
separately established. The dedicated store root MUST already exist; M1-B does
not claim to durably create that root or a persistent hierarchy.

The final namespace MUST contain either no digest entry or complete immutable
bytes. A failure after final publication MAY leave a correct unreferenced blob,
but MUST NOT return the successful value that authorizes later durable reference
publication. A later identical put MUST be able to verify that object, complete
the persistence barriers, and return success. Durability is relative to
successful documented operating-system, filesystem, virtualization, and
storage persistence contracts; process-crash tests are not hardware power-loss
certification.

**Verification: PR-TEST-0047, PR-TEST-0048, PR-TEST-0049, PR-TEST-0050.**

### PR-REQ-0230 - Physical runtime-content availability

Physical availability validation MUST deduplicate a
`RuntimeContentClosureIdentityV1` by blob digest and fully verify each distinct
blob once. Multiple content identities, runtime paths, and executable roles MAY
reference one physical blob. Availability validation MUST NOT change semantic
closure bytes, Revision Core bytes, or `RevisionContentDigest`.

A verified blob means that the opened regular-file object was fully hashed at
acquisition under a dedicated trusted-root and cooperative-writer model. M1-B
does not provide hostile post-verification tamper resistance or an operating
system sandbox. Durable blob publication MUST precede any future database
reference publication. M1-B provides no deletion, garbage collection,
reachability, pin, or database-reference behavior.

**Verification: PR-TEST-0048, PR-TEST-0051.**

### PR-REQ-0231 - SQLite persistence ownership and bootstrap

M1-C MUST use an independently versioned `PersistenceSchemaV1` in a
caller-provisioned, dedicated local storage root. The database and runtime
content child directories MUST already exist. On Windows the supported profile
is local fixed NTFS; on Linux it is limited to the ext4, XFS, Btrfs, and ZFS
profiles established by M1-B. M1-C MUST NOT claim to create those persistent
directories durably.

The SQLite database MUST use application ID `0x50414354`, user version `1`, WAL
journal mode, `synchronous=FULL`, foreign-key enforcement, and a five-second
busy timeout. In WAL mode, the main database and a live WAL MAY together carry
committed database state. The SHM WAL index is reconstructible coordination and
cache state, not authoritative Pactrun Domain state. M1-C does not define live
filesystem-copy backup; a future backup facility MUST use a SQLite-supported
consistent backup or checkpoint mechanism.

Before claiming a database, Pactrun MUST inspect its application ID, user
version, and non-SQLite schema objects. Only an exact V1 database or a pristine
database with application ID zero, user version zero, and no user objects is
admissible. WAL establishment MUST return exactly `wal`. Pactrun MUST then use
`BEGIN IMMEDIATE`, re-read the ownership markers and schema while holding the
write transaction, and either validate exact V1 or atomically create the
complete V1 schema and markers. Foreign, unmarked non-empty, newer, or
schema-drifted databases MUST be rejected. Two concurrent initializers MUST
converge on one exact schema through SQLite locking; M1-C MUST NOT add a second
cross-process lock protocol.

Every V1 table MUST be `STRICT` and `WITHOUT ROWID`. Package IDs MUST be BLOBs
of exactly 16 bytes; Revision and runtime blob digests MUST be BLOBs of exactly
32 bytes. Schema validation MUST verify the actual tables, columns, constraints,
primary keys, foreign keys, and table options rather than trusting version
markers alone. Persistence migration is independent from Revision Migration and
MUST preserve existing Pactrun identities.

**Verification: PR-TEST-0052, PR-TEST-0056.**

### PR-REQ-0232 - Exact persisted Revision content

M1-C MUST persist a Revision as its structured `PackageId` plus
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
blobs to be currently available; physical availability remains a separate M1-B
verification operation.

**Verification: PR-TEST-0053, PR-TEST-0054, PR-TEST-0055, PR-TEST-0057.**

### PR-REQ-0233 - Durable content before new database references

`StoredRuntimeBlob` MUST carry a private, process-local, non-serializable
store-instance witness issued only after M1-B durable publication succeeds. A
new Revision record MUST be authorized by exactly one current same-store witness
for every distinct blob digest required by its runtime-content closure. Missing,
extra, duplicate, or wrong-store witnesses MUST be rejected. Multiple
ContentIds that share one blob require one witness for that physical digest.
Witnesses authorize durable-content-before-reference ordering only and MUST NOT
be persisted or affect any Pactrun identity.

An exact Revision record that was already committed MAY be returned
idempotently without an old witness, including after a process crash and store
reopen. If blob publication succeeds but the database transaction does not,
the unreferenced immutable blob is a safe orphan. M1-C MUST never publish a
database reference before the required M1-B publication success.

**Verification: PR-TEST-0055, PR-TEST-0056.**

### PR-REQ-0234 - Immutable and recoverable Revision records

All M1-C schema and Revision writes, apart from SQLite's required out-of-
transaction WAL mode establishment, MUST use `BEGIN IMMEDIATE`. An exact retry
MUST be idempotent. An existing Revision identity with different canonical
component bytes or a different closure-derived reference index MUST be rejected
as corruption or collision and MUST NOT be overwritten or repaired.

A crash before COMMIT MUST leave no partial Revision record. A crash after
COMMIT but before the API response MAY be retried after restart and MUST recover
the exact committed record without requiring the expired process-local witness.
A successful SQLite COMMIT under WAL and `synchronous=FULL`, after successful
SQLite/VFS persistence operations, is the M1-C durable reference-publication
point. M1-C MUST NOT manually sync the database, WAL, or SHM around each
transaction, and its durability guarantee assumes the platform storage stack
honors successful persistence operations. Process-crash tests do not certify
arbitrary hardware power-loss behavior.

**Verification: PR-TEST-0052, PR-TEST-0054, PR-TEST-0056, PR-TEST-0057.**

## References and non-identity metadata

### PR-REQ-0019 - Human label ambiguity

Human labels and display versions MUST be associations rather than Revision
identity. Multiple labels MAY refer to one Revision, and one label MAY refer to
multiple exact Revisions. An ambiguous reference MUST fail resolution and MUST
NOT implicitly choose the latest or overwrite an older association.

**Verification: Pending automated coverage.**

### PR-REQ-0020 - Metadata observations

Reference labels, presentation, provenance, publisher attribution, and local
management metadata MUST remain separate from the immutable Revision body.
Conflicting observations MAY coexist and MUST NOT create a new Revision,
concatenate implicitly, or use last-write-wins semantics.

Presentation observations MUST refer to semantic keys already present in
`RevisionCore`; an observation for an unknown semantic key MUST be rejected.
The initial product scope MUST NOT silently select one observation as a trusted
preferred or canonical description.

**Verification: Pending automated coverage.**

### PR-REQ-0021 - Portable and local metadata

Revision export and import MUST preserve every portable non-identity
association or observation that the relevant Export Bundle Format defines as
part of the portable bundle. This MAY include human or reference labels,
portable presentation observations, and portable provenance or publisher
attribution claims. These fields MUST remain non-identity metadata.

Local install timestamps, source filesystem paths, local aliases, local notes,
and local trust decisions MUST be excluded by default. Publisher claims and
digests MUST NOT be presented as publisher authentication.

**Verification: Pending automated coverage.**

### PR-REQ-0022 - Repeat Revision import

Importing the same `PackageId` and digest MUST be identity-idempotent. Equivalent
visible metadata associations MUST NOT accumulate duplicates; new distinct
claims MAY be attached. Revision import MUST NOT be rejected solely because a
human-readable label becomes ambiguous. If the Revision or bundle is otherwise
valid, import MUST succeed, preserve both label associations, and MAY emit a
warning. Later human reference resolution MUST report ambiguity and MUST NOT use
last-write-wins or implicit latest selection.

**Verification: Pending automated coverage.**

## Instance identity and state publication

### PR-REQ-0023 - Stable Instance identity

`InstanceId` MUST be Pactrun-generated, opaque, globally strong, stable, and
non-reusable. Migration, Input mutation, and Snapshot Restore MUST NOT change
it. Recreating a deleted Instance name MUST produce a new `InstanceId`.

**Verification: Pending automated coverage.**

### PR-REQ-0024 - Instance names and provenance

`InstanceName` MUST be a human reference that is unique among live Instances in
one management environment. Names MAY be reused after deletion, but durable
provenance MUST use `InstanceId` as its authoritative historical referent.

**Verification: Pending automated coverage.**

### PR-REQ-0025 - Opaque InstanceStateVersion

Every Instance MUST have one opaque, durable, non-revivable
`InstanceStateVersion` representing a Pactrun-owned authoritative state
publication event. It MUST NOT be an external-service hash. A rollback or
recovery that recreates earlier field values MUST publish a new token to prevent
ABA behavior.

**Verification: Pending automated coverage.**

### PR-REQ-0026 - State-version publication

Any committed Instance-state change that can affect future compilation,
admission, execution-visible managed context, lifecycle, or recovery MUST
atomically publish one new `InstanceStateVersion`. This includes active Revision
changes, managed binding mutations, Restore commits, Migration edge commits,
ManualRecoveryRequired transitions, and `ResolveManualRecovery`.

**Verification: Pending automated coverage.**

### PR-REQ-0027 - Non-versioned observations

Hook-only external side effects, Snapshot object creation, Run and Artifact
creation, and presentation-only metadata MUST NOT independently change an
Instance's state version.

**Verification: Pending automated coverage.**

## Managed Input state

### PR-REQ-0028 - Stable Input identity and opaque bytes

Within one Package lineage, the same `InputIdentity` MUST always represent the
same long-lived semantic Input and MUST NOT be reused for another meaning. Input
payloads MUST be preserved byte-for-byte, including empty payloads; Pactrun MUST
NOT infer format or semantics from content or filenames.

**Verification: Pending automated coverage.**

### PR-REQ-0029 - One managed binding registry

An Instance MUST have at most one binding for each `InputIdentity`. `Active` and
`retained` MUST be roles derived relative to the active Revision, not separate
stores or historical value logs. A binding declared by the active Revision is
active; an existing undeclared binding is retained.

**Verification: Pending automated coverage.**

### PR-REQ-0030 - Retained binding lifetime

A retained binding MUST persist until it becomes active again, is explicitly
discarded by Migration, is explicitly deleted by the operator, or the Instance
is removed or abandoned. Historical Revision identity MAY be provenance but
MUST NOT be part of binding identity.

**Verification: Pending automated coverage.**

### PR-REQ-0031 - RequiredInputsSatisfied

`RequiredInputsSatisfied` MUST be derived from required declarations in the
active Revision and current managed bindings. Empty bindings count as present.
Missing required bindings MUST NOT invalidate the Instance or create a separate
lifecycle state.

**Verification: Pending automated coverage.**

### PR-REQ-0032 - Initial binding acquisition and Instance publication

Instance creation MAY commit an incomplete Instance. When a request explicitly
supplies an initial binding, it MUST identify the Input and acquisition source
explicitly and deterministically. The Instance and every explicitly supplied
binding MUST be one atomic management commit. Any acquisition or validation
failure for such a binding MUST fail the whole create request; Pactrun MUST NOT
silently omit it and create an Instance more incomplete than requested.

**Verification: Pending automated coverage.**

### PR-REQ-0033 - Binding mutation invariants

An active required binding MAY initially be absent, but once bound under that
active Revision it MUST NOT be ordinarily unset or deleted. Active optional
bindings MAY be set, replaced, or removed. Replacements MUST be atomic. Retained
bindings MAY be inspected, explicitly exported, or deleted, but MUST NOT be
directly set or replaced by the operator.

**Verification: Pending automated coverage.**

### PR-REQ-0034 - Secret protection

Secret protection MUST be sticky. Normal data MAY be promoted to Secret
automatically, but Secret data MUST NOT be implicitly downgraded. Declassification
requires an explicit Migration transition and explicit operator authorization.
Retained Secrets MUST retain protection, and ordinary inspection, diagnostics,
Run records, and user-visible metadata MUST NOT disclose their values or
value-derived digests.

**Verification: Pending automated coverage.**
