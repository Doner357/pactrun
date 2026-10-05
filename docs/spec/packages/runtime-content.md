---
title: Runtime Content
---

# Runtime Content

Runtime content is the immutable code and support data installed with a
Revision. Logical descriptors identify how that content is used; physical blobs
hold the bytes. Neither represents mutable service data.

## Reproducibility

### PR-REQ-0138 - Revision is the reproducibility boundary

Pactrun MUST NOT promise that the same Source produces the same Revision. It
MUST guarantee that the same canonical Revision Core and the same owned runtime
content produce the same `RevisionContentDigest`.

**Verification: PR-TEST-0044, PR-TEST-0051, PR-TEST-0600.**

## Materialized runtime files {#runtime-content}

### PR-REQ-0141 - Pactrun-owned materialization

Pactrun MUST materialize runtime content it needs to execute an Installed
Revision. Runtime behavior MUST NOT depend on mutable source files after
installation. This is immutable, Revision-scoped, Pactrun-owned content. It
MUST NOT be confused with Instance-scoped `ServiceStorage` or ServiceStorage-
backed Managed Service Resources whose live contents are service-authoritative
and may change across Runs.

The [source acquisition rules](./source-format.md#pr-req-0259---sourcerelativepathv1-and-safe-source-acquisition)
define source-relative paths and same-object staging. Durable blob publication
precedes the [atomic installation commit](../core/objects.md#pr-req-0261---revision-installation-publication).

**Verification: PR-TEST-0143, PR-TEST-0346.**

### PR-REQ-0142 - Logical content roles

Authors and frontends MUST provide enough normalized information for Pactrun to
bind immutable content to logical runtime roles and paths. Reassigning identical
blobs to different semantic roles MAY produce a different Revision digest.

The Pack source format therefore keeps `source` acquisition separate from
the identity-bearing logical `path`, `id`, and `executable` descriptor fields.

This runtime-content closure does not content-address, copy, version, or
automatically Snapshot service-owned live state. The
[Revision format](./revision-format.md) includes ServiceStorage-backed
resource declarations in identity, but their live bytes remain service-owned.
Other service-owned resource types are outside this resource model.

**Verification: PR-TEST-0069, PR-TEST-0331, PR-TEST-0600.**

## Presentation and provenance

Presentation, reference labels, and typed provenance claims can be attached
without changing Revision identity. Presentation is current metadata over the
closed target and field sets in PR-REQ-0251; provenance is the value-keyed
typed claim set in PR-REQ-0252. Authoring and persistence MUST NOT infer a
generic target, key/value map, history, or locale layer from those contracts.
Authors must reference stable semantic keys and must not use presentation
overlays to create Actions, Inputs, Parameters, outputs, or operations that are
absent from the installed `RevisionCore`.
