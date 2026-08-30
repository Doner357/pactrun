---
title: Recipes and Runtime Content
---

# Recipes and Runtime Content

**Status: Normative Package contract specification.**

## Recipes

### PR-REQ-0137 - Trusted install-time authoring

A Recipe MUST be treated as a trusted install-time authoring program. It MAY
inspect a provided InstallContext, vary by platform or architecture, fetch from
the network, resolve mutable upstream references, run authoring tools, and stage
Revision-owned content.

**Verification: Pending automated coverage.**

### PR-REQ-0138 - Revision is the reproducibility boundary

Pactrun MUST NOT promise that the same Source produces the same Revision. It
MUST guarantee that the same canonical Revision Core and the same owned runtime
content produce the same `RevisionContentDigest`.

**Verification: Pending automated coverage.**

### PR-REQ-0139 - Recipe output boundary

The only formal Recipe output MUST be a `RevisionCandidate`. A Recipe SHOULD
NOT perform service lifecycle side effects as part of the authoring transaction;
service behavior belongs in Hooks.

**Verification: Pending automated coverage.**

### PR-REQ-0140 - Language-neutral Authoring Contract

The Recipe Authoring Contract MUST be language-neutral at the semantic level,
versioned independently, and isolated from Pactrun's internal domain types.
Python, Go, Rust, or other SDKs MUST be adapters rather than separate semantic
APIs.

**Verification: Pending automated coverage.**

## Runtime content

### PR-REQ-0141 - Pactrun-owned materialization

Pactrun MUST materialize runtime content it needs to execute an Installed
Revision. Runtime behavior MUST NOT depend on mutable source files after
installation. This is immutable, Revision-scoped, Pactrun-owned content. It
MUST NOT be confused with Instance-scoped `ServiceStorage` or ServiceStorage-
backed Managed Service Resources whose live contents are service-authoritative
and may change across Runs.

**Verification: Pending automated coverage.**

### PR-REQ-0142 - Logical content roles

Authors and frontends MUST provide enough normalized information for Pactrun to
bind immutable content to logical runtime roles and paths. Reassigning identical
blobs to different semantic roles MAY produce a different Revision digest.

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
