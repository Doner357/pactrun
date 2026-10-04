---
title: Implementation Guidance
---

# Implementation Guidance

**Status: Informative implementation guidance constrained by the owning contracts.**

Start with the [current handoff](./next-milestone.md), select a [task reading path](./reading-paths.md),
and read its owning Spec rules. [User guides](../guides/index.md)
and [author guides](../package-authors/index.md) describe
available workflows. This page describes implementation discipline rather than
repeating milestone status or a second format-support matrix.

## Implementation decisions

Choose mechanisms that satisfy the approved contracts: private Rust module/API
organization, repository/query helpers, bounded buffering, test fixtures, parser
libraries, process supervision adapters and equivalent synchronization mechanisms.
Libraries implement mechanisms; Domain types retain policy, identity and lifecycle
ownership. Preserve the modular-monolith dependency direction and typed boundaries.

The following are already specified and are not unconstrained implementation
choices: identity/token encodings, canonical bytes, Hook framing and authority,
SQLite baseline shape, archive layout, machine-result schemas, ServiceStorage
continuity/retention, and Cleanup/abandonment publication. Read their owners before
changing a representation. New externally observable behavior or incompatible
representation requires the applicable design-change process.

## Identity, metadata and storage checks

- Acquire and hash the same safely opened source objects. Validate before durable
  publication; do not replace exact-object acquisition with path re-resolution.
- Use closed typed metadata values and complete semantic comparators. Preserve
  exact UTF-8 values, Absent-before-Present ordering, and explicit SQL ordering
  equivalent to Domain ordering. Current-state CAS does not promise history or
  ABA detection.
- Follow the [Persistence baseline](../spec/persistence/persistence-baseline.md).
  Do not substitute generic JSON/EAV storage, rowid ordering, all-NULL presentation
  rows or nullable note/trust tombstones for its contract. Check presentation
  targets against the strict-decoded Revision and preserve all four fields for
  each supported presentation target.
- Keep Input staging bounded and detached. Preserve token-first CAS, Secret
  floors, protection inheritance, no-clobber file export and explicitly non-atomic
  stdout export. Do not synchronize Inputs with live service files.
- Keep accepted Runs, pins, ownership and outcome publication distinct. No
  reconciler may fabricate an owner's successful publication or infer service repair.

## Execution and service checks

Keep admitted paths host-native through Windows process creation and interactive
adapters. An explicit `lpApplicationName` launch is not shell redirection. The
existing image guard rejects non-image batch-suffixed candidates while eligible
native images retain exact-path launch. Human Plan output preserves native
paths using terminal escaping or explicit native code-unit/byte forms; use the
machine contract for structured consumers.

Workspace is execution-scoped scratch. ServiceStorage is persistent,
service-authoritative state with explicit granted authority; Input bindings are
detached Pactrun-authoritative values. Service mutations do not automatically
advance Instance state tokens or become atomic with Pactrun database commits.

Use the [canonical Revision](../spec/contracts/revision-canonical.md),
[Hook protocol](../spec/contracts/hook-protocol.md),
[ServiceStorage execution](../spec/execution/m6-5-service-storage-execution.md),
and [retirement](../spec/execution/m7-instance-retirement.md) contracts for their
implemented representations and transitions. Keep the broader taxonomy for
non-ServiceStorage resources outside this scope.

Cleanup uncertainty, durable no-replay boundaries, abandonment custody and
explicit discard follow their owning lifecycle rules. A cleanup-finalization
retry must not replay a completed Cleanup Hook; ordinary GC does not authorize
destruction of abandoned service data.

## Verification route

Select checks using [development and verification policy](./development-and-verification.md).
Prove success, refusal, partial publication and output failure at the appropriate
level. Use source-qualified evidence; preserve requirement/test identities and
report skipped or reused checks explicitly. Read [writing and maintenance](./documentation-style.md)
when changing documentation.

## Historical sequence {#suggested-implementation-sequence}

The following retained anchors navigate the old sequence; they do not schedule
new work. The [captured guidance](./history/guidance-before-consistency-review-2026-09-28.md)
preserves its original context, including subsequently completed gates.

