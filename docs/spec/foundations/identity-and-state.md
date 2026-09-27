---
title: Identity and State
---

# Identity and State

**Status: Normative architecture.**

<!-- spec-navigation:start -->
## Reading map (informative)

Look up exact identities, non-identity metadata, Instance state, Managed Inputs, and the separately closed ServiceStorage semantics. This is not a CLI tutorial.

Start with the [specification map](../index.md)
and [shared vocabulary](../glossary.md) if a term is unfamiliar.
Check [implementation status and remaining decisions](../../development/next-milestone.md)
before treating an approved contract as available runtime behavior.
The original status, rules, exceptions, and verification declarations below retain their meaning.
<!-- spec-navigation:end -->

This page owns internal identity, canonicalization, metadata, Instance state,
and managed binding invariants. User-visible behavior is summarized in the
[product behavior](../behavior/packages-revisions-and-instances.md).

## Revision identity

### PR-REQ-0011 - Stable Package lineage

`PackageId` MUST be opaque, stable, and independent of content. It MUST survive
Revision changes, presentation or publisher changes, export, and import. A fork
that becomes a new lineage MUST be explicitly re-identified.

For the closed M2 frontend, that declaration is the required exact
`package_id` in `PackSourceYamlV1`; installation MUST NOT derive a lineage from
source location or content. ID generation is a separate non-mutating operation.

**Verification: PR-TEST-0067, PR-TEST-0069, PR-TEST-0142, PR-TEST-0594, PR-TEST-0600.**

### PR-REQ-0012 - Operational content digest

`RevisionContentDigest` MUST identify Pactrun-managed operational semantics and
immutable owned runtime content. It MUST exclude `PackageId`, display content,
publisher claims, labels, source location, install time, local aliases, notes,
and local trust decisions. A future Revision Core format may make a
ServiceStorage-backed Managed Service Resource declaration identity-bearing,
but the service-owned live bytes MUST remain outside the Revision digest.

**Verification: PR-TEST-0044, PR-TEST-0067, PR-TEST-0331, PR-TEST-0346, PR-TEST-0493, PR-TEST-0594, PR-TEST-0600.**

### PR-REQ-0013 - Exact Revision identity

An exact `RevisionIdentity` MUST be the pair of `PackageId` and
`RevisionContentDigest`. Equal content digests under different Package IDs MUST
remain distinct Revision identities.

**Verification: PR-TEST-0053, PR-TEST-0054, PR-TEST-0594.**

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

`RevisionCoreFormatV1` is already Frozen and contains no `ServiceStorage` or
ServiceStorage-backed Managed Service Resource declaration. The accepted future
architecture does not retroactively add those concepts to its closed schema.
Candidate `PackSourceYamlV1` is an authoring projection into this unchanged
format, not another Revision identity or a raw-Core authoring route.

**Verification: PR-TEST-0040, PR-TEST-0041, PR-TEST-0069, PR-TEST-0331, PR-TEST-0493, PR-TEST-0600.**

### PR-REQ-0015 - Runtime content closure identity

Runtime content identity MUST encode logical runtime location or role, object
kind, immutable content identity, and runtime-semantic metadata. Hashing an
unordered set of blob digests is insufficient. Source-only paths, timestamps,
inodes, and ownership metadata MUST NOT affect identity unless Pactrun
explicitly defines them as runtime semantics.

**Verification: PR-TEST-0044, PR-TEST-0045, PR-TEST-0069, PR-TEST-0600.**

### PR-REQ-0016 - Semantic normalization

Pactrun MUST apply semantic defaults, validate, and normalize equivalent forms
before projecting and encoding `RevisionCore`. Unordered domain collections
MUST be sorted by a stable semantic key; ordered collections MUST preserve their
order; duplicate semantic keys MUST be rejected.

M2 MUST use the schema-directed scalar decoder in PR-REQ-0258 before this
normalization. A YAML library's implicit resolver or eager Boolean, null, or
numeric representation MUST NOT decide Revision meaning. Raw numeric source
tokens remain available until the target Frozen semantic type is known.

