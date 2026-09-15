---
title: Resources and Versioning
---

# Resources and Versioning

**Status: Normative architecture.**

<!-- spec-navigation:start -->
## Reading map (informative)

Use this contract for resource lifetimes, retention and pins, local trust, and independent format/runtime version boundaries.

Start with the [specification map](../index.md)
and [shared vocabulary](../glossary.md) if a term is unfamiliar.
Check [implementation status and remaining decisions](../../development/next-milestone.md)
before treating an approved contract as available runtime behavior.
The original status, rules, exceptions, and verification declarations below retain their meaning.
<!-- spec-navigation:end -->

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
uncommitted Snapshot Candidates MUST be execution-scoped; Snapshot Candidate
execution-scoped lifetime and post-termination cleanup remain unchanged.

Action Workspaces MUST remain execution-scoped. For an Action, execution
termination means that the supervised Hook execution has ended and required
process-tree termination has been observed. A path on which no Hook process was
started does not require a Hook-termination event.

Execution termination is distinct from durable Run terminal publication: the
transaction that publishes the Finished Run outcome and its associated
authoritative records. “After terminal execution” MUST NOT be interpreted as
requiring Workspace cleanup to wait until that transaction has committed.

Once execution has terminated and output preparation no longer needs the
execution Workspace, Pactrun MUST attempt Workspace cleanup. The attempt MAY
precede durable Run terminal publication. Successful cleanup MUST NOT be a
prerequisite for terminal publication.

Cleanup failure MAY leave non-authoritative residue. It MUST NOT change the Run
outcome, create or reopen recovery risk, create a manual-recovery obligation,
or prevent terminal publication. Residue remains eligible for subsequent
owner-session teardown or confirmed-owner-loss housekeeping; it MUST NOT become
managed content merely because it remains on disk.

After confirmed owner loss, orphaned Action Workspace bytes are cleanup-eligible
under the selected ownership mechanism. Their cleanup MUST NOT recover or
publish uncommitted output slots.

**Verification: PR-TEST-0106, PR-TEST-0110.**

### PR-REQ-0282 - Execution-workspace housekeeping failures

Cleanup is non-authoritative housekeeping and MUST NOT determine Action
success. A cleanup failure MUST NOT change an already-selected outcome,
replace a primary failure, open recovery risk, create a manual-recovery
obligation, or prevent terminal publication. An error observed before
terminalization MUST be recorded as a secondary failure under
[PR-REQ-0051](../execution/execution-and-concurrency.md#pr-req-0051---failure-detail-ordering).

This cleanup MUST be limited to the current Run's execution tree and MUST
protect other Runs, the owner lease, and prepared output staging. It MUST NOT
create a durable cleanup queue or extend execution pins. Later housekeeping
MUST NOT rewrite an existing terminal outcome.

**Verification: PR-TEST-0110.**

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

PersistenceSchemaV3 represents the active-Instance guard as an exact
`ON DELETE RESTRICT` foreign key. This relational guard does not define the
future M7 Instance deletion workflow.

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

`PackSourceYamlV1` and `PersistenceSchemaV4` are two additional independent
internal version domains. Their `V1` and `V4` labels do not couple them to
Revision Core, Hook Protocol, Snapshot Integrity, or a future CLI format.

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

The implemented V1/V2-to-V3 extension in PR-REQ-0270 additionally preserves all
M1-D metadata and adds no inferred Instance, binding, payload, source, or
installation-history rows.

The implemented V1/V2/V3-to-V4 extension in PR-REQ-0276 additionally preserves
every Instance, binding, payload header, and payload chunk and adds no inferred
Run, invocation, execution owner, pin, outcome, failure, Hook completion,
Artifact, or recovery-guard rows. V4 is the first schema that represents Runs;
a later migration MUST preserve or transform every `run_executions` row as a
non-terminal Run and every `instance_recovery_guards` row as an unresolved
recovery obligation rather than inferring their disposition. V4 is the
integrated M3 baseline. M4's [V5 implementation](../persistence/persistence-schema-v5.md)
adds explicit writable admission and only the exact V4-to-V5 bootstrap in
PR-REQ-0300; the M4 binary MUST NOT implicitly chain older schemas through V4.
Internal persistence remains non-Frozen and non-public.

**Verification: PR-TEST-0073, PR-TEST-0082, PR-TEST-0202.**

### PR-REQ-0079 - Revision Core format ownership

Each published Revision Core format MUST define its closed semantic schema,
identity-affecting normalization, collection ordering, runtime-content
descriptor, canonical byte profile, hash framing, domain separation, and hash
algorithm.

The Frozen `RevisionCoreFormatV1` contract is defined by
[Revision Core Format V1](../contracts/revision-core-format-v1.md).

**Verification: Pending automated coverage.**

### PR-REQ-0080 - Snapshot integrity format ownership

Each Snapshot integrity format MUST define its integrity-bearing fields,
SnapshotId binding, producer and provenance normalization, complete managed
binding representation, service-content roles, collection ordering, canonical
bytes, domain separation, and hash profile. It MUST hash a semantic manifest,
not archive bytes. `SnapshotIntegrityFormatV1` MUST apply semantic normalization
before RFC 8785 JCS encoding and use a fixed SHA-256 profile. The exact Frozen
contract is defined by
[Snapshot Integrity Format V1](../contracts/snapshot-integrity-format-v1.md).

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
the same abstraction remains a separate taxonomy gate. M6.5 supplies the
independently versioned [Core V2](../contracts/revision-core-format-v2.md) and
[Hook V2](../contracts/hook-protocol-v2.md) representations; M7 still owns
destructive finalization, Cleanup receipts and abandonment.

**Verification: PR-TEST-0331, PR-TEST-0369, PR-TEST-0385.**

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
