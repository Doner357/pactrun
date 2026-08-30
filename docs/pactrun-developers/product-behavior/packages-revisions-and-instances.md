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
