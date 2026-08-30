---
title: Implementation Guidance
---

# Implementation Guidance

**Status: Informative implementation guidance constrained by the normative
architecture.**

The Frozen V1 identity and wire domains remain closed for their existing scope,
and no known issue blocks prototypes that stay within that scope. The
representation-independent semantics for `ServiceStorage` and ServiceStorage-
backed Managed Service Resources are closed; their serialization, wire,
persistence, and runtime remain deferred. Implementers must not hide that work
by modeling service-owned live files as synchronized Inputs or M1-D metadata. If
a prototype shows that established invariants cannot coexist, the conflict must
return to design review rather than being hidden by another abstraction.

The M1-D non-identity metadata domain and persistence contract are also closed
for implementation. The
[Non-Identity Metadata Semantic Baseline](../architecture/non-identity-metadata-semantic-baseline.md)
is the navigation entry point; its linked requirement pages, including the
[Persistence Schema V2 Candidate](../architecture/persistence-schema-v2.md),
remain normative.

## Implementation decisions

Implementers may choose, without changing Pactrun semantics:

- SQLite, another KV mechanism, or a file model;
- database tables, indexes, repositories, and migration framework;
- storage directories and immutable-content layout;
- UUID and hash libraries;
- concrete `InstanceId` and `InstanceStateVersion` encodings;
- Rust crate, module, type, trait, and design-pattern names;
- PTY, ConPTY, Unix process-group, and Windows Job Object libraries;
- file locks, execution ownership, leases, and durable pin storage;
- persistence choices outside the exact M1-D Candidate schema;
- socket, named-pipe, or other side-channel transport;
- Hook Protocol framing and encoding;
- YAML and CLI parsers;
- compression, archive layout, blob storage, refcount, or mark-and-sweep GC;
- buffering, batching, caching, test, and mock libraries;
- exact machine-readable CLI schema spelling.

Those decisions must not change the canonical hash contracts, domain
identities, state transitions, authority boundaries, or user-visible
guarantees.

They also do not authorize an implementer to invent ServiceStorage declaration
serialization, authority wire shapes, continuity or retention persistence,
discard representation, access encoding, compatibility algorithms, Cleanup
coordination, Abandon non-destruction state, or a broader service-resource
taxonomy. Those representations and mechanisms require later formal design;
the closed semantics are summarized in
[ServiceStorage Semantic Baseline](../architecture/service-storage-semantic-baseline.md).

## Suggested implementation sequence

### Phase 0 - Canonical specifications and vectors

- freeze terminology and persistence-independent domain types;
- specify RevisionCoreV1 semantics and JCS/hash vectors;
- specify SnapshotIntegrityFormatV1 and vectors;
- define the HookProtocolV1 message and state-machine skeleton;
- establish a structured error taxonomy.

### Phase 1 - Identity and persistence

- Package and Revision identities;
- Instance identity and state version;
- immutable runtime content;
- RevisionCore projection and canonicalization;
- label, presentation, and provenance associations;
- persistence migration skeleton.

For M1-D, use only the closed typed metadata values, complete semantic
comparators, mutation batch, semantic current-state CAS, and exact Candidate
PersistenceSchemaV2. Authoritative strings retain exact UTF-8 bytes; optional
values order `Absent` before `Present`; every deterministic SQL query states a
complete `ORDER BY` that is parity-equivalent with the Domain comparator.
Current-state CAS deliberately does not detect history or ABA.

Do not replace the Candidate with a generic key/value, JSON, EAV, nullable
semantic tuple, serialized Rust object, or rowid-ordered repository. Do not
persist all-NULL presentation rows or nullable note/trust tombstones. Target
existence for presentation is checked against the exact strict-decoded
Revision Core, and every one of the four presentation fields is valid for every
closed `PresentationTargetV1` variant.

### Phase 2 - Packs, Instances, and bindings

- minimal YAML authoring;
- Revision installation and incomplete Instance creation;
- active and retained binding derivation;
- active and retained Input operations;
- Secret redaction and protection;
- Observe and Mutate guard.

This phase implements Pactrun-authoritative bindings only. It must not
materialize a Managed Input into a service file and synchronize it as a
substitute for `ServiceStorage`.

### Phase 3 - Action execution

- Resolution and InvokeAction intent;
- typed sequential compilation;
- stale-state admission and durable pins;
- Hook Runtime and Session authority;
- I/O transitions and Run records.

Workspace remains execution-scoped scratch. Persistent service-resource
authority is not part of Frozen Hook Protocol V1 or this phase.

