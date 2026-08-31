---
title: Resources and Versioning
---

# Resources and Versioning

**Status: Normative architecture.**

## Resource lifecycle

### PR-REQ-0072 - Snapshot lifetime

A committed Snapshot MUST be a durable first-class object with no default
expiry in the initial product scope. Instance and Revision deletion MUST NOT
cascade-delete Snapshots, and Snapshot content MUST NOT depend on the creating
Run or Run Artifact remaining available.

**Verification: Pending automated coverage.**

### PR-REQ-0073 - Run, Artifact, and workspace lifetime

A Run Artifact MUST belong to its Run and MAY expire independently of retained
Run metadata. A Run Record MAY have a separate retention policy. Workspaces and
uncommitted Snapshot Candidates MUST be execution-scoped and cleaned after
terminal execution.

**Verification: Pending automated coverage.**

### PR-REQ-0074 - Checkpoint and pin lifetime

Transition checkpoints and durable recovery state MUST remain until no recovery
or reference obligation exists. Execution-only pins MAY be released only after
the Run is terminal and no recovery state needs them.

**Verification: Pending automated coverage.**

### PR-REQ-0075 - Revision deletion guards

Pactrun MUST reject deletion of a Revision referenced by an active Instance or
durably pinned by an accepted Run. A compile-only Plan MUST NOT prevent
deletion. Historical Run identity and Snapshot provenance MUST NOT permanently
prevent Revision deletion.

**Verification: Pending automated coverage.**

### PR-REQ-0076 - Physical content reachability

Physical GC MUST NOT remove content reachable from any managed object,
Snapshot, accepted-Run pin, checkpoint, or recovery state. GC MUST use strong
references or reachability rather than interpreting operation names or service
semantics. This rule governs Pactrun-owned content stores; it does not authorize
Pactrun to delete service-owned live resources. In particular, abandonment does
not make ServiceStorage-backed state collectible as unreferenced storage. The
non-destruction policy and its future durable representation are separate design
concerns defined by PR-REQ-0247.

**Verification: Pending automated coverage.**

## Independent version domains

### PR-REQ-0077 - Separate version domains

Internal persistence schema, Revision Core format and hash domain, Snapshot
integrity format and hash domain, export bundle format, Hook Protocol, Recipe
Authoring API, and structured CLI output MUST be independently versioned. They
MUST NOT share one generalized Pactrun schema version.

**Verification: Pending automated coverage.**

### PR-REQ-0078 - Persistence migrations

Internal persistence migration MUST preserve Pactrun domain identities and
MUST preserve or transform every non-terminal Run and unresolved recovery
obligation. It MUST NOT be confused with Revision Migration.

The implemented V1-to-V2 migration defined by PR-REQ-0257 MUST preserve every
existing Package and Revision identity, both exact canonical Revision content
components, and the complete derived runtime-content reference relation. A
crash before the migration commit MUST leave an exact admissible V1 database;
a crash after commit MUST expose an exact admissible V2 database. An
intermediate version marker or partial V2 schema MUST never be accepted.

**Verification: Pending automated coverage.**

### PR-REQ-0079 - Revision Core format ownership

Each published Revision Core format MUST define its closed semantic schema,
identity-affecting normalization, collection ordering, runtime-content
descriptor, canonical byte profile, hash framing, domain separation, and hash
algorithm.

The Frozen `RevisionCoreFormatV1` contract is defined by
[Revision Core Format V1](../package-contracts/revision-core-format-v1.md).

**Verification: Pending automated coverage.**

### PR-REQ-0080 - Snapshot integrity format ownership

Each Snapshot integrity format MUST define its integrity-bearing fields,
SnapshotId binding, producer and provenance normalization, complete managed
binding representation, service-content roles, collection ordering, canonical
bytes, domain separation, and hash profile. It MUST hash a semantic manifest,
not archive bytes. `SnapshotIntegrityFormatV1` MUST apply semantic normalization
before RFC 8785 JCS encoding and use a fixed SHA-256 profile. The exact Frozen
contract is defined by
[Snapshot Integrity Format V1](../package-contracts/snapshot-integrity-format-v1.md).

