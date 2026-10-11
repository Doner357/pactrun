---
title: Pack Authoring Model
---

# Pack Authoring Model

## Low floor, high ceiling

A minimal Pack can contain metadata, one Action, and one Hook. Structured
parameters, persistent Inputs, managed outputs, Snapshots, Migration,
ServiceStorage-backed Managed Service Resources, and interactive terminal
behavior are optional capabilities and must not burden minimal Packs.

Authors describe capabilities they provide. They are not required to enumerate
large sets of negative `supports_x: false` flags.

The [Pack source format](./source-format.md) is the built-in YAML authoring
interface. Pactrun translates it into an internal Candidate; the Candidate model
is not a public frontend API.

### PR-REQ-0121 - Revision Candidate output

Every authoring frontend MUST ultimately produce a `RevisionCandidate`
containing a host-source-independent `NormalizedPackDefinition` and
Pactrun-owned staged runtime content. Source YAML, frontend ASTs, SDK-specific
values, host source locators, opened handles, and staging paths MUST NOT become
installed execution contracts directly.

An authoring frontend MAY first produce an internal source-stage candidate
whose runtime records still contain validated host source locators. Such a
candidate is an acquisition input, not a weakened `NormalizedPackDefinition`.
The final normalized definition MUST contain resolved semantic runtime
descriptors with author-declared `ContentId` values and content-derived
`blob_digest` values before it crosses this boundary.

The built-in Pack source frontend uses the exact source profile, schema-directed
scalar rules, portable metadata shapes, safe acquisition and projection owned by
PR-REQ-0258 through PR-REQ-0260.

**Verification: PR-TEST-0069.**

### PR-REQ-0122 - Normalized projection boundary

Before installation, Pactrun MUST validate the normalized definition and
project it into identity-bearing `RevisionCore`, the runtime-content closure,
typed current presentation metadata, and typed provenance claims. The normalized
authoring model MUST NOT itself be treated as
the hash or persistence object, and the projection MUST NOT emit a generic
metadata map, arbitrary JSON payload, or storage row shape.

Presentation projection MUST use the closed targets and fields in PR-REQ-0251.
Provenance projection MUST use the closed claims in PR-REQ-0252. Revision-scoped
portable metadata MUST acquire the exact installed Revision target only after
identity projection;
an authoring source MUST NOT be required or permitted to restate that derived
identity.

Local names, notes, and trust assessments are not part of
`NormalizedPackDefinition`. A crate-private installation orchestrator MAY
supply a separate explicit local metadata mutation batch. The built-in source
frontend carries portable metadata only, and the human install command supplies
an empty local metadata batch. Requested Package and Revision names are separate
installation inputs governed by PR-REQ-0372. Source syntax and installation
publication are owned by PR-REQ-0258 through PR-REQ-0261; transport encoding is
owned separately by the
[Pack distribution contract](./distribution.md).

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
|- PresentationMetadata
|- ProvenanceMetadata
|- InputDeclarations
|- ServiceStorageDeclarations
|- ServiceResourceDeclarations
|- Actions
|- SnapshotCapability
|- InboundMigrationEdges
|- InstanceLifecycle.Cleanup
`- RuntimeContentClosure
```

Each Action and lifecycle capability owns its own parameters, requirements, and
implementation. Presentation and provenance may coexist with
semantics in the authoring model, but the installation projection separates
their identity roles. A separate orchestration-supplied local metadata batch is
installation context, not authoring content.

Capability-specific fixed prerequisites are semantics of the capability kind.
They are not an author-configurable generic requirements block. Explicit service
grants and presence prerequisites use the closed shapes in the
[Revision baseline](./revision-format.md), which also owns identity projection.

ServiceStorage and ServiceStorage-backed Managed Service Resource declarations
are part of the current normalized model. Their source fields and identity
projection are owned by the [Pack source](./source-format.md) and
[Revision](./revision-format.md) baselines. Session grants are owned by the
[Hook protocol](../interfaces/hook-protocol.md); association, continuity, retention and
publication follow [ServiceStorage execution](../instances/service-storage.md)
and [Instance retirement](../lifecycle/retirement.md). This page does
not create alternate encodings or implicit synchronization. The broader taxonomy
for non-ServiceStorage-backed resources is not defined by this model.

### PR-REQ-0123 - Explicit normalized semantics

An authoring frontend MAY offer conservative shorthand, including implicit
Carry for the same stable Input identity and Keep for a source-only Input.
Normalization MUST materialize the explicit installed transition. Runtime
execution MUST NOT guess authoring defaults again.

**Verification: PR-TEST-0608, PR-TEST-0611.**

### PR-REQ-0124 - Separate authoring capabilities

Action, Snapshot, Migration, and Cleanup MUST have separate author-facing
models. A frontend MUST NOT expose a catch-all operation type with optional
fields for every capability. Low-level execution primitives MAY be shared.

**Verification: PR-TEST-0069, PR-TEST-0336, PR-TEST-0592.**

## Package quality

Pactrun can lint, plan, warn, and provide actionable diagnostics, but it does
not impose unnecessary framework bureaucracy merely to prevent every possible
bad Pack. Authors remain responsible for service-specific correctness, accurate
recovery-risk signaling, and practical retry behavior.
