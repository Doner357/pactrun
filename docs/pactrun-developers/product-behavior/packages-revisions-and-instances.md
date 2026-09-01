---
title: Packages, Revisions, and Instances
---

# Packages, Revisions, and Instances

**Status: Normative product behavior specification except where marked
informative.**

## The resource model

A **Package** is a stable product lineage. A **Revision** is one immutable
operational definition and its owned runtime content. An **Instance** is the
long-lived service object that a user manages against one active Revision. Its
persistent data may include Pactrun-authoritative Managed Input Bindings and,
under a future versioned contract, service-authoritative Managed Service
Resources. Those ownership models are distinct.

Installing a Package creates or reuses an immutable Revision; it does not create
an Instance. Exact internal identity rules are defined in
[Identity and State](../architecture/identity-and-state.md).
The closed M2 source and installation flow is defined by
[Pack Source YAML V1](../package-contracts/pack-source-yaml-v1.md).

## PR-REQ-0086 - Exact resolution before operation

Every operation that accepts a human Package, Revision, Instance, Snapshot, or
Run reference MUST resolve it to the appropriate exact identity before
compilation or mutation. An ambiguous Revision reference MUST fail with an
actionable `resolution.ambiguous_reference` error and MUST NOT silently select a
newest or most recently installed candidate.

Reference-label resolution MUST compare the exact label and deduplicate by
exact Revision target before deciding ambiguity. Multiple provenance sources
for one target remain one candidate. Deterministic candidate presentation MUST
use the canonical typed Revision ordering in PR-REQ-0250 and MUST NOT depend on
installation, insertion, timestamp, locale, or database query order.

**Verification: Pending automated coverage.**

## PR-REQ-0087 - Instance presentation

User-facing Instance inspection MUST distinguish the stable managed object, its
human name, active exact Revision, current trust state, and configuration
completeness. It MUST NOT represent missing required Inputs as an invalid object
or invent a `NeedsConfiguration` lifecycle state.

M2 exact name syntax, deterministic listing, and minimum inspection fields are
defined by PR-REQ-0263.

**Verification: Pending automated coverage.**

## PR-REQ-0088 - Historical identity after name reuse

Run and Snapshot history MUST remain associated with the original opaque
Instance identity after the Instance is deleted. Reusing the same human name
MUST NOT make old history appear to belong to the new Instance. Rename is not
required in the initial product scope; if added later, it MUST preserve
`InstanceId`.

**Verification: Pending automated coverage.**

## PR-REQ-0089 - Revision labels remain references

User-visible version strings and labels MUST be presented as human references,
not immutable Revision identity. Inspection MUST be able to show every
association and its typed source tuple, including unattributed bindings, when
more than one claim exists. Inspection MAY offer non-authoritative normalized
or fuzzy discovery, but exact resolution, equality, and ambiguity MUST remain
based on the preserved exact label.

**Verification: Pending automated coverage.**

## Incomplete Instances

An Instance may legally exist before all required Inputs are bound. This makes
creation and Migration independent from immediate execution readiness. Use
Instance inspection to discover missing Inputs, then bind them through Input
management. Ordinary Actions and Snapshot Capture remain blocked until the
active Revision's required bindings are present.

## M2 installation and Instance closure

### PR-REQ-0261 - Revision installation publication

M2 installation MUST durably publish every distinct runtime-content blob before
creating its database reference. After blob publication, one `BEGIN IMMEDIATE`
transaction MUST atomically create or reuse the Package, publish the exact
immutable Revision and closure-derived references, and apply the source-
projected portable metadata plus any crate-private explicitly accepted local
metadata batch.

Source-projected metadata MUST use the exact reference-label, presentation, and
provenance shapes in PR-REQ-0258. Its target Revision is the identity derived
for this installation under PR-REQ-0260; source authoring MUST NOT restate that
identity. The explicit local batch remains separate orchestration input.

Metadata application MUST retain the exact M1-D semantics: an identical set
tuple is idempotent, distinct legal set claims coexist, an absent current value
may initialize, current-equals-desired is idempotent, and current-state, alias,
or typed-CAS conflict rolls back the entire installation transaction. Reinstall
MUST NOT gain implicit last-write-wins authority. An exact Revision retry is
identity-idempotent; a committed Revision with different canonical components
or derived references is corruption or collision.

Failure before database commit MUST leave no Package-dependent partial
Revision, reference, or metadata publication. It MAY leave a correct
unreferenced immutable runtime blob. A crash after commit but before response
MUST converge under exact retry. The mutable source root, source paths,
filesystem timestamps, install time, and install history MUST NOT be persisted
as a `LocalInstall` record in M2.

**Verification: Pending automated coverage.**

### PR-REQ-0262 - Migration relational installation policy

For every installed target Revision's inbound Migration edge, Pactrun MUST
derive one repository-context state:

```text
NotEvaluated  exact source Revision is not installed
Valid         exact source is installed and typed roles match
Invalid       exact source is installed and relational semantics do not match
```

This state MUST NOT be persisted, included in Revision identity, or used as a
target installation prerequisite. Installation order therefore MUST NOT change
whether the same target candidate can be installed. `Invalid` MUST be surfaced
in installation results and Revision inspection; `NotEvaluated` MUST remain
distinguishable from both validity and invalidity. In M5, only `Valid` may enter
an executable Plan. `NotEvaluated` and `Invalid` each block execution of that
edge without retroactively invalidating the installed target Revision.

**Verification: Pending automated coverage.**

### PR-REQ-0263 - Instance name and inspection order

`InstanceName` MUST contain 1 through 128 UTF-8 bytes representing a valid
Unicode scalar sequence. Pactrun MUST preserve its exact bytes, MUST NOT
normalize Unicode, fold case, trim, or compare by locale, and MUST reject
U+0000 through U+001F, U+007F through U+009F, U+2028, and U+2029. Equality and
authoritative ordering use unsigned lexicographic comparison of preserved UTF-8
bytes.

One management environment MUST contain at most one live Instance with an exact
name. The name is a human reference, not historical identity; `InstanceId`
remains the opaque stable referent and a reused name receives a new ID. A
deterministic Instance listing MUST order by `InstanceName`, then `InstanceId`,
and MUST NOT use creation time, insertion order, rowid, locale, or query-plan
order. Inspection MUST show the exact active Revision, state version, derived
active/retained Input roles, missing required Inputs, and local Revision trust
without exposing managed payload bytes.

**Verification: Pending automated coverage.**

## ServiceStorage-backed live resources

A future Revision may declare a stable ServiceStorage-backed Managed Service
Resource even while the corresponding live object is absent. The service may
create or mutate the live state later without advancing `InstanceStateVersion`.
Pactrun may manage the resource contract and continuity without owning its
authoritative bytes.

Compatible Revisions interpret and use the same Instance live resource rather
than copying it into Revision-specific files. A source-only resource is retained
conservatively unless destructive removal is explicit. The schema and durable
representation of declaration, compatibility, continuity, retention, discard,
access, and prerequisites remain future design work rather than current V1
product support. This model does not yet classify Docker volumes, external
databases, remote objects, or other non-ServiceStorage-backed resources.

## Package scope

Pactrun currently manages Packs. A Stack is not a supported managed resource and
has no implied installation, Revision, Instance, Action, Snapshot, Migration,
or CLI behavior.