**Verification: PR-TEST-0041, PR-TEST-0045, PR-TEST-0068, PR-TEST-0333, PR-TEST-0337, PR-TEST-0494.**

### PR-REQ-0017 - RevisionCoreFormatV1 hash contract

`RevisionCoreFormatV1` MUST use a closed semantic schema, explicit domain
separation and framing, RFC 8785 JSON Canonicalization Scheme bytes, and a fixed
SHA-256 profile represented by a self-describing digest such as
`sha256:<value>`. JCS MUST NOT replace Pactrun semantic normalization.

The exact Frozen contract is defined by
[Revision Core Format V1](../contracts/revision-canonical.md).

**Verification: PR-TEST-0001, PR-TEST-0012, PR-TEST-0044, PR-TEST-0045.**

### PR-REQ-0018 - Stable published identity

Internal schema, type, serializer, library, and CLI changes MUST NOT alter the
identity of an already published RevisionCore format. Unknown or unsupported
format versions MUST be rejected instead of interpreted on a best-effort basis.

**Verification: PR-TEST-0044, PR-TEST-0331, PR-TEST-0338, PR-TEST-0339, PR-TEST-0493, PR-TEST-0494.**

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
vector MUST be rejected, but an exact `PactrunErrorRef` is required only when
the vector's spelling is registered under `revision_core` in the
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

M2 `InstallPackSource` MUST obtain the current same-store witness for every
distinct staged runtime blob before entering the installation transaction in
PR-REQ-0261. The additional metadata publication does not weaken or replace a
witness.

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

M2 extends that same transaction to source-projected portable metadata and any
explicit crate-private local metadata accepted for the installation. The exact
retry and no-partial-publication rules in PR-REQ-0261 preserve this boundary.

**Verification: PR-TEST-0052, PR-TEST-0054, PR-TEST-0056, PR-TEST-0057.**

## References and non-identity metadata

### PR-REQ-0019 - Human label ambiguity

Human labels and display versions MUST be associations rather than Revision
identity. Multiple labels MAY refer to one Revision, and one label MAY refer to
multiple exact Revisions. Authoritative label equality and lookup MUST preserve
and compare the exact valid Unicode scalar-value sequence without normalization,
case folding, trimming, or locale-sensitive comparison.

Resolution MUST first deduplicate bindings by exact `RevisionIdentity`. Zero
distinct targets is unresolved, one is resolved, and more than one is
ambiguous. Ambiguity MUST fail with `resolution.ambiguous_reference` and MUST
NOT implicitly choose the latest, the most recently installed target, the
binding with the most source claims, or an older or newer association.
Deterministic results MUST use the typed ordering defined by PR-REQ-0250 rather
than insertion or query-plan order.

**Verification: PR-TEST-0532.**

### PR-REQ-0020 - Metadata observations

Reference labels, presentation, provenance, publisher attribution, and local
management metadata MUST remain separate from the immutable Revision body.
Presentation MUST be keyed current metadata as defined by PR-REQ-0251. A
different value for the same typed target and presentation field replaces the
current value only through the explicit typed mutation contract; M1-D MUST NOT
retain conflicting historical presentation values or use an implicit
last-write-wins write.

Provenance and publisher attribution MUST be value-keyed typed claims as
defined by PR-REQ-0252. Identical semantic tuples are one idempotent claim;
different tuples MAY coexist and MUST NOT create a new Revision, concatenate
implicitly, or cause Pactrun to select a trusted preferred claim.

Presentation targets MUST resolve to the closed typed set already established
by the installed `RevisionCoreV1`; an unknown or inapplicable target MUST be
rejected. Presentation MUST NOT create an Action, Input, Parameter, output, or
lifecycle capability that is absent from the Revision.

