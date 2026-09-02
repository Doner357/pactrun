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
- The production crate remains a modular monolith. M2 Pack installation,
  Instance, and Managed Input binding support is integrated into `develop`.
- `SnapshotIntegrityFormatV1` is Frozen and merged into `develop` with Rust
  verification, an independent Node 24 oracle, golden vectors, and
  requirement/test traceability.
- `HookProtocolV1` is Frozen and merged into `develop` with Rust validation, an
  independent Node 24 valid-fixture oracle, cross-language fixtures, and
  requirement/test traceability. Production runtime integration remains
  deferred.
- The structured error taxonomy is Frozen and merged into `develop` with
  negative fixtures and requirement/test traceability.
- Runtime launcher integration under `PR-REQ-0194` remains pending until real
  Compiler, Admission, and Executor integration tests exist.
- The accepted `ServiceStorage` and ServiceStorage-backed Managed Service
  Resource direction corrects the earlier assumption that every Pactrun-visible
  persistent file is an Input. Its representation-independent semantic closure
  is integrated into the canonical `develop` baseline without changing Frozen
  V1 schemas or claiming production support.
- The non-identity metadata semantic closure and PersistenceSchemaV2
  implementation are integrated into the canonical `develop` baseline. The
  schema is the current implemented internal persistence schema while remaining
  non-Frozen and non-public.
- The Pre-M2 installation, Instance, and Managed Input binding design and M2
  implementation are integrated into the canonical `develop` baseline.
  Candidate `PackSourceYamlV1` remains non-Frozen; PersistenceSchemaV3 is the
  current implemented internal schema while remaining non-Frozen and
  non-public.

## Milestone states

- **Proposed:** ready for review but not approved for implementation.
- **Planned:** approved and ordered, but implementation has not started.
- **In progress:** work is active on an isolated feature branch.
- **Blocked:** a named semantic, technical, or environmental blocker prevents
  the completion gate from being met.
- **Complete:** every completion gate has passed and the result has been merged
  into `develop`.

## Design gates

These design-gate states describe semantic and documentation closure, not the
milestone state taxonomy above:

| Design gate | State |
| --- | --- |
| ServiceStorage architecture correction | **Closed.** |
| ServiceStorage semantic closure | **Closed.** |
| ServiceStorage representation and runtime | **Deferred.** |
| Cleanup completion/finalization coordination | **Deferred.** |
| Abandon non-destruction durable representation | **Deferred.** |
| Broader service-owned resource taxonomy | **Deferred.** |
| Non-identity metadata semantic and persistence closure | **Closed.** |
| M1-D non-identity metadata implementation | **Complete.** |
| Pre-M2 installation, Instance, and binding closure | **Closed.** |
| PackSourceYamlV1 authoring projection | **Closed.** |
| PersistenceSchemaV3 internal contract | **Closed.** |

The ServiceStorage architecture correction, ServiceStorage semantic closure,
and non-identity metadata persistence closure are integrated into the canonical
`develop` baseline. A later ServiceStorage representation or runtime design
does not reopen the closed ServiceStorage semantics unless review finds a
substantive conflict. M1-D is integrated against the current internal
PersistenceSchemaV2 contract.

The Pre-M2 closure and M2 implementation are integrated. M2 is `Complete`.
Candidate `PackSourceYamlV1` remains a non-Frozen authoring contract, and
PersistenceSchemaV3 is the current canonical implemented internal schema while
remaining non-Frozen and non-public.

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

**State: Complete.**

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

**State: Complete.**

Implement production Package, Revision, and Instance identities; opaque Instance
state versions; immutable runtime content; production `RevisionCoreV1`
projection and canonicalization; metadata associations; and the persistence
migration skeleton.

The completed M1-A slice implements production identity primitives, the two
Frozen Revision content semantic components, pure projection, strict canonical
codec, framing, digest, and Error Taxonomy conformance. M1-B through M1-D close
the remaining M1 persistence foundation. Authoring and installation are
implemented by M2; runtime launcher integration remains owned by M3.

M1-B implements the crate-private immutable runtime-content blob store against
an already-existing dedicated root. Durable creation and provisioning of that
root remains a later bootstrap or deployment obligation;
M1-B does not introduce CLI provisioning or claim a generic Windows durable
directory-creation mechanism. A later database reference may be published only
after M1-B has returned durable content-publication success.

