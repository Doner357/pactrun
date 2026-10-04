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

The approved [managed-object lifecycle contract](../behavior/managed-object-lifecycle.md)
defines explicit deletion, Artifact delivery and foreground collection. Its
[implementation record](../../development/managed-object-lifecycle-status.md)
distinguishes approval from verified runtime availability.

### PR-REQ-0072 - Snapshot lifetime

A committed Snapshot MUST be a durable first-class object with no default
expiry in the initial product scope. Instance and Revision deletion MUST NOT
cascade-delete Snapshots, and Snapshot content MUST NOT depend on the creating
Run or Run Artifact remaining available.

**Verification: PR-TEST-0421.**

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

**Verification: PR-TEST-0106, PR-TEST-0110, PR-TEST-0421.**

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

**Verification: PR-TEST-0423, PR-TEST-0428.**

### PR-REQ-0075 - Revision deletion guards

Pactrun MUST reject deletion of a Revision referenced by an active Instance or
durably pinned by an accepted Run. A compile-only Plan MUST NOT prevent
deletion. Historical Run identity and Snapshot provenance MUST NOT permanently
prevent Revision deletion.

The [Persistence baseline](../persistence/persistence-baseline.md) represents
the active-Instance guard with an exact `ON DELETE RESTRICT` foreign key. The
[Instance retirement contract](../execution/m7-instance-retirement.md) separately
owns the deletion workflow; a relational guard does not perform Cleanup.

**Verification: PR-TEST-0459, PR-TEST-0465, PR-TEST-0470.**

### PR-REQ-0076 - Physical content reachability

Physical GC MUST NOT remove content reachable from any managed object,
Snapshot, accepted-Run pin, checkpoint, or recovery state. GC MUST use strong
references or reachability rather than interpreting operation names or service
semantics. This rule governs Pactrun-owned content stores; it does not authorize
Pactrun to delete service-owned live resources. In particular, abandonment does
not make ServiceStorage-backed state collectible as unreferenced storage. The
non-destruction policy in PR-REQ-0247 and its durable implementation in the
[retirement contract](../execution/m7-instance-retirement.md) preserve that boundary.

**Verification: PR-TEST-0461, PR-TEST-0463, PR-TEST-0464, PR-TEST-0467, PR-TEST-0469, PR-TEST-0471, PR-TEST-0474.**

## Independent version domains

The [formal product compatibility policy](./product-versioning-and-compatibility.md)
defines the approved same-Major promise and first-formal-baseline consolidation.
It is distinct from the version-domain separation below and does not silently alter
current Frozen codecs or authorize a reset during ordinary implementation.

### PR-REQ-0077 - Separate version domains

There is one product version and exactly eight independent format/protocol
domains: Pack source, Revision canonical, Hook Protocol, Snapshot content/integrity,
Snapshot bundle, Pack distribution, CLI machine interface and Persistence.
Results and event streams share the CLI version; runtime closure, Session subviews,
error catalogs and selector discriminators do not create additional domains.

Format values MUST be strings Major.Minor[-prerelease], without Patch. Published
prereleases use alpha.N, beta.N or rc.N with positive canonical decimal N. Numeric
components have no leading zeroes, fit u64, and complete identifiers are bounded
to 128 ASCII bytes. No whitespace, numeric coercion or build suffix is accepted.
Products use the corresponding three-component published SemVer profile.

Each domain owns its implemented readers and writers. A matching Major or an
ordered newer identifier MUST NOT be interpreted as automatic support. Format
Minor does not imply minimum product Minor. The initial values are independently
1.0-alpha.1; future changes must follow each owning contract rather than synchronizing
unrelated domains. Unsupported formats are refused in their necessary scope.

**Verification: PR-TEST-0183, PR-TEST-0338, PR-TEST-0369, PR-TEST-0385, PR-TEST-0538,
PR-TEST-0557, PR-TEST-0618, PR-TEST-0619, PR-TEST-0625.**

### PR-REQ-0078 - Persistence migrations

The E one-time reset establishes the complete
[fresh baseline schema](../persistence/persistence-baseline.md). Development-era
schemas are unsupported: no implicit or explicit upgrade chain is provided.
Refusal MUST NOT clear real data, rewrite identities, infer Run outcomes, or touch
service-owned resources. Pack-defined Revision Migration is a separate feature
and is not retired by this persistence reset.

The exact public version is the string in the singleton metadata row. SQLite's
application marker and private bootstrap marker alone do not prove support.
Opening an existing store MUST validate the exact supported metadata, table and
index manifest, column declarations, constraints, keys and references. Pristine
means zero ownership markers and no non-SQLite objects. Partial, foreign,
unmarked non-empty and unsupported stores MUST be refused without repair.

Support inspection precedes Pactrun staging/session and content-coordination
creation. SQLite MAY create or update its own read-coordination sidecars while
performing ordinary read-only inspection. This exception MUST NOT change existing
database or committed WAL content, ignore committed WAL frames, grant write
admission, rewrite objects, run cleanup, or interfere with service resources.
An existing WAL can contain committed data and MUST NOT be discarded as a
"temporary" file. This is read coordination, not a format conversion. Writer
admission MUST repeat qualification after acquiring the serialized transaction;
preflight is advisory only. Fresh bootstrap publishes the complete schema and
admission atomically. Interrupted bootstrap cannot expose a partially admitted
schema. Read-only opening cannot initialize or upgrade storage. Merely reopening
or updating the product MUST preserve existing identities, bytes, bindings,
non-terminal Runs and unresolved recovery obligations without reconciliation.

