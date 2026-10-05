---
title: Packages, Revisions, and Instances
---

# Packages, Revisions, and Instances

<!-- spec-navigation:start -->
<a id="reading-map-informative" />

A Package identifies a product lineage. A Revision defines how that product
operates. An Instance is the long-lived object a user manages using one active
Revision.

## The resource model

| Object | Identity | What it represents |
| --- | --- | --- |
| Package | `PackageId` | A stable lineage that continues across Revision changes. |
| Revision | `PackageId` and `RevisionContentDigest` | One immutable operational definition and its owned runtime content. |
| Instance | `InstanceId` | A managed object with a human name, one active Revision, and its own persistent data. |

Installing a Revision does not create an Instance. Multiple Instances can use
the same installed Revision, while retaining separate identities and Input
Bindings. Installing another Revision does not itself change their active
Revision.

A version label is a reference to a Revision, not its identity. An Instance name
is a human reference, not the identity used to associate its Run and Snapshot
history. The [identity contract](../packages/identity.md) defines
the exact representations and equality rules.
<!-- spec-navigation:end -->

## Resolving references

### PR-REQ-0086 - Exact resolution before operation

Every operation that accepts a human Package, Revision, Instance, Snapshot, or
Run reference MUST resolve it to the appropriate exact identity before
compilation or mutation. An ambiguous Revision reference MUST fail with an
actionable `resolution.ambiguous_reference` error. Pactrun MUST NOT silently
select the newest or most recently installed candidate.

