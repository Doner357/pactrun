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

The M1-D non-identity metadata domain and persistence contract are implemented
against the current internal schema. The
[Non-Identity Metadata Semantic Baseline](../architecture/non-identity-metadata-semantic-baseline.md)
is the navigation entry point; its linked requirement pages, including the
[Persistence Schema V2](../architecture/persistence-schema-v2.md),
remain normative.

The Pre-M2 design is integrated into the canonical `develop` baseline and
synthesized by the
[Pre-M2 Installation, Instance, and Binding Baseline](../architecture/pre-m2-installation-instance-binding-baseline.md).
Its linked pages, including Candidate
[Pack Source YAML V1](../package-contracts/pack-source-yaml-v1.md) and the
implemented internal
[Persistence Schema V3](../architecture/persistence-schema-v3.md), own the
M2 contract. M2 is integrated into `develop`. The implemented internal
[Persistence Schema V4](../architecture/persistence-schema-v4.md) extends V3
with M3 Slice 2 Runs, durable pins, Action recovery state, ownership, and Run
Artifacts. The integrated M4 [Persistence Schema V5](../architecture/persistence-schema-v5.md)
preserves that contract and is now the current internal schema, still non-Frozen
and non-public.

The M3 scope and dependency review is complete and synthesized by the
[M3 Action Execution Approval Baseline](../architecture/m3-action-execution-approval-baseline.md).
M3 implementation is complete: Slices 1 through 6 are integrated into
`develop`, including the Slice 5 crate-private managed-output, Run finalization,
cleanup, owner-loss reconciliation, and inspection-data substrate, plus the
Slice 6 user-visible Run inspection, exact human spelling, and `PR-REQ-0097`
verification.

The integrated Slice 6 implementation closes that deferred human boundary
with exact Action/Plan/Run command spelling, a structural-only Run formatter,
read-only inspection and Plan opening, protected file/stdin parameter sources,
owner-held foreground cancellation, and explicit owner-loss/manual-recovery
operations. These additions remain crate-private and do not add a public API,
stable JSON envelope, V5 schema, or Frozen wire change; the result is integrated
into `develop`.

M4 is also complete and integrated: it adds the Snapshot lifecycle and human CLI,
Frozen integrity V2 current-writer/backward-reader support, bounded bundle V1,
exact V5 bootstrap/writable admission and the guarded Restore publication path.
See the [M4 execution record](./m4-implementation-status.md) for integration and
verification evidence. These additions do not implement Snapshot deletion,
ServiceStorage runtime, a public Rust API or stable machine-output envelopes.

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
- persistence choices outside the exact implemented V2, V3, and V4 internal
  schemas;
- socket, named-pipe, or other side-channel transport;
- Hook Protocol framing and encoding;
- YAML and CLI parser libraries that preserve the exact Candidate source and
  fixed minimal-CLI semantics;
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
comparators, mutation batch, semantic current-state CAS, and exact implemented
PersistenceSchemaV2. Authoritative strings retain exact UTF-8 bytes; optional
values order `Absent` before `Present`; every deterministic SQL query states a
complete `ORDER BY` that is parity-equivalent with the Domain comparator.
Current-state CAS deliberately does not detect history or ABA.

Do not replace the internal schema with a generic key/value, JSON, EAV, nullable
semantic tuple, serialized Rust object, or rowid-ordered repository. Do not
persist all-NULL presentation rows or nullable note/trust tombstones. Target
existence for presentation is checked against the exact strict-decoded
Revision Core, and every one of the four presentation fields is valid for every
closed `PresentationTargetV1` variant.

### Phase 2 - Packs, Instances, and bindings

- strict Candidate `PackSourceYamlV1` authoring with schema-directed scalars,
  closed portable metadata unions, explicit Package lineage, and Windows/Linux
  exact-object source-root acquisition;
- exact-object staging and hashing, followed by intrinsic validation, Frozen
  projection, and typed metadata-plan validation before durable M1-B blob
  publication and one atomic Revision/metadata database publication;
- exact PersistenceSchemaV3, including transactional V1/V2 migration;
- bounded-memory, file-backed staging below the dedicated Pactrun storage root;
- incomplete Instance creation and one chunked immutable payload registry;
- active and retained binding derivation and mutation;
- strict token-first state-version CAS and per-Instance Mutate exclusivity;
- Observe-only exact-payload export with atomic no-clobber file publication and
  explicitly non-atomic stdout;
