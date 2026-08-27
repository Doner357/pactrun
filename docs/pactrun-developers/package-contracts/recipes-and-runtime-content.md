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
installation.

**Verification: Pending automated coverage.**

### PR-REQ-0142 - Logical content roles

Authors and frontends MUST provide enough normalized information for Pactrun to
bind immutable content to logical runtime roles and paths. Reassigning identical
blobs to different semantic roles MAY produce a different Revision digest.

**Verification: Pending automated coverage.**

## Presentation and provenance

Presentation and publisher information can be attached without changing
Revision identity. Authors must reference stable semantic keys and must not use
presentation overlays to create Actions, Inputs, or Parameters that are absent
from the installed `RevisionCore`.