### Phase 4 - Snapshots

- capture binding-view consistency;
- SnapshotCandidate validation and commit;
- complete managed binding state and canonical integrity;
- sensitive export/import;
- exact-compatible cross-Instance Restore.

Capture selects service recovery content; this phase does not automatically
Snapshot an entire future `ServiceStorage`.

### Phase 5 - Migration

- target-owned graph and chaining;
- explicit normalized transitions;
- typed active and retained references;
- requirements, outputs, and single-writer validation;
- staged targets, incomplete intermediate state, and per-edge commits.

These transitions govern Managed Input Bindings. They must not be generalized
into an unapproved Service Resource transition or persistence schema.

### Phase 6 - Recovery

- execution ownership;
- risk-entry and risk-resolution protocol;
- durable self-sufficient recovery state;
- crash-boundary injection tests;
- orphan reconciliation and manual recovery.

### Phase 7 - Cleanup and deletion

- typed Cleanup requirements and context;
- shared risk protocol;
- cleanup-before-delete behavior;
- durable Cleanup-completed finalization without inferred Hook replay;
- explicit AbandonManagement.

An ambiguous Cleanup completion before the durable do-not-replay boundary is a
future Hook Protocol and recovery coordination gate. After that boundary,
storage-finalization retry must not replay Cleanup. Abandonment must not make
service-owned state eligible for ordinary GC or unreferenced-storage cleanup.

### Phase 8 - Recipes and advanced authoring

- versioned language-neutral Authoring Contract;
- InstallContext;
- network- and host-dependent generation;
- Revision Candidate output.

## Specification status and open work

### SnapshotIntegrityFormatV1

The Frozen semantic manifest, binding and content descriptors, normalization,
JCS profile, framing, digest encoding, and golden vectors are defined by
[Snapshot Integrity Format V1](../package-contracts/snapshot-integrity-format-v1.md).
Production Snapshot persistence and runtime behavior remain Phase 4 work.

### HookProtocolV1

The Frozen transport, framing, exact version confirmation, request and
acknowledgment rules, authority handles, operation contexts, staged outputs,
recovery-risk state machine, cancellation, completion handshake, and protocol
errors are defined by
[Hook Protocol V1](../package-contracts/hook-protocol-v1.md). Production Hook
Runtime integration remains Phase 3 and later work.

### RevisionCoreV1 authoring spelling

The Frozen identity spelling, normalization, framing, and verification
boundary are defined by
[Revision Core Format V1](../package-contracts/revision-core-format-v1.md).
Production projection and persistence remain Phase 1 implementation work.

### Closed ServiceStorage semantics; deferred representation

Persistent Instance data has two relevant ownership domains: detached
Pactrun-authoritative Managed Input Bindings and ServiceStorage-backed,
service-authoritative Managed Service Resources. The service may create or
mutate live resource bytes without advancing `InstanceStateVersion`; Pactrun
does not implicitly synchronize, pin, content-address, or automatically
Snapshot those bytes. This closure does not classify non-ServiceStorage-backed
service-owned resources.

The domain semantics are closed in the linked normative requirements and
synthesized by the ServiceStorage Semantic Baseline. Before implementation,
formal design must still define Revision Core serialization, Hook authority wire
support where needed, durable continuity/retention/discard and Abandon non-
destruction representation, Cleanup completion coordination, and the concrete
runtime. Revision Core and Hook Protocol versions remain independent. No
authoring syntax, CLI, wire shape, storage table, retained-resource or orphan-
storage registry, compatibility algorithm, or broader resource taxonomy is
chosen here.

### Persistence and concurrency encoding

Choose state-version representation, atomic publication, execution ownership,
durable pins, recovery-state storage, checkpoints, and failure-injection points.
These choices do not reopen the exact M1-D schema, typed batch/CAS boundary, or
deterministic comparator contract. History-sensitive metadata concurrency,
metadata versions, and ABA detection remain deferred.

### CLI and structured output

Choose deterministic initial Input acquisition spelling separately from the
purpose-specific authorization spelling for sensitive Snapshot export, Secret
export and declassification, recovery override, and AbandonManagement. Also
choose completeness diagnostics and versioned machine-output spelling.

## Deferred beyond the initial product scope

- detailed Stack semantics;
- cross-Package adoption or replacement;
- operating-system sandbox, WASI, or containerized Hooks;
- HostAccessRequest and EffectiveIsolation implementation;
- registry, signing, and publisher trust selection;
- encrypted Snapshot export;
- advanced workflow profiles;
- richer conditional or optional Migration output contracts.
