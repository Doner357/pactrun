---
title: Authoring Model
---

# Authoring Model

**Status: Normative Package contract specification.**

## Low floor, high ceiling

A minimal Pack can contain metadata, one Action, and one Hook. Structured
parameters, persistent Inputs, managed outputs, Snapshots, Migration, Recipes,
future ServiceStorage-backed Managed Service Resources, and interactive terminal
behavior are optional capabilities and must not burden minimal Packs.

Authors describe capabilities they provide. They are not required to enumerate
large sets of negative `supports_x: false` flags.

### PR-REQ-0121 - Revision Candidate output

Every authoring frontend MUST ultimately produce a `RevisionCandidate`
containing a host-source-independent `NormalizedPackDefinition` and
Pactrun-owned staged runtime content. Source YAML, Recipe ASTs, SDK-specific
values, host source locators, opened handles, and staging paths MUST NOT become
installed execution contracts directly.

An authoring frontend MAY first produce an internal source-stage candidate
whose runtime records still contain validated host source locators. Such a
candidate is an acquisition input, not a weakened `NormalizedPackDefinition`.
The final normalized definition MUST contain resolved semantic runtime
descriptors with author-declared `ContentId` values and content-derived
`blob_digest` values before it crosses this boundary.

Candidate `PackSourceYamlV1` is the closed minimal M2 frontend. Its exact source
profile, schema-directed scalar rules, portable metadata shapes, source
acquisition, and projection are owned by PR-REQ-0258 through PR-REQ-0260.

**Verification: PR-TEST-0069.**

### PR-REQ-0122 - Normalized projection boundary

Before installation, Pactrun MUST validate the normalized definition and
project it into identity-bearing `RevisionCore`, the runtime-content closure,
typed reference-label bindings, typed current presentation metadata, and typed
provenance claims. The normalized authoring model MUST NOT itself be treated as
the hash or persistence object, and the projection MUST NOT emit a generic
metadata map, arbitrary JSON payload, or storage row shape.

Reference-label projection MUST use PR-REQ-0250, presentation projection MUST
use the closed targets and fields in PR-REQ-0251, and provenance projection
MUST use the closed claims in PR-REQ-0252. Revision-scoped portable metadata
MUST acquire the exact installed Revision target only after identity projection;
an authoring source MUST NOT be required or permitted to restate that derived
identity.

Local aliases, notes, and trust assessments are not part of
`NormalizedPackDefinition`. A crate-private installation orchestrator MAY
supply a separate explicit local M1-D mutation batch, but Candidate
`PackSourceYamlV1` carries portable metadata only and the M2 human install
command supplies an empty local batch. This contract does not itself redefine
the M2 source syntax or installation transaction, which are owned by
PR-REQ-0258 through PR-REQ-0261. It does not define an Export Bundle Format or
change the M1-D production implementation.

Source-stage locators, directory handles, acquisition evidence, and transient
staging identities MUST be consumed before `NormalizedPackDefinition` is
constructed. They MUST NOT enter Frozen canonical components, Revision
identity, metadata values, or repository rows. `ContentId` remains the
author-declared semantic identifier; only `blob_digest` is derived from staged
bytes.

**Verification: PR-TEST-0069.**

## Normalized Pack Definition

The model may contain:

```text
NormalizedPackDefinition
|- PackageIdentityDeclaration
|- ReferenceLabelMetadata
|- PresentationMetadata
|- ProvenanceMetadata
|- InputDeclarations
|- Actions
|- SnapshotCapability
|- InboundMigrationEdges
|- InstanceLifecycle.Cleanup
`- RuntimeContentClosure
```

Each Action and lifecycle capability owns its own parameters, requirements, and
implementation. Reference labels, presentation, and provenance may coexist with
semantics in the authoring model, but the installation projection separates
their identity roles. A separate orchestration-supplied local metadata batch is
installation context, not authoring content.

In `RevisionCoreFormatV1`, capability-specific fixed prerequisites are
semantics of the capability kind. They are not an author-configurable generic
requirements block. The exact identity projection is defined by
[Revision Core Format V1](./revision-core-format-v1.md).

The displayed normalized model reflects capabilities representable by the
current Frozen format. The accepted future architecture requires a versioned
way to declare `ServiceStorage` and stable ServiceStorage-backed Managed Service
Resources, but this page does not add fields to `RevisionCoreV1` or choose
authoring syntax, identity encoding, association or locator fields, access or
prerequisite encoding, compatibility mapping, continuity, retention, discard,
or persistence representation. Those representations remain formal design
gates. The broader taxonomy for non-ServiceStorage-backed service-owned
resources also remains deferred.

## PR-REQ-0123 - Explicit normalized semantics

An authoring frontend MAY offer conservative shorthand, including implicit
Carry for the same stable Input identity and Keep for a source-only Input.
Normalization MUST materialize the explicit installed transition. Runtime
execution MUST NOT guess authoring defaults again.

**Verification: Pending automated coverage.**

## PR-REQ-0124 - Separate authoring capabilities

Action, Snapshot, Migration, and Cleanup MUST have separate author-facing
models. A frontend MUST NOT expose a catch-all operation type with optional
fields for every capability. Low-level execution primitives MAY be shared.

**Verification: Pending automated coverage.**

## Package quality

Pactrun can lint, plan, warn, and provide actionable diagnostics, but it does
not impose unnecessary framework bureaucracy merely to prevent every possible
bad Pack. Authors remain responsible for service-specific correctness, accurate
recovery-risk signaling, and practical retry behavior.