Reference-label resolution MUST compare the exact label and deduplicate by
exact Revision target before deciding ambiguity. Multiple provenance sources
for one target remain one candidate. Deterministic candidate presentation MUST
use the [canonical typed Revision ordering](../packages/metadata.md#pr-req-0250---reference-label-binding-lookup-and-ordering).
It MUST NOT depend on installation, insertion, timestamp, locale, or database
query order.

**Verification: PR-TEST-0062, PR-TEST-0066, PR-TEST-0532, PR-TEST-0586, PR-TEST-0587, PR-TEST-0588.**

### PR-REQ-0089 - Revision labels remain references

User-visible version strings and labels MUST be presented as human references,
not immutable Revision identity. When more than one claim exists, inspection
MUST be able to show every association and its typed source tuple, including
unattributed bindings.

Inspection MAY offer non-authoritative normalized or fuzzy discovery. Exact
resolution, equality, and ambiguity MUST remain based on the preserved exact
label.

**Verification: PR-TEST-0532.**

## Installing a Revision {#m2-installation-and-instance-closure}

Installation validates a Candidate before publishing durable content, then
publishes the Revision and its metadata together. The
[Pack source contract](../packages/source-format.md) defines source acquisition,
normalization, and the identity-bearing projection.

### PR-REQ-0261 - Revision installation publication

Installation MUST perform these steps in order:

1. Acquire and hash exact source objects without durably publishing them.
2. Complete intrinsic Candidate validation, Frozen projection, and construction
   and validation of the typed metadata plan.
3. Durably publish every distinct runtime-content blob before creating its
   database reference.
4. In one `BEGIN IMMEDIATE` transaction, atomically create or reuse the Package,
   publish the exact immutable Revision and closure-derived references, and
   apply the source-projected portable metadata plus any crate-private,
   explicitly accepted local metadata batch.

A semantically invalid Pack Source MUST create no durable runtime blob.
Physical publication and witness acquisition are deduplicated by `blob_digest`,
not by the author-declared `ContentId`. Different semantic entries with
identical bytes share one physical blob and one current same-store witness.

Source-projected metadata MUST use the exact reference-label, presentation, and
provenance shapes in the [Pack source schema](../packages/source-format.md#pr-req-0258---packsourceyamlv1-schema-numbers-and-package-lineage).
Its target is the Revision identity derived by the
[Candidate projection](../packages/source-format.md#pr-req-0260---revisioncandidate-projection-and-metadata-boundary).
Source authoring MUST NOT restate that identity. The explicit local metadata
batch remains separate orchestration input.

Metadata application MUST preserve these conflict and retry rules:

- An identical set tuple is idempotent; distinct legal set claims coexist.
- An absent current value may initialize; current-equals-desired is idempotent.
- A current-state, alias, or typed compare-and-set (CAS) conflict rolls back the
  entire installation transaction.

Reinstallation MUST NOT gain implicit last-write-wins authority. An exact
Revision retry is identity-idempotent. A committed Revision with different
canonical components or derived references is corruption or collision.

The failure boundary determines what may remain:

| Failure point | Required result |
| --- | --- |
| Before durable-blob publication | Failure MUST leave no runtime blob. |
| After durable-blob publication, before database commit | Failure MUST leave no Package-dependent partial Revision, reference, or metadata publication. It MAY leave only complete, valid, unreferenced immutable runtime blobs. |
| After database commit, before the response | A crash MUST converge under exact retry. |

The mutable source root, source paths, filesystem timestamps, install time, and
install history MUST NOT be persisted as a `LocalInstall` record.

**Verification: PR-TEST-0072.**

### PR-REQ-0262 - Migration relational installation policy

For every installed target Revision's inbound Migration edge, Pactrun MUST
derive one state from the Revisions installed in the local repository:

| State | Meaning |
| --- | --- |
| `NotEvaluated` | The exact source Revision is not installed. |
| `Valid` | The exact source Revision is installed and typed roles match. |
| `Invalid` | The exact source Revision is installed and relational semantics do not match. |

This state MUST NOT be persisted, included in Revision identity, or used as a
target installation prerequisite. Installation order therefore MUST NOT change
whether the same target Candidate can be installed.

`Invalid` MUST be surfaced in installation results and Revision inspection.
`NotEvaluated` MUST remain distinguishable from both validity and invalidity.
Only `Valid` may enter an executable Plan. `NotEvaluated` and `Invalid` each
block execution of that edge without retroactively invalidating the installed
target Revision.

**Verification: PR-TEST-0072.**

## Instance names and inspection

### PR-REQ-0263 - Instance name and inspection order

`InstanceName` MUST contain 1 through 128 UTF-8 bytes representing a valid
Unicode scalar sequence. Pactrun MUST preserve its exact bytes and MUST NOT
normalize Unicode, fold case, trim, or compare by locale. It MUST reject
U+0000 through U+001F, U+007F through U+009F, U+2028, and U+2029.

Equality and authoritative ordering use unsigned lexicographic comparison of
the preserved UTF-8 bytes. One management environment MUST contain at most one
live Instance with an exact name.

The name is a human reference, not historical identity. `InstanceId` remains
the opaque stable referent, and a reused name receives a new ID. A deterministic
Instance listing MUST order by `InstanceName`, then `InstanceId`. It MUST NOT
use creation time, insertion order, rowid, locale, or query-plan order.

Inspection MUST show the exact active Revision, state version, derived
active/retained Input roles, missing required Inputs, and local Revision trust
without exposing managed payload bytes.

**Verification: PR-TEST-0525, PR-TEST-0078, PR-TEST-0079.**

### PR-REQ-0087 - Instance presentation

Instance list/show MUST expose the stable InstanceId and exact active Revision.
Input/Secret completeness and the current recovery guard MUST be derived from
one read snapshot and presented separately. Missing required bindings are
identified without reading values.

Absence of a guard is not proof of service health or unconditional execution
eligibility. No persistent readiness flag is introduced. User-facing inspection
MUST distinguish:

- The stable managed object and its human name.
- The active exact Revision and current trust state.
- Configuration completeness.

Missing required Inputs MUST NOT be represented as an invalid object or an
invented `NeedsConfiguration` lifecycle state.

**Verification: PR-TEST-0525.**

### PR-REQ-0088 - Historical identity after name reuse

Run and Snapshot history MUST remain associated with the original opaque
Instance identity after the Instance is deleted. Reusing the same human name
MUST NOT make old history appear to belong to the new Instance. Rename is not
required; if added, it MUST preserve `InstanceId`.

**Verification: PR-TEST-0531, PR-TEST-0535.**

## Incomplete Instances

An Instance may legally exist before all required Inputs are bound. Creation
and Migration are therefore independent of immediate execution readiness. Use
Instance inspection to discover missing Inputs, then bind them through
[Input management](../instances/inputs-secrets.md).

Ordinary Actions and Snapshot Capture remain blocked until the active
Revision's required bindings are present. Completing those bindings does not,
by itself, establish service health or satisfy every execution prerequisite.

## ServiceStorage-backed live resources

An Instance's persistent data can include both Pactrun-owned Managed Input
Bindings and service-owned live resources. Their authority is different: a
binding is a detached, opaque value managed by Pactrun; a live resource may change while
the service runs.

A Revision may declare a stable ServiceStorage-backed Managed Service Resource
even when its live object is absent. The service may create or mutate the live
state later without advancing `InstanceStateVersion`. Pactrun may manage the
resource contract and continuity without owning its authoritative bytes.

Compatible Revisions interpret and use the same Instance live resource rather
than copying it into Revision-specific files. A resource declared only by the
source Revision of a transition is retained conservatively unless destructive
removal is explicit. The
[Revision contract](../packages/revision-format.md),
[ServiceStorage execution contract](../instances/service-storage.md),
and [Instance retirement contract](../lifecycle/retirement.md)
define declarations, compatibility, continuity, retention, discard, access, and
prerequisites.

This model does not classify Docker volumes, external databases, remote objects,
or other resources outside ServiceStorage.

## Package scope

Pactrun manages Packs. A Stack is not a supported managed resource and has no
implied installation, Revision, Instance, Action, Snapshot, Migration, or CLI
behavior.