The completed M1-C slice adds the first
independently versioned SQLite persistence schema for exact Revision identities,
the two canonical Revision content components, and the closure-derived
ContentId-to-blob reference index. Both the database and runtime-content roots
remain pre-provisioned. M1-C does not add installation, Instances, bindings,
backup UX, garbage collection, or generic repositories.

M1-D is the completed implementation slice for the closed non-identity metadata
contract. It implements exact textual values, reference-label bindings, current
presentation, typed provenance claims, and local Revision alias, note, and
trust state through the crate-private typed repository contract and exact
PersistenceSchemaV2. The authoritative design is linked from the
[Non-Identity Metadata Semantic Baseline](../architecture/non-identity-metadata-semantic-baseline.md)
and [Persistence Schema V2](../architecture/persistence-schema-v2.md).
M1-C MUST NOT be retrofitted with an opaque metadata schema.

M1-D implementation MUST preserve exact UTF-8 values and complete typed
comparators; implement semantic current-state CAS with the specified absence
and ABA limits; provide canonical physical representation for optional tuples
and current absence; state complete SQL ordering; and validate SQL/Domain
ordering parity. It MUST migrate exact V1 to exact V2 transactionally without
changing Revision identities, canonical Revision bytes, or runtime-content
references. The slice has no stable public Rust API, CLI, wire, or Export Bundle
scope.

`ServiceStorage` is not an M1-D metadata extension. M1-D MUST NOT persist a
storage or resource declaration or identity, live presence, service-owned
contents, locator or association, continuity, compatibility, retention,
discard, persistent Hook authority, operation prerequisite, Cleanup or storage-
finalization obligation, Abandon non-destruction obligation, or broader resource
taxonomy. It MUST NOT add a service-resource table, retained-resource registry,
or another operational durable representation.

M1 closes through these integrated implementation slices:

| Slice | Closed implementation requirements | Automated coverage |
| --- | --- | --- |
| M1-A | `PR-REQ-0225`–`PR-REQ-0227` | `PR-TEST-0039`–`PR-TEST-0046` |
| M1-B | `PR-REQ-0228`–`PR-REQ-0230` | `PR-TEST-0047`–`PR-TEST-0051` |
| M1-C | `PR-REQ-0231`–`PR-REQ-0234` | `PR-TEST-0052`–`PR-TEST-0057` |
| M1-D | `PR-REQ-0248`–`PR-REQ-0257` | `PR-TEST-0058`–`PR-TEST-0067` |

This aggregate completion closes the identity and persistence foundation, not
future operation implementations. Broad requirements whose remaining clauses
depend on Action execution, Snapshot Restore, Migration, recovery, or Instance
deletion retain their later milestone ownership and verification status.
`PR-REQ-0194` runtime launcher integration is explicitly an M3 completion
obligation and does not keep M1 open.

Completion gate:

- the production codec reproduces every Frozen Revision Core vector;
- identity is unchanged after persistence and reload;
- immutable content and atomic publication invariants have real tests;
- M1-D tests cover fresh V2, exact V1-to-V2 migration, crash/reopen and
  concurrent migration, schema-drift rejection, canonical optional tuples and
  current absence, the complete presentation target-by-field relation,
  deterministic SQL/Domain comparator parity, idempotency, CAS and its
  intentional ABA limitation, alias conflicts, deletion cascades, and reload
  equivalence;
- M1-D introduces no JSON/EAV metadata store, ServiceStorage representation, or
  operational-state backdoor;
- format verifier code is not silently promoted into a production persistence
  contract without an explicit implementation boundary;
- full remote CI passes.

### M2 - Packs, Instances, and managed bindings

**State: Complete.**

Implement minimal YAML authoring, Revision installation, incomplete Instance
creation, the single active/retained managed-binding registry, Input operations,
Secret protection and redaction, and Observe/Mutate guards.

This milestone covers Pactrun-authoritative Managed Input Bindings only. It
MUST NOT model a service-generated editable persistent file as an Input, an
Input rematerialization target, or an implicitly synchronized binding in order
to simulate `ServiceStorage`.

The approved implementation entry point is the
[Pre-M2 Installation, Instance, and Binding Baseline](../architecture/pre-m2-installation-instance-binding-baseline.md).
M2 uses Candidate `PackSourceYamlV1`, exact PersistenceSchemaV3,
file-backed operation-local staging below the dedicated Pactrun storage root,
strict token-first Instance CAS, and the fixed minimal human CLI. The source
frontend can project all Frozen `RevisionCoreV1` declarations but M2 does not
execute Action, Snapshot, Migration, or Cleanup capabilities.

