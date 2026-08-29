---
title: Implementation Roadmap
---

# Implementation Roadmap

**Status: Draft informative planning document. Non-normative.**

This page records the proposed implementation order and review gates for
Pactrun. It does not establish product semantics, change requirement status, or
replace the normative Developer documentation. If this roadmap conflicts with a
normative requirement, the normative requirement controls and the conflict must
return to design review.

Milestone state on this page is planning metadata. It must not be treated as
evidence that a requirement has automated coverage or that a format or protocol
is Frozen.

## Current baseline

- `RevisionCoreFormatV1` is Frozen with Rust verification, an independent Node
  24 oracle, golden vectors, and requirement/test traceability.
- The production crate remains a compile-oriented modular-monolith scaffold.
- `SnapshotIntegrityFormatV1` is Frozen and merged into `develop` with Rust
  verification, an independent Node 24 oracle, golden vectors, and
  requirement/test traceability.
- `HookProtocolV1` is Frozen and merged into `develop` with Rust validation, an
  independent Node 24 valid-fixture oracle, cross-language fixtures, and
  requirement/test traceability. Production runtime integration remains
  deferred.
- Phase 0 still requires a structured error taxonomy.
- Runtime launcher integration under `PR-REQ-0194` remains pending until real
  Compiler, Admission, and Executor integration tests exist.

## Milestone states

- **Proposed:** ready for review but not approved for implementation.
- **Planned:** approved and ordered, but implementation has not started.
- **In progress:** work is active on an isolated feature branch.
- **Blocked:** a named semantic, technical, or environmental blocker prevents
  the completion gate from being met.
- **Complete:** every completion gate has passed and the result has been merged
  into `develop`.

## Milestones

### M0-A - SnapshotIntegrityFormatV1

**State: Complete.**

Define the exact Snapshot integrity contract without implementing Snapshot
persistence or runtime behavior.

Scope:

- authoritative semantic manifest fields;
- `SnapshotId`, producer Revision, authoritative origin Instance, and capture
  time representations;
- complete active, retained, absent, and protected binding-state descriptors;
- logical service-content roles, paths, and digest descriptors;
- semantic normalization and collection ordering;
- RFC 8785 JCS preconditions;
- version markers, domain separation, framing, SHA-256, and digest spelling;
- positive and negative golden vectors, a Rust verifier, an independent Node 24
  oracle, and requirement/test traceability.

Completion gate:

- Candidate schema and normative documentation are complete;
- all identity-bearing choices are explicit;
- Rust verification, Node parity, RFC 8785 checks, and traceability pass;
- complete remote `cargo xtask ci` passes before and after a status-only Freeze;
- the Freeze changes no schema, canonical bytes, framing, vectors, or digests;
- unresolved Domain questions keep the format Candidate and are reported rather
  than guessed.

### M0-B - HookProtocolV1

**State: Complete.**

Define the language-neutral Hook Protocol message and state-machine skeleton.

Scope:

- transport and framing profile;
- exact version confirmation and unsupported-version behavior;
- request identifiers, acknowledgments, and ordering;
- Session establishment and authority handles;
- Action, Snapshot, Migration, and Cleanup contexts and typed outputs;
- recovery-risk entry and resolution;
- cancellation, timeout, EOF, and protocol violations;
- separation of the Hook Protocol channel from Pack-facing terminal I/O;
- cross-language fixtures and positive, negative, and state-transition tests.

Completion gate:

- protocol messages and state transitions form a closed typed contract;
- authority cannot be enlarged by a Hook request;
- failure and recovery-risk transitions are unambiguous;
- cross-language fixtures and traceability pass;
- runtime implementation remains out of scope until its own milestone.

### M0-C - Structured error taxonomy

**State: In progress.**

Define stable error categories and codes across format validation, semantic and
relational validation, resolution, compilation, admission, execution, Hook
Protocol, persistence, and recovery.

Completion gate:

- each code has one owned semantic boundary;
- machine-readable codes are separated from presentation text and diagnostics;
- multi-fault inputs do not accidentally establish a global first-error order;
- implemented mechanically verifiable errors have negative tests and
  traceability.

### M1 - Identity and persistence foundation

**State: Proposed.**

Implement production Package, Revision, and Instance identities; opaque Instance
state versions; immutable runtime content; production `RevisionCoreV1`
projection and canonicalization; metadata associations; and the persistence
migration skeleton.

Completion gate:

- the production codec reproduces every Frozen Revision Core vector;
- identity is unchanged after persistence and reload;
- immutable content and atomic publication invariants have real tests;
- format verifier code is not silently promoted into a production persistence
  contract without an explicit implementation boundary;
