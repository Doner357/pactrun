---
title: Authoring Model
---

# Authoring Model

**Status: Normative Package contract specification.**

## Low floor, high ceiling

A minimal Pack can contain metadata, one Action, and one Hook. Structured
parameters, persistent Inputs, managed outputs, Snapshots, Migration, Recipes,
and interactive terminal behavior are optional capabilities and must not burden
minimal Packs.

Authors describe capabilities they provide. They are not required to enumerate
large sets of negative `supports_x: false` flags.

## PR-REQ-0121 - Revision Candidate output

Every authoring frontend MUST produce a `RevisionCandidate` containing a
`NormalizedPackDefinition` and Pactrun-owned runtime content. Source YAML,
Recipe ASTs, and SDK-specific values MUST NOT become installed execution
contracts directly.

**Verification: Pending automated coverage.**

## PR-REQ-0122 - Normalized projection boundary

Before installation, Pactrun MUST validate the normalized definition and
project it into identity-bearing `RevisionCore`, portable presentation
observations, provenance observations, and local management metadata. The
normalized authoring model MUST NOT itself be treated as the hash or persistence
object.

**Verification: Pending automated coverage.**

## Normalized Pack Definition

The model may contain:

```text
NormalizedPackDefinition
|- PackageIdentityDeclaration
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
implementation. Presentation may coexist with semantics in the authoring model,
but the installation projection separates their identity roles.

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
