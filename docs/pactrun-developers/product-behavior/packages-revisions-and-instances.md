---
title: Packages, Revisions, and Instances
---

# Packages, Revisions, and Instances

**Status: Normative product behavior specification except where marked
informative.**

## The resource model

A **Package** is a stable product lineage. A **Revision** is one immutable
operational definition and its owned runtime content. An **Instance** is the
long-lived service object that a user manages against one active Revision.

Installing a Package creates or reuses an immutable Revision; it does not create
an Instance. Exact internal identity rules are defined in
[Identity and State](../architecture/identity-and-state.md).

## PR-REQ-0086 - Exact resolution before operation

Every operation that accepts a human Package, Revision, Instance, Snapshot, or
Run reference MUST resolve it to the appropriate exact identity before
compilation or mutation. An ambiguous Revision reference MUST fail with an
actionable ambiguity error and MUST NOT silently select a newest or most
recently installed candidate.

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
association and its provenance when more than one claim exists.

**Verification: Pending automated coverage.**

## Incomplete Instances

An Instance may legally exist before all required Inputs are bound. This makes
creation and Migration independent from immediate execution readiness. Use
Instance inspection to discover missing Inputs, then bind them through Input
management. Ordinary Actions and Snapshot Capture remain blocked until the
active Revision's required bindings are present.

## Package scope

Pactrun currently manages Packs. A Stack is not a supported managed resource and
has no implied installation, Revision, Instance, Action, Snapshot, Migration,
or CLI behavior.