- full remote CI passes.

### M2 - Packs, Instances, and managed bindings

**State: Proposed.**

Implement minimal YAML authoring, Revision installation, incomplete Instance
creation, the single active/retained managed-binding registry, Input operations,
Secret protection and redaction, and Observe/Mutate guards.

Completion gate:

- Package Source can produce and install an exact immutable Revision;
- Instance creation and incomplete readiness are explicit;
- active and retained remain roles over one registry;
- Input and Secret behavior has positive and negative tests;
- no Action runtime behavior is claimed by this milestone.

### M3 - Action execution

**State: Proposed.**

Implement resolution, `InvokeAction` intent, typed sequential compilation,
Admission, stale-state checks, durable pins, Hook Runtime and Session authority,
I/O transitions, Executor behavior, and Run records.

This milestone owns the first real automated coverage for `PR-REQ-0194`. It must
test host launcher lookup, Plan binding, Admission revalidation, exact process
selection, and argument-tail delivery without claiming that `argv[0]` is a
Pack-facing contract.

Completion gate:

- native and interpreter Hooks execute through the same typed lifecycle;
- Compiler, Admission, and Executor responsibilities remain distinct;
- stale Plans fail before launch and accepted execution continuity is tested;
- Run creation, phase, outcome, cancellation, and I/O transitions are durable;
- runtime integration tests, not Revision Core vectors, verify launcher
  behavior.

### M4 - Snapshot lifecycle

**State: Proposed.**

Implement Capture binding-view consistency, `SnapshotCandidate` validation and
commit, complete managed binding state, canonical integrity, sensitive
export/import boundaries, and exact-compatible cross-Instance Restore.

Completion gate:

- runtime Snapshot integrity reproduces the Frozen integrity-format vectors;
- Capture observes the same pinned binding view that the committed Snapshot
  records;
- Restore uses staged Snapshot bindings and exact producer compatibility;
- admission, Secret handling, and failure cleanup have integration tests.

### M5 - Migration

**State: Proposed.**

Implement target-owned Migration graphs, chaining, normalized transitions,
typed active/retained source references, target requirements and outputs,
single-writer validation, staged targets, and per-edge commits.

Completion gate:

- intrinsic target validation and exact-source relational validation remain
  distinct;
- exact source semantics are required before an edge becomes executable;
- Admission checks actual binding existence against the single managed-binding
  registry;
- incomplete intermediate state and per-edge failure behavior are tested.

### M6 - Recovery

**State: Proposed.**

Implement execution ownership, recovery-risk entry and resolution, durable
self-sufficient recovery state, crash-boundary injection, orphan reconciliation,
and manual recovery.

Completion gate:

- crash tests cover every durable transition boundary;
- recovery does not depend on ephemeral process state;
- orphan ownership and reconciliation behavior are deterministic;
- normal execution cannot bypass an open recovery risk.

### M7 - Cleanup and deletion

**State: Proposed.**

Implement typed Cleanup requirements and context, the shared risk protocol,
cleanup-before-delete behavior, retry semantics, and explicit
`AbandonManagement`.

Completion gate:

- success, failure, cancellation, and open-risk outcomes have integration tests;
- deletion never silently skips required Package cleanup;
- retry does not introduce an unsupported deletion lifecycle taxonomy;
- abandonment skips Package code and records explicit operator intent.

### M8 - Recipes and advanced authoring

**State: Proposed.**

Implement the versioned language-neutral Authoring Contract, `InstallContext`,
network- and host-dependent generation, and `RevisionCandidate` output.

Completion gate:

- every frontend converges on the same normalized Candidate boundary;
- authoring metadata and environmental observations do not alter Frozen identity
  semantics unless the relevant format includes them;
- source-dependent generation has deterministic validation and diagnostics.

## Cross-cutting completion rules

Every milestone must:

- begin from the relevant canonical requirements rather than implementation
  behavior;
- use stable unique requirement and test IDs with resolvable bidirectional
  traceability, without requiring numeric contiguity;
- add tests at the level that proves the real contract, including negative,
  persistence, concurrency, crash, and integration behavior where required;
- report `Pending automated coverage` honestly until a real test artifact exists;
- stop on a Domain conflict or oracle mismatch instead of changing normative
  expectations to match one implementation;
- run local workspace-safe checks and the complete configured-remote
  `cargo xtask ci` before merge;
- report documentation impact and retained external resources;
- use an isolated `feature/*` branch based on `develop` and avoid unrelated
  workspace changes.

GitHub Pages publication and the existing untracked Pages workflow are outside
this roadmap unless separately approved.