This metadata category is descriptive and non-operational. It MUST NOT be used
to persist a ServiceStorage declaration, a ServiceStorage-backed Managed Service
Resource identity or declaration, live-resource presence, continuity,
retention, authority, an operation prerequisite, or a storage-lifetime
obligation.

**Verification: PR-TEST-0062, PR-TEST-0063.**

### PR-REQ-0021 - Portable and local metadata

Revision export and import MUST preserve every portable non-identity
association or observation that the relevant Export Bundle Format defines as
part of the portable bundle. Reference-label bindings, presentation metadata,
source-URI claims, publisher-attribution claims, and attribution claims are
semantically portable-capable because their Domain meaning does not depend on
one Pactrun installation. These fields MUST remain non-identity metadata.

Local install timestamps, source filesystem paths, local aliases, local notes,
and local trust decisions are local-only and MUST be excluded by default.
Portable-capable does not require a future Export Bundle Format to carry a
metadata kind and does not define serialization, carriage, import conflict, or
merge policy. Publisher claims and digests MUST NOT be presented as publisher
authentication.

**Verification: PR-TEST-0539.**

### PR-REQ-0022 - Repeat Revision import

Importing the same `PackageId` and digest MUST be identity-idempotent. Equivalent
visible metadata associations MUST NOT accumulate duplicates. Equivalence MUST
use the complete typed semantic tuple for the metadata kind, including every
typed source component; it MUST NOT use a storage row identifier, timestamp, or
serialization artifact. New distinct set-like claims MAY be attached, while
current metadata may change only through its explicit replacement semantics.

Revision import MUST NOT be rejected solely because a human-readable label
becomes ambiguous. If the Revision or bundle is otherwise valid, import MUST
preserve both label associations and MAY emit a warning. Later human reference
resolution MUST report ambiguity and MUST NOT use last-write-wins or implicit
latest selection. The Export Bundle Format and its import application policy
remain separate design work.

**Verification: PR-TEST-0538, PR-TEST-0539, PR-TEST-0540.**

## Instance identity and state publication

### PR-REQ-0023 - Stable Instance identity

`InstanceId` MUST be Pactrun-generated, opaque, globally strong, stable, and
non-reusable. Migration, Input mutation, and Snapshot Restore MUST NOT change
it. Recreating a deleted Instance name MUST produce a new `InstanceId`.

**Verification: PR-TEST-0258, PR-TEST-0436, PR-TEST-0591, PR-TEST-0604.**

### PR-REQ-0024 - Instance names and provenance

`InstanceName` MUST be a human reference that is unique among live Instances in
one management environment. Names MAY be reused after deletion, but durable
provenance MUST use `InstanceId` as its authoritative historical referent.
M2 exact syntax and ordering are closed by PR-REQ-0263.

**Verification: PR-TEST-0531.**

### PR-REQ-0025 - Opaque InstanceStateVersion

Every Instance MUST have one opaque, durable, non-revivable
`InstanceStateVersion` representing a Pactrun-owned authoritative state
publication event. It MUST NOT be an external-service hash. A rollback or
recovery that recreates earlier field values MUST publish a new token to prevent
ABA behavior. Service-owned live bytes are not part of this token.

**Verification: PR-TEST-0259, PR-TEST-0301, PR-TEST-0330, PR-TEST-0346, PR-TEST-0591.**

### PR-REQ-0026 - State-version publication

Any committed Pactrun-authoritative Instance-state change that can affect future
compilation, admission, execution-visible managed context, lifecycle, or
recovery MUST atomically publish one new `InstanceStateVersion`. This includes
active Revision changes, managed binding mutations, Restore commits, Migration
edge commits, ManualRecoveryRequired transitions, and
`ResolveManualRecovery`. It does not make service-owned live bytes part of the
same publication.

**Verification: PR-TEST-0263, PR-TEST-0301, PR-TEST-0330, PR-TEST-0377, PR-TEST-0591.**

### PR-REQ-0027 - Non-versioned observations