Completion gate:

- Package Source can produce and install an exact immutable Revision;
- schema-directed plain/quoted string and Boolean projection, all-explicit-tag
  rejection, all-decimal Package IDs, and JSON-number-only numeric projection
  have negative and parity tests;
- every portable metadata union and presentation target, implicit installed-
  Revision targeting, and duplicate semantic-key rejection have tests;
- Windows and Linux exact-object source acquisition have no-follow,
  no-mount-crossing, alias, manifest, and TOCTOU tests;
- Instance creation and incomplete readiness are explicit;
- active and retained remain roles over one registry;
- payloads are chunked, bounded, random-identified, immutable, and tested across
  exact V1/V2-to-V3 migration and crash boundaries;
- export observes one exact payload without blocking Mutate and tests
  reclamation, atomic no-clobber file publication, and non-atomic stdout;
- strict state-version CAS and file-backed staging cleanup have positive and
  negative tests;
- persisted sticky Secret floors, Normal-to-Secret promotion, active-Normal
  with stored-Secret acceptance, active-Secret with stored-Normal corruption,
  retained effective protection, permitted-deletion continuity, structural
  redaction, and the explicit plaintext-at-rest limitation have positive and
  negative tests;
- the fixed M2 human CLI carries no installation-time local metadata, generic
  overwrite, stable machine envelope, or new Frozen error-code promise;
- Input and Secret behavior has positive and negative tests;
- no Action runtime behavior is claimed by this milestone.

### M3 - Action execution

**State: Proposed.**

Implement resolution, `InvokeAction` intent, typed sequential compilation,
Admission, stale-state checks, durable pins, Hook Runtime and Session authority,
I/O transitions, Executor behavior, and Run records.

Frozen `HookProtocolV1` has no persistent service-storage authority. M3 MUST NOT
reinterpret Workspace authority, pins, or `InstanceStateVersion` as authority
or linearization over service-owned live bytes.

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

M4 MUST NOT automatically scan `ServiceStorage` or treat every ServiceStorage-
backed Managed Service Resource as Snapshot content. Service recovery content
remains selected and transformed by Capture before commit.

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

The listed transitions and staged binding commit are Managed Input semantics.
M5 MUST NOT extend `Carry`, `Keep`, `Discard`, or `Declassify` into an invented
ServiceStorage-backed resource schema or claim a service filesystem/database
transaction is atomic with Pactrun persistence.

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

Recovery of a Pactrun-owned boundary MUST NOT be presented as rollback or proof
of coherence for service-owned live resources.

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

M7 MUST NOT infer deletion of a source-only ServiceStorage-backed Managed
Service Resource merely because a Revision stops naming it. Cleanup success is
not itself the durable do-not-replay boundary. An ambiguous completion does not
authorize replay, while a published Cleanup-completed boundary permits only
Pactrun-owned storage finalization. Abandonment does not authorize present or
later GC deletion of service-owned state; the durable representations remain
open.

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

## Deferred ServiceStorage representation and runtime gates

The representation-independent semantics are closed by PR-REQ-0235 through
PR-REQ-0248. The following work remains required before production
`ServiceStorage` or ServiceStorage-backed Managed Service Resource support can
be planned as an implementation milestone:

- a future Revision Core serialization for storage/resource declarations,
  identities, associations, locators, compatibility mappings, prerequisites,
  and user exposure/access;
- a future Hook Protocol authority only where a Hook requires
  protocol-mediated persistent access, independently versioned from Revision
  Core;
- a durable representation and persistence-ownership boundary for continuity,
  retention, explicit discard, and Abandon non-destruction obligations;
- protocol/runtime coordination for target commit plus risk clear and for the
  ambiguous window between Cleanup success submission and durable do-not-replay
  publication;
- storage allocation, presence observation, locking, storage-lifetime
  finalization, operator handoff, explicit discard, and the concrete runtime;
- a broader taxonomy deciding whether Docker volumes, external databases,
  remote objects, or other service-owned resources use related abstractions.

This list is a design gate, not a V2 schema, persistence design, CLI spelling,
authoring syntax, compatibility algorithm, transition union, orphan-storage
registry, or production milestone. Implementers MUST NOT bypass it by using
Managed Inputs or M1-D metadata as a live-file or operational-state mirror.
Hook-produced Managed Input or explicit ownership adoption may be designed
separately and is not expanded here.

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