- persisted sticky Secret floors, effective active/retained protection,
  structural redaction, and purpose-specific export authorization under an
  explicit plaintext-at-rest and no-secure-erasure limitation;
- the fixed minimal M2 human CLI with an empty installation local-metadata
  batch.

This phase implements Pactrun-authoritative bindings only. It must not
materialize a Managed Input into a service file and synchronize it as a
substitute for `ServiceStorage`.

It also must not add Action execution, durable pins, Run/recovery state,
Instance deletion, advanced authoring, stable machine APIs, new Frozen errors,
or cryptographic Secret storage. Authoring all Frozen capability declarations
does not claim their runtime implementation.

### Phase 3 - Action execution

- Resolution and InvokeAction intent;
- typed sequential compilation;
- stale-state admission and durable pins;
- Hook Runtime and Session authority;
- I/O transitions, Action outputs, and Run records;
- exact launcher revalidation and native/interpreter process execution;
- cancellation, timeout, EOF, protocol failure, and process-loss finalization;
- the minimum Action recovery substrate required for durable HookProtocolV1
  risk acknowledgments and open-risk consequences;
- crash/failure injection and end-to-end runtime integration;
- requirement-backed minimal invocation, plan, Run, sensitive-parameter,
  cancellation, and manual-recovery spelling before M3 completion.

On Windows, exact admitted Hook pathnames remain host-native (`Path`/`OsStr`/
`OsString`) across the interactive adapter and reach the native launcher
without UTF-8 conversion. An explicit `lpApplicationName` `CreateProcessW`
launch is not shell redirection; the Executor-side `GetBinaryTypeW` guard
rejects non-image batch-suffixed candidates while native PE names remain
eligible. The human `--plan` projection also preserves host-native launcher
pathnames: valid UTF-8 uses the existing readable terminal escaping, while
non-UTF-8 Windows and POSIX paths use explicit native UTF-16 and byte forms;
this is not a stable machine protocol.

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

- complete and generalize execution ownership beyond the Action substrate in
  Phase 3;
- complete risk-entry and risk-resolution handling for later managed-execution
  types and their commit boundaries;
- complete durable self-sufficient recovery state across those workflows;
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

### M3 Action execution

M3 is approved and ordered by the linked Action execution baseline. The
normative execution, Hook Protocol, Run, pin, recovery-risk, launcher, and
sensitive-data contracts are sufficient to begin bounded internal
implementation. Slice 5 now provides owner-held finalization, atomic eligible
Action output publication, execution-workspace cleanup, explicit confirmed
owner-loss reconciliation, and crate-private Run inspection/data access over
PersistenceSchemaV4. Slice 6 supplies the human projection and traceability for
`PR-REQ-0097`; the exact persistence encoding and process mechanisms remain
implementation choices. Exact new CLI spelling is closed by `PR-REQ-0284` and
its verification, satisfying the M3 human-interface completion gate.

### RevisionCoreV1 authoring spelling

The Frozen identity spelling, normalization, framing, and verification
boundary are defined by
[Revision Core Format V1](../package-contracts/revision-core-format-v1.md).
The Candidate M2 source spelling and projection into that unchanged boundary
are defined separately by PackSourceYamlV1. Raw Core JSON remains an internal
codec/conformance input rather than Package authoring.

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

M2 fixes random 128-bit state tokens, token-first compare-and-set, per-Instance
Mutate exclusivity, Observe coexistence, chunked payload persistence, and
operation-local export acquisition in PR-REQ-0264 through PR-REQ-0270. These
choices do not reopen the exact M1-D schema, typed metadata batch/CAS boundary,
or deterministic comparator contract. Execution ownership, durable pins,
recovery storage, checkpoints, history-sensitive metadata concurrency, metadata
versions, and metadata ABA detection remain later work.

### CLI and structured output

PR-REQ-0271 fixes the minimal M2 human spelling for Package ID generation,
installation, Instance create/list/show, and Input list/set/export/delete. It
has no install-time local-metadata options, force/overwrite export, or stable
machine envelope. Snapshot export and declassification, recovery override,
AbandonManagement, later lifecycle commands, completeness presentation details,
and a versioned machine-output format remain future work.

## Deferred beyond the initial product scope

- detailed Stack semantics;
- cross-Package adoption or replacement;
- operating-system sandbox, WASI, or containerized Hooks;
- HostAccessRequest and EffectiveIsolation implementation;
- registry, signing, and publisher trust selection;
- encrypted Snapshot export;
- advanced workflow profiles;
- richer conditional or optional Migration output contracts.