Hook-only external side effects, service-created or service-modified Managed
Service Resource bytes within ServiceStorage, Snapshot object creation, Run and
Artifact creation, and presentation-only metadata MUST NOT independently change
an Instance's state version. Pactrun MUST NOT claim that
`InstanceStateVersion` linearizes mutations that Pactrun does not
authoritatively own or observe.

**Verification: PR-TEST-0246, PR-TEST-0346, PR-TEST-0558, PR-TEST-0560, PR-TEST-0602.**

## Managed Input state

### PR-REQ-0028 - Stable Input identity and opaque bytes

Within one Package lineage, the same `InputIdentity` MUST always represent the
same long-lived semantic Input and MUST NOT be reused for another meaning. Input
payloads MUST be preserved byte-for-byte, including empty payloads; Pactrun MUST
NOT infer format or semantics from content or filenames.

**Verification: PR-TEST-0075, PR-TEST-0147, PR-TEST-0590.**

The [pre-E review](../../development/pre-e-readiness.md) records the
architecture, author-responsibility and evidence-scope review for this rule.
Automated examples do not prove subjective quality or arbitrary author intent.

### PR-REQ-0029 - One managed binding registry

An Instance MUST have at most one binding for each `InputIdentity`. `Active` and
`retained` MUST be roles derived relative to the active Revision, not separate
stores or historical value logs. A binding declared by the active Revision is
active; an existing undeclared binding is retained.

**Verification: PR-TEST-0280, PR-TEST-0285, PR-TEST-0591.**

### PR-REQ-0030 - Retained binding lifetime

A retained binding MUST persist until it becomes active again, is explicitly
discarded by Migration, is explicitly deleted by the operator, or the Instance
is removed or abandoned. Historical Revision identity MAY be provenance but
MUST NOT be part of binding identity.

**Verification: PR-TEST-0610, PR-TEST-0612, PR-TEST-0614.**

### PR-REQ-0031 - RequiredInputsSatisfied

`RequiredInputsSatisfied` MUST be derived from required declarations in the
active Revision and current managed bindings. Empty bindings count as present.
Missing required bindings MUST NOT invalidate the Instance or create a separate
lifecycle state.

**Verification: PR-TEST-0225, PR-TEST-0321, PR-TEST-0525, PR-TEST-0590, PR-TEST-0599.**

### PR-REQ-0032 - Initial binding acquisition and Instance publication

Instance creation MAY commit an incomplete Instance. When a request explicitly
supplies an initial binding, it MUST identify the Input and acquisition source
explicitly and deterministically. The Instance and every explicitly supplied
binding MUST be one atomic management commit. Any acquisition or validation
failure for such a binding MUST fail the whole create request; Pactrun MUST NOT
silently omit it and create an Instance more incomplete than requested.

For M2, acquisition and bounded file-backed staging finish before the database
transaction, and the exact atomic publication and cleanup contract is
PR-REQ-0265.

**Verification: PR-TEST-0147, PR-TEST-0590, PR-TEST-0599.**

### PR-REQ-0033 - Binding mutation invariants

An active required binding MAY initially be absent, but once bound under that
active Revision it MUST NOT be ordinarily unset or deleted. Active optional
bindings MAY be set, replaced, or removed. Replacements MUST be atomic. Retained
bindings MAY be inspected, explicitly exported, or deleted, but MUST NOT be
directly set or replaced by the operator.

**Verification: PR-TEST-0076.**

### PR-REQ-0034 - Secret protection

Secret protection MUST be sticky. The immutable payload's persisted
`Normal | Secret` protection is the stored protection floor for a binding. For
an active binding, effective protection is the stronger of that stored floor
and the active declaration. For a retained binding, which has no active
declaration, effective protection is the stored floor. An active Secret
declaration therefore requires a committed Secret payload, while an active
Normal declaration with a Secret payload is valid and remains effectively
Secret.

