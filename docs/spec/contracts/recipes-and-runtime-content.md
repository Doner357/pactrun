---
title: Runtime Content and Retired Recipe Design
---

# Runtime content and retired Recipe design

**Status: Normative Package contract specification.**

<!-- spec-navigation:start -->
## Reading map (informative)

Read the active immutable runtime-content rules and the retirement notices for the rejected Recipe proposal.

Start with the [specification map](../index.md)
and [shared vocabulary](../glossary.md) if a term is unfamiliar.
Check [implementation status and remaining decisions](../../development/next-milestone.md)
before treating an approved contract as available runtime behavior.
Recipe-specific requirements below are explicitly retired; the independent identity and runtime-content rules remain active.
<!-- spec-navigation:end -->

<a id="recipes" />

## Retired Recipe proposal and retained identity rule

M8 was rejected on 2026-09-16. The [decision and original text](../../development/history/m8-recipes-rejected.md)
are historical, not future implementation obligations. Existing paths and
requirement anchors remain for traceability; retired IDs are not reused.

### PR-REQ-0137 - Trusted install-time authoring

**Retired: M8 rejected on 2026-09-16.** No install-time Recipe capability is
required. See the [original text and rationale](../../development/history/m8-recipes-rejected.md#retired-requirement-text).

**Verification: Not applicable — retired requirement, not pending runtime coverage.**

### PR-REQ-0138 - Revision is the reproducibility boundary

Pactrun MUST NOT promise that the same Source produces the same Revision. It
MUST guarantee that the same canonical Revision Core and the same owned runtime
content produce the same `RevisionContentDigest`.

**Verification: Pending automated coverage.**

### PR-REQ-0139 - Recipe output boundary

**Retired: M8 rejected on 2026-09-16.** The Recipe-specific output contract is
withdrawn. The existing internal Candidate boundary in PR-REQ-0121/0122 remains
active and does not imply a public API.

**Verification: Not applicable — retired requirement, not pending runtime coverage.**

### PR-REQ-0140 - Language-neutral Authoring Contract

**Retired: M8 rejected on 2026-09-16.** A public Candidate API for additional
frontends is deferred until concrete demand, not an approved implementation task.
The [decision record](../../development/history/m8-recipes-rejected.md) distinguishes
that possible future entry point from rejected install-time generation.

**Verification: Not applicable — retired requirement, not pending runtime coverage.**

## Runtime content

### PR-REQ-0141 - Pactrun-owned materialization

Pactrun MUST materialize runtime content it needs to execute an Installed
Revision. Runtime behavior MUST NOT depend on mutable source files after
installation. This is immutable, Revision-scoped, Pactrun-owned content. It
MUST NOT be confused with Instance-scoped `ServiceStorage` or ServiceStorage-
backed Managed Service Resources whose live contents are service-authoritative
and may change across Runs.

For M2, `SourceRelativePathV1` acquisition and same-object staging are defined
by PR-REQ-0259, and durable blob publication precedes the atomic installation
reference boundary in PR-REQ-0261.

**Verification: Pending automated coverage.**

### PR-REQ-0142 - Logical content roles

Authors and frontends MUST provide enough normalized information for Pactrun to
bind immutable content to logical runtime roles and paths. Reassigning identical
blobs to different semantic roles MAY produce a different Revision digest.

Candidate `PackSourceYamlV1` therefore keeps `source` acquisition separate from
the identity-bearing logical `path`, `id`, and `executable` descriptor fields.

This runtime-content closure does not content-address, copy, version, or
automatically Snapshot service-owned live state. A formal identity-bearing
ServiceStorage-backed Managed Service Resource declaration requires a future
Revision Core format. This requirement does not classify other service-owned
resource kinds.

**Verification: Pending automated coverage.**

## Presentation and provenance

Presentation, reference labels, and typed provenance claims can be attached
without changing Revision identity. Presentation is current metadata over the
closed target and field sets in PR-REQ-0251; provenance is the value-keyed
typed claim set in PR-REQ-0252. Authoring and persistence MUST NOT infer a
generic target, key/value map, history, or locale layer from those contracts.
Authors must reference stable semantic keys and must not use presentation
overlays to create Actions, Inputs, Parameters, outputs, or operations that are
absent from the installed `RevisionCore`.
