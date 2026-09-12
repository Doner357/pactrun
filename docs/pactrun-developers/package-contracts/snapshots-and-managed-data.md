---
title: Snapshots and Managed Data
---

# Snapshots and Managed Data

**Status: Normative Package contract specification.**

## Managed data concepts

### PR-REQ-0143 - Workspace

A Workspace MUST be execution-scoped staging and MUST NOT become committed
managed content until the operation-specific validation and commit succeeds.
It MUST NOT be used or reinterpreted as persistent `ServiceStorage`.

**Verification: Pending automated coverage.**

### PR-REQ-0144 - Run Artifact

An ordinary Action's managed-output declarations define the maximum authorized
output vocabulary. Declaration alone MUST NOT require submission or publication
of an output. An Action completion MAY submit a valid subset, including the
empty set, as defined by [PR-REQ-0212](./hook-protocol-v1.md#pr-req-0212---managed-output-authorities-and-completion).

Only outputs validly submitted through a protocol-accepted completion and
successfully published by Pactrun's managed-output commit become Run Artifacts.
Pactrun MUST represent each such successfully published output as a Run
Artifact owned by that Run.

Pactrun MUST NOT create a Run Artifact merely because an output was declared, a
slot was preallocated, or bytes were written to that slot. Unsubmitted outputs,
outputs from an unaccepted completion, and outputs whose managed publication
failed MUST NOT become Run Artifacts.

Publication eligibility and atomic publication behavior are defined by
[PR-REQ-0281](../product-behavior/actions-plans-and-runs.md#pr-req-0281---atomic-managed-output-publication-and-late-completion).
The initial product scope MUST NOT expose Artifact as a global top-level
resource taxonomy.

**Verification: PR-TEST-0106, PR-TEST-0107, PR-TEST-0109.**

### PR-REQ-0145 - SnapshotCandidate

Snapshot Capture output MUST first be a temporary `SnapshotCandidate`. Pactrun
MUST validate it and commit a Snapshot without requiring the candidate to become
a permanent Run Artifact. The Run needs only a reference to the committed
Snapshot.

**Verification: PR-TEST-0246.**

## Snapshot capability

### PR-REQ-0146 - Snapshot domain boundary

Snapshot Capture and Restore MUST be authored as Pactrun-defined managed-state
lifecycle capabilities, not Actions. A Capture Hook provides service recovery
state; Pactrun supplies and records the complete managed binding state.

Their `RevisionCoreFormatV1` identity projection keeps Capture and Restore
separate. Capability-specific prerequisites remain additive to global managed
execution invariants. See
[Revision Core Format V1](./revision-core-format-v1.md).

**Verification: PR-TEST-0246.**

### PR-REQ-0147 - Snapshot authoritative body

A Snapshot's immutable authoritative body MUST contain a stable `SnapshotId`,
integrity format and digest, exact producer Revision identity, authoritative
origin Instance identity, capture time, complete active and retained managed
binding state including absence and protection, and logical service Snapshot
content selected and submitted through Capture.

**Verification: PR-TEST-0246, PR-TEST-0247.**

### PR-REQ-0148 - Snapshot identity roles

`SnapshotId` MUST identify the object, `SnapshotIntegrityDigest` MUST protect
the exact authoritative body, and producer `RevisionIdentity` MUST control
direct Restore compatibility. The digest MUST NOT replace object identity.

**Verification: PR-TEST-0213, PR-TEST-0247.**

### PR-REQ-0149 - Snapshot integrity domain

The integrity digest MUST bind `SnapshotId`, producer identity, authoritative
origin identity, capture time, complete managed binding state, and service
content roles and digests. It MUST exclude presentation-only origin names,
local import metadata, labels, notes, trust decisions, compression, and archive
representation.

The exact V1 identity spelling, semantic manifest, normalization, framing, and
verification boundary are defined by
[Snapshot Integrity Format V1](./snapshot-integrity-format-v1.md).

**Verification: PR-TEST-0184, PR-TEST-0185.**

### PR-REQ-0150 - Capture view consistency

Pactrun MUST pin one binding view for the Capture Hook and the committed
Snapshot. A Package MUST NOT observe one active binding version while the
Snapshot records another. This binding-view guarantee does not freeze or
automatically enumerate service-owned live bytes.

**Verification: PR-TEST-0246, PR-TEST-0249.**

### PR-REQ-0239 - ServiceStorage-backed resource capture boundary

Pactrun MUST NOT automatically scan or Snapshot an entire `ServiceStorage` or
all declared ServiceStorage-backed Managed Service Resources. A resource
declaration MUST NOT by itself make the live resource Snapshot content.

The Capture Hook is responsible for selecting recovery-relevant service-owned
state at an appropriate service consistency boundary, transforming it into a
`SnapshotCandidate`, and submitting that candidate for Pactrun validation,
hashing, and commit. Existing automatic inclusion of complete Pactrun-managed
binding state remains unchanged. The committed service-content closure is the
Capture Hook's immutable recovery representation, not a live
`ServiceStorage` mirror.

Capture may merge, split, select, or transform resources into logical recovery
content, so `SnapshotIntegrityFormatV1` does not require a one-to-one resource-
identity provenance mapping. Its existing exact producer Revision identity and
logical service-content descriptors remain sufficient for V1. A future format
MAY design optional additional provenance, but it is not required by this
semantic closure.

This requirement does not change `SnapshotIntegrityFormatV1` schema, canonical
bytes, framing, digest, or vectors and does not classify non-ServiceStorage-
backed service-owned resources.

**Verification: PR-TEST-0246.**

### PR-REQ-0151 - Restore staged context

The Restore Hook MUST receive the active Input view from the staged Snapshot
binding state that Pactrun will commit on success. It MUST NOT receive the
target's pre-Restore active bindings as though they were the resulting state.

**Verification: PR-TEST-0258, PR-TEST-0268.**

### PR-REQ-0152 - Snapshot Secret handling

Capture MUST include Secret bindings as ordinary managed recovery content and
must not require a separate author flag. Package output and diagnostics MUST
respect Secret redaction, while portable export remains an explicit operator
sensitive-data boundary.

**Verification: PR-TEST-0246, PR-TEST-0268.**