Normal data MAY be promoted to Secret automatically, but Secret data MUST NOT
be implicitly downgraded. Declassification requires an explicit Migration
transition and explicit operator authorization. Retained Secrets MUST retain
protection, and ordinary inspection, diagnostics, Run records, and user-visible
metadata MUST NOT disclose their values or value-derived digests.

Only a deletion already permitted by PR-REQ-0033 ends the old binding's sticky
continuity. An active required binding remains non-deletable once bound. A
later bind after an allowed deletion is a newly acquired binding whose initial
protection follows the then-active declaration; it MUST NOT be described as
declassifying the old Secret payload and does not add a secure-erasure claim.

M2 publication, persistence validation, storage, staging, and disclosure apply
this rule through PR-REQ-0264, PR-REQ-0266, PR-REQ-0268, and PR-REQ-0269.
Secret classification MUST NOT be confused with cryptographic storage.

**Verification: PR-TEST-0076.**

## ServiceStorage-backed semantic closure

This section defines representation-independent architecture invariants for
ServiceStorage-backed Managed Service Resources. It does not decide whether
Docker volumes, external databases, remote objects, or other service-owned
resources use this abstraction. That broader resource taxonomy remains future
design work. This section also does not define a Revision Core schema, Hook
Protocol wire shape, authoring syntax, persistence model, CLI, or production
implementation.

### PR-REQ-0235 - Persistent Instance-data ownership

Pactrun MUST distinguish Pactrun-authoritative Managed Input Bindings from
service-authoritative live state in Pactrun-provided ServiceStorage. A Managed
Input Binding is an Instance-scoped, persistent, detached opaque value whose
authoritative copy, presence, replacement, retention, and Secret protection are
managed by Pactrun. A ServiceStorage-backed Managed Service Resource is attached
live state whose authoritative bytes or contents are owned and used by the
service. Pactrun ownership of the provided storage lifetime MUST NOT be
misrepresented as ownership of those contents.

A persistent file MUST NOT be classified as an Input merely because Pactrun or
a user needs to see or modify it. Pactrun MUST NOT maintain an Input and a
service-owned file as implicit bidirectionally synchronized authoritative
copies. Explicit future ownership transfer into a Managed Input may be designed
separately, but is not the default solution for service-owned live state.

**Verification: PR-TEST-0346, PR-TEST-0370.**

Isolated live bytes are not mirrored into Inputs or Snapshots, including real
V2 Action writes. Cross-Revision continuity and target publication have separate
evidence under PR-REQ-0237 and PR-REQ-0326.

### PR-REQ-0236 - Managed Service Resource declaration and existence

`ServiceStorage` MUST mean Pactrun-provided, Instance-scoped persistent storage
made available to service or Hook behavior across Runs. A ServiceStorage-backed
Managed Service Resource MUST have a stable semantic identity declared by a
Revision and a contractual association with a ServiceStorage semantic identity.
It MAY also have a locator, user exposure, access, and operation prerequisites.
A file-shaped ServiceStorage-backed resource MAY be called a Managed Service
File.

Pactrun may manage those contracts and cross-Revision semantic continuity, but
MUST NOT thereby claim ownership, content addressing, versioning, automatic
Snapshot inclusion, or mutation linearization of the service-owned live bytes.
The declaration exists independently from the live object: a Revision MAY
declare a resource that is currently absent and that the service creates later.

The identity encoding, association schema, access encoding, prerequisite
encoding, authoring form, wire authority, and durable representation remain
independent version boundaries implemented by Core/Hook V2 and V7. This
requirement does not classify non-ServiceStorage-backed service-owned resources.

**Verification: PR-TEST-0346.**

This declaration/existence test covers eager isolated storage and independently
absent file/directory objects. Cross-Run access and continuity are separately
covered under PR-REQ-0244 and PR-REQ-0237.

### PR-REQ-0241 - ServiceStorage and resource semantic identity

