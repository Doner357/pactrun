---
title: ServiceStorage Staged Design Alignment
---

# ServiceStorage staged design alignment

**Status: Informative delivery alignment. M6 and M6.5 are implemented and
integrated; M7 lifecycle design and implementation require separate approval.**

This record implements the approved work order, not new product requirements.
The [semantic baseline](./service-storage-semantic-baseline.md) and its owning
PR-REQ-0235 through PR-REQ-0248 remain authoritative. Closed semantics are not
reopened, and Frozen formats are unchanged.

## Ordered delivery and approval boundaries

| Stage | Deliverable | Exit gate |
| --- | --- | --- |
| Pre-M6 | This alignment, the M6 bounded baseline, and an initial evidence inventory | Review the operation/boundary matrix and approve the bounded M6 implementation scope; document any genuine persistence gap before implementation |
| M6 | Recovery hardening for existing Action, Capture, Restore, and Migration execution | Existing-operation crash, ownership, guard, and reference-lifetime coverage; no ServiceStorage or Cleanup runtime claim |
| M6.5 | ServiceStorage representation, persistence, and runtime | Separately approved representation contracts followed by real-resource integration and crash evidence |
| M7 | Cleanup, deletion, and AbandonManagement | Durable do-not-replay/finalization and non-destruction behavior with integration and crash evidence |
| M8 (historical, rejected) | Recipes and advanced authoring | Removed by the [2026-09-16 decision](../history/m8-recipes-rejected.md); no implementation gate remains |

M6.5 is explicitly scheduled after M6 and before M7. Scheduling does not satisfy
its design gates or promote it to `Planned`. The subsequent 2026-09-14 operator
authorization approves bounded M6 S0-S4 implementation without per-slice stops.
M6.5 originally remained `Proposed` until its own baseline and representation
contracts were approved; the M6 authorization did not extend to ServiceStorage implementation.
Bounded M6 is now integrated into local develop as recorded in the
[M6 closeout](../m6-implementation-status.md).
The [M6.5 S0 package](./m6-5-servicestorage-baseline.md) supplied concrete
proposals under the original S0-only authorization. That stop was satisfied:
after review on 2026-09-15 the operator authorized continued S1-S7 implementation
with mapped-source consumption. These historical M6 approval limits do not
revoke the later M6.5 approval. Format Freeze and implementation verification
remain distinct gates, and M7 destructive operations remain outside M6.5.
M6.5 has now been integrated under explicit Git authorization; the
[M6.5 closeout](../m6-5-implementation-status.md) records its implementation and
verification. M7 still needs its own lifecycle design gate.
See the [M6 baseline](./m6-recovery-implementation-baseline.md) and
[roadmap](../implementation-roadmap.md#m65---servicestorage).

## Closed semantics and M6 constraints

- Managed Input Bindings are detached, Pactrun-authoritative values.
  ServiceStorage-backed resources are attached, service-authoritative live
  contents; Pactrun owns provided-storage lifetime, not content authority.
- Storage and resource identity are distinct from locators and from observed
  existence. Live changes do not advance `InstanceStateVersion`.
- Compatible continuity is in place; source-only resources are conservatively
  retained. Transformations use explicit target-owned semantics, not inference
  from paths or bytes.
- A recovery checkpoint restores only the Pactrun-owned committed boundary.
  It neither captures service bytes nor proves service/source coherence.
- Capture submits a recovery representation; it does not scan or mirror live
  storage. Inputs, metadata, Workspace, and implicit synchronization are not
  ServiceStorage implementations.
- Target publication and risk clear respect PR-REQ-0246. Cleanup success,
  its durable do-not-replay boundary, and storage finalization are distinct under
  PR-REQ-0247. Abandonment does not authorize current or later destruction.

M6 preserves these constraints without inventing authority handles, persistent
resource registries, versioned declarations, or future-operation placeholders.
M6 recovery success must never be described as service rollback.

## Design ownership and dependencies

| Design area | Owner and prerequisite |
| --- | --- |
| Core declarations, identity/association/locator encoding, compatibility/mapping, exposure/prerequisites, authoring projection | M6.5 design gate; a future Core format, not an amendment to Frozen Core V1 |
| Protocol-mediated whole-storage/resource authority, materialization and access | M6.5, only where required; version Hook authority independently of Core and preserve Frozen HookProtocolV1 |
| Target commit plus risk-clear coordination | M6.5; close the concrete ordering and acknowledgments before transformation runtime |
| Durable storage ownership, continuity, retention, presence, allocation, access and locking | M6.5; agree the lifecycle persistence boundary with M7 before approving the storage schema |
| Cleanup ambiguous completion and durable do-not-replay coordination | M7; receiving success or `completion_accepted` alone does not establish the durable boundary or authorize replay |
| Finalization obligations, Abandon non-destruction, discoverability, handoff and explicit discard | M7 behavior; M6.5 schema review must account for these obligations without shipping M7 operations early |
| Existing-operation recovery and diagnostic evidence | M6; reuse M3-M5 ownership, risk, publication and reconciliation rather than creating another recovery model |

Before M6.5 schema approval, its design must record which durable owner preserves
retention and non-destruction obligations, how M7 can publish and finish its
lifecycle obligations, and any required explicit compatibility/upgrade path.
This is a cross-stage review gate, not a selected schema or a new resource
registry. Exact M7 protocol coordination remains an M7 approval gate.

M6.5 implementation proceeds through format/validation, persistence and
allocation/access, Hook and continuity/Migration integration, then crash and
compatibility verification. It must not expose a destructive storage route that
bypasses the still-unimplemented M7 lifecycle.

## Evidence and remaining exclusions

M6.5 needs real ServiceStorage-backed resources proving Instance isolation,
identity independent of locator, conservative source-only retention, and loss
after possible target coherence but before Pactrun publication. The last case
must retain open risk and require manual recovery without replay or rollback.
Managed Input tests and mock resources do not certify this support.

M7 owns real Cleanup completion ambiguity, finalization-only retry, blocked
ordinary management during finalization, and abandonment/GC non-destruction
evidence. M6 cannot close those requirements before that runtime exists.

Docker volumes, external databases, remote objects, and the broader taxonomy of
non-ServiceStorage resources remain deferred and do not block this bounded
schedule. Snapshot deletion, public Rust APIs, stable machine-output envelopes,
and publication workflow changes are not added by this alignment.