### Phase 0 - Canonical specifications and vectors

[Historical phase](./history/guidance-before-consistency-review-2026-09-28.md#phase-0---canonical-specifications-and-vectors).
Current formats are listed in the handoff and owned by Spec.

### Phase 1 - Identity and persistence

[Historical phase](./history/guidance-before-consistency-review-2026-09-28.md#phase-1---identity-and-persistence).
Use the current identity and Persistence contracts.

### Phase 2 - Packs, Instances, and bindings

[Historical phase](./history/guidance-before-consistency-review-2026-09-28.md#phase-2---packs-instances-and-bindings).
Use the current source, acquisition and Input contracts.

### Phase 3 - Action execution

[Historical phase](./history/guidance-before-consistency-review-2026-09-28.md#phase-3---action-execution).
Use the current execution and Hook contracts.

### Phase 4 - Snapshots

[Historical phase](./history/guidance-before-consistency-review-2026-09-28.md#phase-4---snapshots).
Use the current Snapshot integrity, bundle and operation contracts.

### Phase 5 - Migration

[Historical phase](./history/guidance-before-consistency-review-2026-09-28.md#phase-5---migration).
Use the current edge, execution and selector contracts.

### Phase 6 - Recovery

[Historical phase](./history/guidance-before-consistency-review-2026-09-28.md#phase-6---recovery).
Use the current recovery owner rather than replaying the old work order.

### Phase 6.5 - ServiceStorage

[Historical phase](./history/guidance-before-consistency-review-2026-09-28.md#phase-65---servicestorage).
ServiceStorage has implemented declaration, authority and execution contracts.

### Phase 7 - Cleanup and deletion

[Historical phase](./history/guidance-before-consistency-review-2026-09-28.md#phase-7---cleanup-and-deletion).
Retirement and retained-object lifecycle have their own current contracts.

### Removed phase: M8 Recipes

[M8 was rejected](./history/m8-recipes-rejected.md). The [historical phase](./history/guidance-before-consistency-review-2026-09-28.md#removed-phase-m8-recipes)
is retained for audit. It does not authorize a Recipe feature or public Candidate API.

## Contract lookup {#specification-status-and-open-work}

These legacy anchors now point to the effective owners, not obsolete reader support.

### Snapshot integrity {#snapshotintegrityformatv1}

[Snapshot integrity baseline](../spec/contracts/snapshot-integrity.md).

### Hook protocol {#hookprotocolv1}

[Hook protocol baseline](../spec/contracts/hook-protocol.md).

### Action execution {#m3-action-execution}

[Action behavior](../spec/behavior/actions-plans-and-runs.md) and
[execution boundaries](../spec/execution/execution-and-concurrency.md).

### Pack authoring {#revisioncorev1-authoring-spelling}

[Pack source](../spec/contracts/pack-source.md) and
[identity projection](../spec/contracts/revision-canonical.md). Raw canonical JSON
remains a codec/conformance input; it is not an alternate Pack source frontend.

### ServiceStorage contracts {#closed-servicestorage-semantics-deferred-representation}

[Revision declarations](../spec/contracts/revision-canonical.md),
[Session authority](../spec/contracts/hook-protocol.md), and
[service execution](../spec/execution/m6-5-service-storage-execution.md).

### Persistence and concurrency encoding

[Persistence baseline](../spec/persistence/persistence-baseline.md) and
[execution ownership](../spec/execution/execution-and-concurrency.md).

### CLI and structured output

[Human commands](../spec/behavior/command-and-output-reference.md),
[machine interface](../spec/contracts/cli-machine-interface.md), and
[object selectors](../spec/contracts/cli-id-selectors.md).

## Deferred beyond the initial product scope

Detailed Stack semantics, cross-Package adoption/replacement, OS/WASI/container
isolation, HostAccessRequest/EffectiveIsolation implementation, registry/signing
and publisher trust selection, encrypted Snapshot export, advanced workflow
profiles, and richer conditional/optional Migration outputs require separate
scope and design. Completion of ServiceStorage does not authorize these features.