Any future internal migration requires an explicit supported contract and must
preserve those same obligations; E does not invent a future codec or migration.

**Verification: PR-TEST-0202, PR-TEST-0621, PR-TEST-0622, PR-TEST-0623, PR-TEST-0624, PR-TEST-0639, PR-TEST-0344.**

### PR-REQ-0079 - Revision Core format ownership

Each published Revision Core format MUST define its closed semantic schema,
identity-affecting normalization, collection ordering, runtime-content
descriptor, canonical byte profile, hash framing, domain separation, and hash
algorithm.

The Frozen `RevisionCoreFormatV1` contract is defined by
[Revision Core Format V1](../contracts/revision-canonical.md).

**Verification: PR-TEST-0044, PR-TEST-0045, PR-TEST-0331, PR-TEST-0333, PR-TEST-0493.**

### PR-REQ-0080 - Snapshot integrity format ownership

Each Snapshot integrity format MUST define its integrity-bearing fields,
SnapshotId binding, producer and provenance normalization, complete managed
binding representation, service-content roles, collection ordering, canonical
bytes, domain separation, and hash profile. It MUST hash a semantic manifest,
not archive bytes. `SnapshotIntegrityFormatV1` MUST apply semantic normalization
before RFC 8785 JCS encoding and use a fixed SHA-256 profile. The exact Frozen
contract is defined by
[Snapshot Integrity Format V1](../contracts/snapshot-integrity.md).

**Verification: PR-TEST-0013, PR-TEST-0015, PR-TEST-0016, PR-TEST-0018,
PR-TEST-0019.**

### PR-REQ-0081 - Bundle envelopes

Revision and Snapshot bundles MUST be self-describing, versioned envelopes that
identify their kind, format version, manifest, and content. Unsupported formats
MUST be rejected rather than guessed. Packaging or compression changes MUST NOT
change contained domain identity.

For Revision transport, this envelope is the user-facing distribution Pack in
[Pack Distribution V1](../contracts/pack-distribution.md), not a separately
managed Bundle object. Snapshot transport remains independent and unchanged.

**Verification: PR-TEST-0538, PR-TEST-0540, PR-TEST-0542.**

### PR-REQ-0082 - Hook Protocol version

The canonical Hook Protocol MUST have its own language-neutral version covering
Session establishment, authority, I/O transitions, typed contexts and outputs,
recovery-risk messages, cancellation, EOF, and protocol violations. SDKs and
helpers MUST remain adapters to that protocol.

**Verification: PR-TEST-0020, PR-TEST-0022, PR-TEST-0023, PR-TEST-0026, PR-TEST-0028, PR-TEST-0030, PR-TEST-0031, PR-TEST-0032, PR-TEST-0362, PR-TEST-0491.**

### PR-REQ-0240 - Future ServiceStorage version gates

ServiceStorage declarations and Session authorities MUST follow their explicitly
versioned [Revision](../contracts/revision-canonical.md) and
[Hook](../contracts/hook-protocol.md) contracts. Implementers MUST NOT add them
retroactively to an older closed schema or authority union. The current
complete baselines include these capabilities; the retired numeric development
formats are not alternative supported readers.

Revision Core and Hook Protocol remain independent version domains. A future
Revision Core format MUST NOT imply that every Hook uses the same-numbered or a
new Hook Protocol version. PR-REQ-0235 through PR-REQ-0248 own the resource
semantics. Their implemented declaration, authority and publication mechanisms
are owned by the current baselines and
[ServiceStorage execution](../execution/m6-5-service-storage-execution.md);
[retirement](../execution/m7-instance-retirement.md) owns Cleanup receipts,
finalization and abandonment custody.

Whether resources outside ServiceStorage use the same abstraction remains a
separate taxonomy gate. Completion of the bounded ServiceStorage contracts does
not authorize that broader design. The historical requirement heading is retained
for stable references; it does not describe these implemented mechanisms as
pending work.

**Verification: PR-TEST-0331, PR-TEST-0369, PR-TEST-0385.**

### PR-REQ-0083 - Structured CLI version

Human-readable output MAY evolve for usability. Machine-readable CLI output
MUST be a versioned interface, and breaking changes MUST require an explicit
version change. Interactive Hook terminal streams MUST NOT be wrapped in the
structured output envelope.

**Verification: PR-TEST-0554, PR-TEST-0556, PR-TEST-0557.**

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
are now owned by the [Pack distribution baseline](../contracts/pack-distribution.md), not by the
historical metadata classification alone.

**Verification: PR-TEST-0538, PR-TEST-0539, PR-TEST-0540, PR-TEST-0544, PR-TEST-0594.**

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
local-only. Export serialization, selection, carriage, import conflict and
merge policy are owned by the [Pack distribution contract](../contracts/pack-distribution.md)
and MUST NOT alter Revision identity.

**Verification: PR-TEST-0058.**

### PR-REQ-0085 - Snapshot import identity

Snapshot export and import MUST preserve `SnapshotId`, integrity format and
digest, producer Revision identity, immutable provenance, complete managed
binding state, and service recovery content. The same Snapshot ID and digest
MUST be idempotent; the same ID with a different digest MUST be rejected as an
identity collision.

**Verification: PR-TEST-0206, PR-TEST-0208, PR-TEST-0209, PR-TEST-0213, PR-TEST-0269.**