A ServiceStorage declaration and each ServiceStorage-backed Managed Service
Resource declaration MUST have separate stable semantic identities within one
Package lineage. An identity MUST retain one long-lived semantic meaning across
Revisions and MUST NOT be reused for another storage or resource meaning. The
same declaration identity in different Instances denotes the same contract role,
not a shared physical storage or live object, and identities in different
Packages MUST NOT be treated as implicitly equivalent.

A Managed Service File is a file-shaped subtype of a ServiceStorage-backed
Managed Service Resource, not a third persistent ownership model. A resource's
storage-relative locator identifies its contractual logical location within the
associated storage; it MUST NOT become resource identity, a host-path contract,
an existence assertion, or evidence of representation compatibility.

**Verification: PR-TEST-0346, PR-TEST-0371.**

These tests enforce separate Instance allocations, explicit identity/locator
mapping and rejection of cross-Package continuity. Preserving an identity's
long-lived semantic meaning remains a Package-author obligation; Pactrun does
not infer that meaning from service contents.

### PR-REQ-0242 - Declaration, existence, and observation

A Revision declaration and the corresponding Instance live resource existence
MUST remain separate facts. Live existence has the point-in-time semantic states
`Present`, `Absent`, and `Unknown`. `Present` means only that the resource was
observed to exist at the relevant observation boundary; it MUST NOT assert that
the contents are valid, coherent, unchanged afterward, or compatible with a
Revision. `Unknown` MUST NOT be interpreted as either presence or absence.

Pactrun MAY observe existence only through a capability that can make the
relevant point-in-time observation. It MUST NOT claim continuous observation,
advance `InstanceStateVersion` merely because observed existence or live bytes
change, or apply Managed Input presence and mutation linearization to the live
resource.

**Verification: PR-TEST-0346, PR-TEST-0354, PR-TEST-0357, PR-TEST-0387, PR-TEST-0388.**

Partial coverage: external live-byte changes do not advance InstanceStateVersion.
PR-TEST-0354 additionally covers point-in-time absence and actual leaf kinds in
the internal filesystem primitive. PR-TEST-0357 covers CLI observation and
missing protected-root Unknown without state mutation. PR-TEST-0387 exercises
native Linux permission denial; PR-TEST-0388 separates runtime predicates from
read-only planning. These point-in-time observations do not exclude subsequent
external-service races.

### PR-REQ-0248 - M1-D metadata boundary

M1-D non-identity metadata persistence MUST be limited to the closed typed
descriptive values in PR-REQ-0249 through PR-REQ-0253: reference-label
bindings, current presentation, typed provenance claims, local aliases, one
local current note, and one local current trust assessment. It MUST NOT accept
generic metadata, arbitrary keys, JSON/EAV payloads, or opaque serialized
objects, and MUST NOT become a backdoor persistence mechanism for
ServiceStorage or ServiceStorage-backed Managed Service Resource operational
semantics.

M1-D MUST NOT persist storage or resource declarations or identities, live
presence, service-owned contents, locators or associations, continuity,
compatibility, retention or discard, persistent Hook authority, operation
prerequisites, Cleanup-completion or storage-finalization obligations,
AbandonManagement non-destruction obligations, or the broader service-owned
resource taxonomy. PersistenceSchemaV2 and its repository contract MUST enforce
this boundary rather than merely hiding excluded operational data behind a
metadata name.

**Verification: PR-TEST-0059, PR-TEST-0067.**

### PR-REQ-0272 - M2 scope and anti-backdoor boundary

M2 MUST implement only minimal Pack authoring and Revision installation,
Instance creation and inspection, Pactrun-authoritative Managed Input bindings,
Secret disclosure controls, and their internal persistence and concurrency
boundaries. It MUST NOT reinterpret a Managed Input, M1-D metadata value,
transient staging file, payload chunk, Instance state token, or database row as
ServiceStorage or a ServiceStorage-backed Managed Service Resource.

