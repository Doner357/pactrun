---
title: Implementation Guidance
---

# Implementation Guidance

**Status: Informative implementation guidance constrained by the normative
architecture.**

Core domain semantic closure is complete, with no known unresolved domain
semantics blocking a prototype. Open work concerns exact formats, wire
contracts, CLI spelling, persistence mechanisms, implementation, and tests. If
a prototype shows that established invariants cannot coexist, the conflict must
return to design review rather than being hidden by another abstraction.

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
- metadata-association persistence;
- socket, named-pipe, or other side-channel transport;
- Hook Protocol framing and encoding;
- YAML and CLI parsers;
- compression, archive layout, blob storage, refcount, or mark-and-sweep GC;
- buffering, batching, caching, test, and mock libraries;
- exact machine-readable CLI schema spelling.

Those decisions must not change the canonical hash contracts, domain
identities, state transitions, authority boundaries, or user-visible
guarantees.

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

### Phase 2 - Packs, Instances, and bindings

- minimal YAML authoring;
- Revision installation and incomplete Instance creation;
- active and retained binding derivation;
- active and retained Input operations;
- Secret redaction and protection;
- Observe and Mutate guard.

### Phase 3 - Action execution

- Resolution and InvokeAction intent;
- typed sequential compilation;
- stale-state admission and durable pins;
- Hook Runtime and Session authority;
- I/O transitions and Run records.

### Phase 4 - Snapshots

- capture binding-view consistency;
- SnapshotCandidate validation and commit;
- complete managed binding state and canonical integrity;
- sensitive export/import;
- exact-compatible cross-Instance Restore.

### Phase 5 - Migration

- target-owned graph and chaining;
- explicit normalized transitions;
- typed active and retained references;
- requirements, outputs, and single-writer validation;
- staged targets, incomplete intermediate state, and per-edge commits.

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
- retry semantics without deletion lifecycle states;
- explicit AbandonManagement.

### Phase 8 - Recipes and advanced authoring

- versioned language-neutral Authoring Contract;
- InstallContext;
- network- and host-dependent generation;
- Revision Candidate output.

## Specification work still open

### SnapshotIntegrityFormatV1

Define the exact semantic manifest fields, binding and content descriptors, JCS
preconditions, normalization, framing bytes, digest encoding, and golden
vectors.

### HookProtocolV1

Define transport and framing, negotiation, request IDs and acknowledgments,
authority handles, Migration and Cleanup contexts, staged outputs, recovery-risk
state machine, cancellation, EOF, and protocol errors.

### RevisionCoreV1 authoring spelling

Define stable Input identity representation, explicit transition schema,
Migration requirements and outputs, Cleanup requirements, and runtime-content
descriptors without changing their already established semantics.

### Persistence and concurrency encoding

Choose state-version representation, atomic publication, execution ownership,
durable pins, recovery-state storage, checkpoints, and failure-injection points.

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
