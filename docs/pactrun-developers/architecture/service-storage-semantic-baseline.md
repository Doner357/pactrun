---
title: ServiceStorage Semantic Baseline
---

# ServiceStorage Semantic Baseline

**Status: Canonical semantic synthesis and navigation entry point.**

This page summarizes the representation-independent ServiceStorage semantics
closed before M1-D. It is not a competing normative source and intentionally
contains no independent requirement. Normative authority remains with the
linked requirement-bearing specification pages. If a summary here differs from
a linked requirement, the linked requirement controls.

## Scope

This closure covers Pactrun-provided `ServiceStorage` and ServiceStorage-backed
Managed Service Resources, including the file-shaped Managed Service File
subtype. It does not decide whether Docker volumes, external databases, remote
objects, or any other non-ServiceStorage-backed service-owned resource must,
may, or cannot use the same abstraction. That broader resource taxonomy remains
deferred.

## Closed semantics

| Topic | Closed synthesis | Normative authority |
| --- | --- | --- |
| Persistent ownership | Managed Input Bindings are detached and Pactrun-authoritative; ServiceStorage-backed resources are attached and service-authoritative in their live contents. Pactrun owns the provided-storage lifetime without acquiring content authority. | [PR-REQ-0235](./identity-and-state.md#pr-req-0235---persistent-instance-data-ownership) |
| Declaration and scope | ServiceStorage is Pactrun-provided and Instance-scoped. A resource declaration is separate from whether its live object exists. | [PR-REQ-0236](./identity-and-state.md#pr-req-0236---managed-service-resource-declaration-and-existence) |
| Semantic identity | Storage and resource identities are separate, stable, non-reusable within a Package lineage, and denote contract roles rather than shared objects across Instances. A locator is not identity. | [PR-REQ-0241](./identity-and-state.md#pr-req-0241---servicestorage-and-resource-semantic-identity) |
| Live existence | `Present`, `Absent`, and `Unknown` are point-in-time observations. Presence does not prove validity or coherence, and live changes do not advance `InstanceStateVersion`. | [PR-REQ-0242](./identity-and-state.md#pr-req-0242---declaration-existence-and-observation) |
| User exposure | Read exposure and mutation route are separate. Direct mutation does not imply hot-edit safety or Pactrun-managed quiescence, reload, rollback, or linearization. | [PR-REQ-0243](../package-contracts/actions-inputs-and-parameters.md#pr-req-0243---resource-exposure-and-mutation-route) |
| Hook authority and prerequisites | Future authority has whole-storage and individual-resource scopes with least privilege. It differs from Workspace and OS confinement. Presence prerequisites differ from Input readiness and access authority. | [PR-REQ-0244](../package-contracts/hooks-recovery-and-cleanup.md#pr-req-0244---persistent-storage-authority-and-prerequisites) |
| Compatible continuity | Compatible Revisions interpret the same Instance live resource in place. Source-only resources are not destroyed solely because the target stops declaring them. | [PR-REQ-0237](../package-contracts/migrations.md#pr-req-0237---service-resource-continuity-and-conservative-retention) |
| Explicit mappings | Pactrun does not infer compatibility from locators or bytes. Locator, representation, identity, split, and merge changes use explicit target-owned semantics. | [PR-REQ-0245](../package-contracts/migrations.md#pr-req-0245---explicit-compatibility-and-resource-mapping) |
| Transformation ownership | The Package owns service-specific transformation and the recovery-risk handshake; Pactrun does not parse or atomically transact service bytes with SQLite. | [PR-REQ-0238](../package-contracts/migrations.md#pr-req-0238---service-transformation-and-recovery-boundary) |
| Target publication | Risk remains open until the Pactrun-owned target boundary can publish. A pre-publication crash after possible target coherence leads to manual recovery rather than a false source-coherence claim. | [PR-REQ-0246](./recovery-and-reconciliation.md#pr-req-0246---service-transformation-target-publication-boundary) |
| Snapshot capture | ServiceStorage is not scanned or mirrored automatically. Capture submits a recovery representation, and V1 does not require a resource-identity provenance mapping. | [PR-REQ-0239](../package-contracts/snapshots-and-managed-data.md#pr-req-0239---servicestorage-backed-resource-capture-boundary) |
| Cleanup and deletion | Normal deletion ends the provided storage lifetime only after a durable Cleanup-completed boundary. After that boundary, only Pactrun-owned finalization may retry. | [PR-REQ-0247](../product-behavior/snapshots-migrations-and-recovery.md#pr-req-0247---cleanup-finalization-and-abandonment) |
| Abandonment | Abandonment ends management without authorizing destruction of service-owned state during the operation or through later ordinary GC. | [PR-REQ-0113](../product-behavior/snapshots-migrations-and-recovery.md#pr-req-0113---abandonmanagement-intent), [PR-REQ-0181](../package-contracts/hooks-recovery-and-cleanup.md#pr-req-0181---abandonment-skips-package-code), [PR-REQ-0247](../product-behavior/snapshots-migrations-and-recovery.md#pr-req-0247---cleanup-finalization-and-abandonment) |
| Version gates | Closed semantics do not add fields or authority to Frozen V1 formats. Version domains remain independent. | [PR-REQ-0240](./resources-and-versioning.md#pr-req-0240---future-servicestorage-version-gates) |
| M1-D boundary | Non-identity metadata persistence cannot carry ServiceStorage operational semantics or obligations. | [PR-REQ-0248](./identity-and-state.md#pr-req-0248---m1-d-metadata-boundary) |

## Cleanup ambiguous-completion gate

The Frozen Hook completion rule says that loss before accepted completion
prevents protocol success, prohibits inferred replay or compensation, and that
`completion_accepted` does not itself imply a Run or Instance commit. See
[PR-REQ-0216](../package-contracts/hook-protocol-v1.md#pr-req-0216---operation-completion-and-terminal-states).

Consequently, a Cleanup success submission followed by loss before Pactrun
durably publishes the Cleanup-completed/do-not-replay boundary is not treated as
proof of durable success and is not treated as permission to replay Cleanup.
The protocol and recovery coordination that resolves this ambiguous window is
deferred. Once the durable boundary exists, only Pactrun-owned storage-lifetime
finalization may resume, as summarized from PR-REQ-0247.

## Deferred representation and mechanism

### Future Revision Core

- storage and resource declaration serialization and canonical projection;
- identity encoding, association, locator, compatibility, and mapping syntax;
- exposure, prerequisite, and lifetime encoding;
- authoring YAML or Recipe API;
- the broader taxonomy for non-ServiceStorage-backed service-owned resources.

### Future Hook Protocol and recovery

- whole-storage and individual-resource authority wire representation;
- handles, Session fields, visibility, and path materialization;
- target-commit and risk-clear request/result/ack coordination;
- Cleanup completion acceptance and durable do-not-replay publication
  coordination;
- protocol negotiation and conformance fixtures.

### Future persistence and runtime

- SQLite or other durable representation;
- continuity, retention, discard, and non-destruction state representation;
- Cleanup-completed and storage-finalization obligation representation;
- storage allocation, filesystem layout, host paths, locking, and presence
  observation;
- AbandonManagement discoverability, operator handoff, and explicit discard;
- concrete ServiceStorage runtime and CLI.

As summarized from
[PR-REQ-0235](./identity-and-state.md#pr-req-0235---persistent-instance-data-ownership)
and
[PR-REQ-0248](./identity-and-state.md#pr-req-0248---m1-d-metadata-boundary),
Managed Input persistence and M1-D metadata are not substitutes for these
deferred designs.

## M1-D re-entry

The M1-D scope summarized by
[PR-REQ-0248](./identity-and-state.md#pr-req-0248---m1-d-metadata-boundary)
contains already specified typed presentation, provenance, label,
reference-label, local-alias, local-note, and local-trust metadata. It excludes
every ServiceStorage declaration, identity, association, presence observation,
live content, authority, prerequisite, continuity, retention, discard,
Cleanup/finalization obligation, Abandon non-destruction obligation, and broader
resource-taxonomy decision listed there.

The closed metadata contract is navigated from the
[Non-Identity Metadata Semantic Baseline](./non-identity-metadata-semantic-baseline.md).
That closure does not narrow or replace any ServiceStorage exclusion above.