M2 MUST NOT add Action execution, Hook launch, durable execution pins, Run
ownership, Snapshot runtime, Migration execution, recovery state, Instance
deletion workflow, advanced Recipe authoring, stable machine APIs, new Frozen
error codes, `LocalInstall` history, or cryptographic Secret storage. The
ability to author all `RevisionCoreV1` capability declarations establishes
installation projection only and MUST NOT be presented as runtime support for
those later milestones.

Candidate `PackSourceYamlV1` and the implemented internal
`PersistenceSchemaV3`, together with its `PersistenceSchemaV4` extension, are
independently versioned internal contracts. They MUST
NOT modify or acquire the compatibility
status of Frozen Revision Core, Snapshot Integrity, Hook Protocol, or Error
Taxonomy V1. A future ServiceStorage design MUST enter through its own approved
Revision, authority, persistence, and runtime gates rather than an M2
authoring, binding, or metadata extension.

**Verification: PR-TEST-0079.**

## Pre-M1-D non-identity metadata closure

### PR-REQ-0249 - Typed metadata scope and authoritative strings

M1-D metadata MUST be Revision-scoped, descriptive, and non-identity-bearing.
Its closed types are `ReferenceLabelBinding`, current presentation metadata,
the three provenance claim types in PR-REQ-0252, local Revision aliases, one
optional local current note, and one optional local current trust assessment.
M1-D MUST NOT attach metadata to a Package, Instance, generic entity path, or an
as-yet-undefined local installation record.

Except where a closed type says otherwise, a textual metadata value MUST be a
non-empty valid Unicode scalar-value sequence. Pactrun MUST preserve its exact
UTF-8 byte sequence and MUST NOT normalize Unicode, fold case, trim, repair, or
apply locale-sensitive comparison. Equality and authoritative ordering MUST use
the typed exact value. Exact textual ordering is unsigned lexicographic ordering
of the preserved UTF-8 bytes. M1-D sets no semantic length cap; a future
transport limit MUST reject an oversized request rather than truncate or change
the value.

`SourceUri` MUST match the ASCII `URI` production in RFC 3986. It MUST contain a
scheme, MUST reject a relative reference and non-ASCII input, and MAY contain a
fragment. URI validity does not authorize case normalization, percent-encoding
normalization, base resolution, parser-result reserialization, parser semantic
equality, or any other canonicalization; its authoritative equality and
ordering remain those of the caller-provided exact URI string. A future
normalized or fuzzy search facility MAY provide non-authoritative discovery but
MUST NOT change binding identity, equality, conflict, or ambiguity.

Every optional typed component MUST order `Absent` before `Present(value)`;
present values use their underlying typed comparator. Every closed union or enum
used for deterministic ordering MUST publish an explicit stable rank and MUST
NOT depend on a Rust discriminant, SQL rowid, insertion time, query-plan order,
SQLite NULL ordering, or locale collation.

**Verification: PR-TEST-0058, PR-TEST-0066.**

### PR-REQ-0250 - Reference-label binding, lookup, and ordering

A `ReferenceLabelBinding` MUST be the complete tuple of a non-empty exact
`ReferenceLabel`, one exact `RevisionIdentity`, and one typed source. The source
union and stable rank are `Unattributed < SourceUri < Publisher <
PublisherSourceUri`. A publisher contains a non-empty exact publisher name and
an optional non-empty exact namespace; source-bearing variants contain an exact
`SourceUri`. The complete tuple determines equality. Adding an identical tuple
or removing an absent identical tuple MUST be idempotent; removing one tuple
MUST NOT remove another source or target binding. A label has no independent
managed object or lifetime apart from its bindings.

Authoritative lookup by label MUST collect distinct exact Revision targets.
Multiple source tuples for the same target MUST NOT make that target ambiguous;
multiple distinct Revision targets MUST. Lookup results and enumeration MUST
order by label UTF-8 bytes, `PackageId` canonical bytes,
`RevisionContentDigest` canonical bytes, source-variant rank, and then the
source fields in their declared tuple order. Optional source fields use the
ordering in PR-REQ-0249.