**Verification: PR-TEST-0013, PR-TEST-0015, PR-TEST-0016, PR-TEST-0018,
PR-TEST-0019.**

### PR-REQ-0081 - Bundle envelopes

Revision and Snapshot bundles MUST be self-describing, versioned envelopes that
identify their kind, format version, manifest, and content. Unsupported formats
MUST be rejected rather than guessed. Packaging or compression changes MUST NOT
change contained domain identity.

**Verification: Pending automated coverage.**

### PR-REQ-0082 - Hook Protocol version

The canonical Hook Protocol MUST have its own language-neutral version covering
Session establishment, authority, I/O transitions, typed contexts and outputs,
recovery-risk messages, cancellation, EOF, and protocol violations. SDKs and
helpers MUST remain adapters to that protocol.

**Verification: Pending automated coverage.**

### PR-REQ-0240 - Future ServiceStorage version gates

The accepted `ServiceStorage` and ServiceStorage-backed Managed Service Resource
architecture MUST NOT be added to the closed `RevisionCoreFormatV1` schema or
the Frozen `HookProtocolV1` authority union. A formal identity-bearing storage
or resource declaration requires a future Revision Core format. A formal
persistent service-storage Session authority, if required, requires a future
Hook Protocol version.

Revision Core and Hook Protocol remain independent version domains. A future
Revision Core format MUST NOT imply that every Hook uses the same-numbered or a
new Hook Protocol version. PR-REQ-0235 through PR-REQ-0248 close the
representation-independent ServiceStorage-backed semantics. The declaration,
authority, compatibility, access, prerequisite, continuity, retention, discard,
migration-coordination, Cleanup coordination, non-destruction, and durable-
representation encodings and mechanisms remain formal design gates rather than
properties of V1. Whether non-ServiceStorage-backed service-owned resources use
the same abstraction remains a separate taxonomy gate.

**Verification: Pending automated coverage.**

### PR-REQ-0083 - Structured CLI version

Human-readable output MAY evolve for usability. Machine-readable CLI output
MUST be a versioned interface, and breaking changes MUST require an explicit
version change. Interactive Hook terminal streams MUST NOT be wrapped in the
structured output envelope.

**Verification: Pending automated coverage.**

## Import identity

### PR-REQ-0084 - Revision import identity

Revision export and import MUST preserve `PackageId`,
`RevisionContentDigest`, `RevisionCore`, and owned runtime content. The same
Package ID and digest MUST be idempotent; a different digest under the same
Package ID MUST form another Revision; the same digest under another Package ID
MUST remain a distinct Revision identity. Export and import MUST also preserve
the bundle-defined portable non-identity metadata required by
[PR-REQ-0021](./identity-and-state.md#pr-req-0021---portable-and-local-metadata).

Pre-M1-D classifies portable-capable metadata but does not define an Export
Bundle Format, require any metadata kind to be carried, or define application,
conflict, replacement, or merge semantics for imported metadata. Those choices
remain owned by the future bundle format and import design.

**Verification: Pending automated coverage.**

### PR-REQ-0254 - Non-identity metadata portability boundary

Current presentation metadata, `ReferenceLabelBinding`, `SourceUriClaim`,
`PublisherAttributionClaim`, and `AttributionClaim` MUST be semantically
portable-capable: their Domain meaning is valid across Pactrun installations
and MUST NOT contain a local database identity, install event, row identity,
host path, or local trust conclusion.

Local aliases, local current notes, local current trust assessments, local
install timestamps, and source filesystem paths MUST be local-only.
Portable-capable MUST NOT be equated with identity-bearing or automatically
exported. Local persistence MUST NOT be treated as evidence that a kind is
local-only. Export serialization, selection, carriage, import conflict, and
merge policy remain future Export Bundle Format work and MUST NOT alter
Revision identity.

**Verification: PR-TEST-0058.**

### PR-REQ-0085 - Snapshot import identity

Snapshot export and import MUST preserve `SnapshotId`, integrity format and
digest, producer Revision identity, immutable provenance, complete managed
binding state, and service recovery content. The same Snapshot ID and digest
MUST be idempotent; the same ID with a different digest MUST be rejected as an
identity collision.

**Verification: Pending automated coverage.**