**Verification: PR-TEST-0058, PR-TEST-0062, PR-TEST-0066.**

### PR-REQ-0251 - Closed current presentation model

Current presentation metadata MUST be keyed by one exact Revision, one
`PresentationTargetV1`, and one `PresentationField`. The target union and stable
rank are, in order: Revision root, Input, Action, Action Parameter, Managed
Output, Snapshot Capture, Snapshot Capture Parameter, Snapshot Restore,
Snapshot Restore Parameter, Migration edge, and Cleanup. Nested targets MUST
use their typed parent and child semantic identities. A Migration edge MUST use
the exact source Revision selector within the target Revision context.

The presentation fields and stable rank are `display_name < summary <
description < help`. Every one of the four fields is valid for every target in
the closed target union. Values MUST be non-empty exact strings under
PR-REQ-0249; clearing a field represents absence. No Hook internal, Migration
transition row, runtime `ContentId`, JSON path, authoring-AST path, arbitrary
field, or locale is a presentation target or key in M1-D.

A target MUST exist in the strict-decoded installed `RevisionCoreV1`. Unknown,
malformed, or absent targets MUST be rejected. Reapplying the current value is
idempotent. A different value or clear operation MUST use the semantic CAS rule
in PR-REQ-0255 and MUST NOT retain an older presentation value. Deterministic
enumeration MUST order by Revision identity, target rank, target components in
their declared order, and presentation-field rank.

**Verification: PR-TEST-0058, PR-TEST-0063, PR-TEST-0066.**

### PR-REQ-0252 - Typed provenance claims

M1-D provenance MUST contain exactly these value-keyed claim types and stable
rank: `SourceUriClaim < PublisherAttributionClaim < AttributionClaim`.

- `SourceUriClaim` contains one exact `SourceUri`.
- `PublisherAttributionClaim` contains a non-empty exact publisher name, an
  optional non-empty exact namespace, and an optional exact source URI.
- `AttributionClaim` contains non-empty exact attribution text and an optional
  exact source URI.

The complete typed tuple determines equality and idempotency. Identical tuples
MUST NOT accumulate; different tuples MAY coexist. Deterministic enumeration
MUST order by Revision identity, claim rank, and the fields above in declaration
order, with optional fields ordered by PR-REQ-0249. M1-D MUST NOT add an opaque
claim ID, generic claim kind, arbitrary key/value payload, automatic timestamp,
mutation order, preferred claim, authenticity conclusion, or audit history.

**Verification: PR-TEST-0058, PR-TEST-0062, PR-TEST-0066.**

### PR-REQ-0253 - Local aliases, notes, and trust

A `LocalAlias` MUST be a non-empty exact string in a Revision-only namespace
within one management environment. One alias may target at most one exact
Revision, and one Revision may have multiple aliases. Setting the same target
is idempotent; a different target is a conflict unless an explicit semantic CAS
authorizes the replacement. Alias equality and conflict use the exact typed
value, not normalized or fuzzy search output.

Each exact Revision MAY have one non-empty local current note and one local
current trust assessment. Note absence is distinct from an empty string. Trust
is exactly `Trusted | Distrusted`, with absence meaning `NoDecision`.
`Distrusted` is descriptive local assessment only and MUST NOT independently
deny installation or execution. Note and trust replacement and clearing use
PR-REQ-0255. M1-D MUST NOT create alias, note, or trust history, opaque IDs,
purpose-scoped trust, policy enforcement, or a metadata version token.

Local install time and source filesystem path are local-only but belong to a
future typed installation or source-observation model. M1-D MUST NOT persist
them as current Revision metadata before `LocalInstall` identity, cardinality,
and lifecycle are defined.

**Verification: PR-TEST-0058, PR-TEST-0064, PR-TEST-0066, PR-TEST-0530, PR-TEST-0535.**
