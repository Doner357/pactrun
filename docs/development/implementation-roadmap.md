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

## Development entry after Spec migration

Use the [Spec map](../spec/index.md), [task reading paths](./reading-paths.md),
and [current baseline / M6 handoff](./next-milestone.md) before starting the next
milestone. All numbered product rules now live under Spec. Design syntheses and
milestone records remain development guidance, not competing specifications.

The Spec reorganization did not change milestone states. The subsequent
2026-09-13 M5 approval is recorded in its milestone section and bounded baseline.
Usage guides remain a later documentation task and do not block specification-
driven development.

## Current baseline

- `RevisionCoreFormatV1` is Frozen with Rust verification, an independent Node
  24 oracle, golden vectors, and requirement/test traceability.
- The production crate remains a modular monolith. M2 Pack installation,
  Instance, and Managed Input binding support is integrated into `develop`.
  M3 Slice 1 typed resolution and side-effect-free Plan compilation are
  integrated into `develop`. M3 Slice 2 exact PersistenceSchemaV4 Runs,
  durable pins, Action recovery state, ownership, and Run Artifacts are
  integrated into `develop`. M3 Slice 3 transactional Run acceptance and
  Admission are integrated into `develop`. M3 Slice 4 Hook process launch,
  execution materialization, and the HookProtocolV1 runtime are integrated
  into `develop`; M3 Slices 5 and 6's finalization substrate and human CLI are
  also integrated into `develop`.
- `SnapshotIntegrityFormatV1` is Frozen and merged into `develop` with Rust
  verification, an independent Node 24 oracle, golden vectors, and
  requirement/test traceability.
- `HookProtocolV1` is Frozen and merged into `develop` with Rust validation, an
  independent Node 24 valid-fixture oracle, cross-language fixtures, and
  requirement/test traceability. The Action runtime is implemented in M3
  Slice 4; M4 Capture and Restore now reuse it in `develop`. Migration and Cleanup
  execution remain deferred.
- The structured error taxonomy is Frozen and merged into `develop` with
  negative fixtures and requirement/test traceability.
- Runtime launcher integration under `PR-REQ-0194` has Compiler, Admission,
  and Executor coverage. The M3 Slice 4 Windows correction is integrated into
  `develop` and implements native `CreateProcessW` launch with Job Object
  assignment before the primary thread resumes. Its Windows-only regression
  tests run locally; the configured POSIX CI host cannot execute that adapter.
- The accepted `ServiceStorage` and ServiceStorage-backed Managed Service
  Resource direction corrects the earlier assumption that every Pactrun-visible
  persistent file is an Input. Its representation-independent semantic closure
  is integrated into the canonical `develop` baseline without changing Frozen
  V1 schemas or claiming production support.
- The non-identity metadata semantic closure and M1-D implementation are
  integrated into the canonical `develop` baseline. M1-D introduced
  PersistenceSchemaV2 for that metadata contract.
- The Pre-M2 installation, Instance, and Managed Input binding design and M2
  implementation are integrated into the canonical `develop` baseline.
  M2 introduced PersistenceSchemaV3 for Instances and Managed Input bindings.
  Candidate `PackSourceYamlV1` remains non-Frozen.
- [PersistenceSchemaV5](../spec/persistence/persistence-schema-v5.md) is the integrated
  `develop` persistence baseline, with explicit writable admission and exact V4
  bootstrap. It preserves [PersistenceSchemaV4](../spec/persistence/persistence-schema-v4.md),
  introduced by M3 Slice 2, and the earlier contracts.
- M4 Snapshot lifecycle is `Complete`, with S0-S8 integrated into `develop`.
  Capture writes Frozen integrity V2; V1/V2 import, export, verification and
  exact-compatible Restore are available through the human CLI.
- The M3 Action execution scope and dependency review is complete. M3 is
  `Complete`, with Slices 1 through 6 integrated into `develop`, including the
  Action Hook runtime, Slice 5's crate-private managed-output/finalization
  substrate, and Slice 6's user-visible inspection and human CLI.

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
| M3 Action execution scope and dependency review | **Closed.** |
| PersistenceSchemaV4 internal contract | **Closed.** |
| M3 exact new CLI spelling | **Closed.** Defined by `PR-REQ-0284` and its verification. |

The ServiceStorage architecture correction, ServiceStorage semantic closure,
and non-identity metadata persistence closure are integrated into the canonical
`develop` baseline. A later ServiceStorage representation or runtime design
does not reopen the closed ServiceStorage semantics unless review finds a
substantive conflict. M1-D was integrated against PersistenceSchemaV2; its
metadata contract is preserved by the current PersistenceSchemaV6.

The Pre-M2 closure and M2 implementation are integrated. M2 is `Complete`.
Candidate `PackSourceYamlV1` remains a non-Frozen authoring contract. M2
introduced PersistenceSchemaV3; M3 Slice 2 subsequently integrated
PersistenceSchemaV4 as the implemented internal schema for that M3 stage.
PersistenceSchemaV5 was introduced by M4. M5's PersistenceSchemaV6 is now the
current implemented internal schema, with explicit exact-V5 upgrade only;
Declarative Migration execution is implemented; Hook and operator-input execution
remain pending. These contracts remain non-Frozen and non-public.

The M3 review is recorded in the
[M3 Action Execution Approval Baseline](./design-notes/m3-action-execution-approval-baseline.md).
M3 is `Complete`; the implementation remains bounded by that baseline and its
linked normative requirements.

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
[Non-Identity Metadata Semantic Baseline](./design-notes/non-identity-metadata-semantic-baseline.md)
and [Persistence Schema V2](../spec/persistence/persistence-schema-v2.md).
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
[Pre-M2 Installation, Instance, and Binding Baseline](./design-notes/pre-m2-installation-instance-binding-baseline.md).
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

**State: Complete.**

Implement resolution, `InvokeAction` intent, typed sequential compilation,
Admission, stale-state checks, durable pins, Hook Runtime and Session authority,
I/O transitions, Executor behavior, and Run records.

The approved scope and work ordering are synthesized by the
[M3 Action Execution Approval Baseline](./design-notes/m3-action-execution-approval-baseline.md).
Exact new CLI spelling was a required M3 closure item and is now closed by
`PR-REQ-0284` and its verification.

The completed M3 Slice 1 implements exact Instance and Action resolution,
`InvokeAction`, typed primitive parameter binding, side-effect-free immutable
Plan compilation, compile-time fact consistency, and exact interpreter launcher
selection. `PR-TEST-0080` and `PR-TEST-0081` provide automated coverage for
`PR-REQ-0273` and `PR-REQ-0274`. Admission, Run persistence, durable pins,
process launch, HookProtocolV1 runtime, output publication, recovery, and human
CLI spelling remain owned by later approved slices. `PR-REQ-0194` therefore
remains Pending automated coverage until its Admission and Executor clauses are
implemented and tested.

The completed M3 Slice 2 implements exact
[PersistenceSchemaV4](../spec/persistence/persistence-schema-v4.md): transactional
V1/V2/V3-to-V4 migration, durable Run records with a Running-only execution
owner selected by `PR-REQ-0277`, execution pins released only in the terminal
transaction, live and terminal recovery risk state, the `ManualRecoveryRequired`
Instance trust guard published atomically with a fresh state version, chunked
Run Artifacts, and the crate-private repository operations later slices
compose. `PR-TEST-0082` through `PR-TEST-0088` provide automated coverage for
`PR-REQ-0047`, `PR-REQ-0048`, `PR-REQ-0050`, `PR-REQ-0054`, `PR-REQ-0063`,
`PR-REQ-0065`, `PR-REQ-0066`, `PR-REQ-0069`, `PR-REQ-0071`, `PR-REQ-0078`,
`PR-REQ-0109`, and `PR-REQ-0275` through `PR-REQ-0277`. Run acceptance and
Admission composition (`PR-REQ-0042`, `PR-REQ-0043`, `PR-REQ-0049`), the
before-acknowledgment ordering of durable risk publication (`PR-REQ-0055`,
`PR-REQ-0056`), the runtime terminal consequence (`PR-REQ-0057`), cancellation
and failure ordering (`PR-REQ-0051`, `PR-REQ-0052`), owner-loss reconciliation
(`PR-REQ-0060` through `PR-REQ-0062`, `PR-REQ-0064`), and output publication
(`PR-REQ-0073`, `PR-REQ-0144`) are now composed by the Slice 5 crate-private
substrate. Run inspection and human spelling remain owned by Slice 6 and keep
their `Pending automated coverage` status where not closed by this slice.

The completed M3 Slice 3 composes acceptance and Admission over that substrate:
a compiled Plan becomes a durable Run before any check, and one Admission
`BEGIN IMMEDIATE` transaction evaluates the trust guard, stale compilation and
state facts (state version, exact references, readiness, runtime-content
availability, interpreter launcher re-selection), and Mutate exclusivity in the
normative precedence of `PR-REQ-0279`, publishing either the pins or the
`Failed` refusal outcome in that same transaction. `PR-REQ-0278` selects the
Mutate exclusivity model: the process-local mutation guard is held only for the
acceptance and Admission call, and only Running and Admitted Mutate Action Runs
conflict, with both access modes read from persisted Revision declarations.
The Frozen error taxonomy gained the appended codes
`admission.mutation_conflict` and `admission.recovery_guard_active` under
`PR-REQ-0222`. `PR-TEST-0089` through `PR-TEST-0092` provide automated coverage
for `PR-REQ-0039`, `PR-REQ-0041`, `PR-REQ-0042`, `PR-REQ-0043`, `PR-REQ-0045`,
`PR-REQ-0049`, `PR-REQ-0067`, `PR-REQ-0068`, `PR-REQ-0278`, and `PR-REQ-0279`.
`PR-TEST-0090` exercises the Action clause of `PR-REQ-0091` (readiness at
Admission) while that requirement remains `Pending automated coverage` for its
Capture, Migration, Cleanup, and Restore clauses. `PR-REQ-0194` remains Pending
until its Executor clause is implemented in Slice 4. Two known costs are
recorded: runtime-content verification re-hashes blobs under the SQLite write
lock during Admission, and a Mutate Run whose owner process is lost after
Admission holds the Instance's Mutate exclusivity until Slice 5 reconciliation
finishes it as `Interrupted`. Process launch, Workspace materialization,
HookProtocolV1 runtime is owned by Slice 4; output publication and owner-loss
reconciliation are composed by Slice 5; human spelling remains owned by Slice 6.

The completed M3 Slice 4 executes an admitted Action once. The execution owner
materializes `runtime/`, `workspace/`, `bindings/`, and `outputs/` beneath its
own `StagingSession` by copying pinned bytes rather than hard-linking or
re-resolving selectors, creates one owner-private Hook Protocol listener that
the Hook discovers through the `PR-REQ-0280` environment contract, launches the
exact admitted path without an implicit shell (POSIX process groups; Windows
`CreateProcessW` plus Job Object process-tree control), and drives the
production Frozen `HookProtocolV1` Action state machine over a stream that is
separate from the declared `none | output | interactive` terminal streams. One
outcome arbiter locks the first winning event (accepted completion, requested
cancellation, startup or action deadline, or premature Hook loss); a later
valid `complete` is still protocol-accepted but cannot override a locked
`Cancelled` or `TimedOut`; deadlines and the termination grace are
caller-supplied optional policy with no normative defaults; and pre-spawn
materialization, listener, or launch failure is `Failed`, never `TimedOut`.
Every post-Admission path leaves an owner-held continuation (`ReadyToFinalize`,
`RetryProcessControl`, `RetryDurableOperation`, or `RetryFinalization`) in a
volatile registry owned
by the live process, and terminal facts exist only after the supervised process
tree has been observed terminated. The execution tree is ephemeral owner state:
it is cleanup-eligible after confirmed owner loss and unpublished output slots
are never recovered from it. The Frozen error taxonomy gained the appended codes
`execution.session_materialization_failed`, `execution.launch_failed`,
`execution.protocol_transport_failed`, and
`execution.hook_reported_protocol_error` under `PR-REQ-0222`. `PR-TEST-0093`
through `PR-TEST-0100` provide automated coverage for `PR-REQ-0046`,
`PR-REQ-0052`, `PR-REQ-0055`, `PR-REQ-0056`, `PR-REQ-0060`, `PR-REQ-0098`,
`PR-REQ-0167` through `PR-REQ-0169`, `PR-REQ-0172`, and `PR-REQ-0280`; add
runtime coverage to `PR-REQ-0047` and to the Frozen protocol requirements
`PR-REQ-0205` through `PR-REQ-0208`, `PR-REQ-0210` through `PR-REQ-0212`, and
`PR-REQ-0215` through `PR-REQ-0217`; and complete `PR-REQ-0194` together with
the Compiler coverage of `PR-TEST-0081` and the Admission coverage of
`PR-TEST-0090`. `PR-TEST-0097` covers the request, propagate, observe
termination, finalize ordering and the `TimedOut` identity of `PR-REQ-0052`;
Slice 5 now supplies the durable publication of the resulting `Cancelled` or
`TimedOut` disposition, managed-output publication, owner-continuation
consumption, and explicit owner-loss reconciliation. User-visible Run
inspection and human spelling remain owned by Slice 6.

The completed M3 Slice 5 implementation composes the crate-private finalizer
over PersistenceSchemaV4. After observed process-tree termination it stages the
valid submitted output subset independently, attempts cleanup of only the
current Run execution tree, records cleanup or publication errors without
changing the established causal outcome, and publishes the Run terminal record
and Artifacts in one transaction. Persistence failures retain an owner-held
finalization retry; confirmed owner loss is reconciled explicitly by staging
lease observation and finishes valid orphaned Runs as `Interrupted` without
replaying or salvaging output slots. The internal substrate also exposes
ordered Run listing/loading and a single-snapshot Run-plus-current-recovery-guard
inspection substrate, artifact streaming, and independent artifact expiry; it
does not expose user-visible Run inspection.

`PR-TEST-0104`, `PR-TEST-0105`, `PR-TEST-0106`, `PR-TEST-0107`,
`PR-TEST-0109`, `PR-TEST-0110`, `PR-TEST-0111`, `PR-TEST-0112`, and
`PR-TEST-0113` provide focused coverage for the running-state matrix,
corruption fail-closed behavior, output subset publication, late completion,
publication failure ordering, cleanup residue, owner/session lease probing,
Action orphan reconciliation, and the production Hook text finalization
boundary. The additional inspection snapshot test is supporting coverage for
the crate-private substrate and has no user-visible inspection verification
edge. `PR-REQ-0097` is verified by `PR-TEST-0114`; that user-visible
projection is integrated into `develop` with Slice 6.

Slice 6 coverage includes `PR-TEST-0114`,
`PR-TEST-0115`, `PR-TEST-0116`, `PR-TEST-0117`, `PR-TEST-0118`,
`PR-TEST-0119`, `PR-TEST-0120`, `PR-TEST-0121`, `PR-TEST-0122`,
`PR-TEST-0123`, `PR-TEST-0124`, `PR-TEST-0125`, `PR-TEST-0126`,
`PR-TEST-0127`, `PR-TEST-0128`, `PR-TEST-0129`, `PR-TEST-0130`,
`PR-TEST-0131`, `PR-TEST-0132`, `PR-TEST-0133`, `PR-TEST-0134`,
`PR-TEST-0135`, `PR-TEST-0136`, `PR-TEST-0137`, and `PR-TEST-0138`: structural
human projection and command spelling, read-only Plan opening, exact
Ordinary/Protected parameter sources, pre-launch cancellation, explicit
recovery command boundaries, real POSIX interactive PTY behavior, real
Windows-console cancellation, and lower-level Windows interactive exact-launch
behavior. `PR-TEST-0139` covers the full production Action path through
launcher resolution, Admission re-selection, `ProcessSupervisor::spawn()`,
production adapter argv/`env::args_os()`, and the exact second launch.
`PR-TEST-0140` and `PR-TEST-0141` cover lossless Windows and Linux human Plan
pathname projections for non-UTF-8 host-native paths, including the read-only
no-Run/no-launch boundary. The `develop` baseline now includes this Slice 6
implementation.

M3 Slices 1 through 6 are closed and integrated into `develop`. Slice 5
completed Managed Action Output atomic publication, owner-held finalization and
retry, durable Run terminalization, Action owner-loss reconciliation, and the
crate-private Run/artifact/inspection substrate. Slice 6 completes the human
Action/Plan/Run and recovery boundary, including the `PR-REQ-0097`
verification. The overall M3 milestone is `Complete`.

The integrated Slice 6 implementation adds the bounded human surface without
changing the internal V4 encoding: `action list/show`, foreground `invoke`
with exact parameter-source handling, side-effect-free `--plan`, structural
Run inspection, explicit owner reconciliation, and guarded manual recovery.
Its read-only opening does not create a staging lease or perform migration,
cleanup, reconciliation, or payload acquisition. Ctrl+C is latched by the
invocation-scoped owner and is arbitrated at the final durable acceptance
commit; the arbiter is never held across Admission. An uncertain commit keeps
the candidate RunId for exact readback/retry, and the same live owner continues
cancellation, process termination, cleanup, and durable finalization after
acceptance. The CLI treats handler-install failure and every ordinary I/O or
retry diagnostic as non-authoritative to Run ownership.

The Windows launch correction uses the narrow `pactrun-windows-ntfs` adapter:
the exact admitted path is `lpApplicationName`, the primary thread starts
suspended, the kill-on-close Job Object is assigned, and only then does the
thread resume. Assignment or resume failure terminates and waits for the
suspended process. The adapter uses CRT-compatible argument quoting, the parent
environment with both protocol discovery variables replaced, and an explicit
standard-handle inheritance list. Its second launch of the real interactive
Hook reuses the same native process-creation primitive with the adapter's
existing Job Object, so no second containment boundary is introduced. It
inherits the current working directory. Process waits use
`WaitForSingleObject` and `GetExitCodeProcess`, including terminal exit code
259.

Windows passes the exact admitted path as `CreateProcessW`'s
`lpApplicationName`; that explicit native launch is not shell redirection. The
Executor uses `GetBinaryTypeW` as an explicit fail-closed guard for a
batch-suffixed path that is not a native image. A native PE named `.cmd` or
`.bat` still reaches exact native launch. This preserves the existing
no-implicit-shell policy without changing Compiler or Admission candidate
eligibility, bind file identity, or replacement detection behind an admitted
path.

`PR-TEST-0101` covers extensionless direct and interpreter images, competing
`.exe` siblings, batch-script refusal, and native images with batch suffixes.
`PR-TEST-0102` covers exact argument delivery (including quotes, trailing
backslashes, empty values, and Unicode), case-insensitive discovery replacement,
parent environment and working-directory inheritance, exit-code observation,
and all three terminal mappings. `PR-TEST-0103` starts a descendant before any
Hook handshake and proves that process-tree termination releases its live
exclusive handle. These tests supplement `PR-TEST-0095` and `PR-TEST-0097`.
They execute only on Windows; `PR-TEST-0125` additionally drives a real Windows
console event through an isolated console driver and a new Hook process group.
`PR-TEST-0134` through `PR-TEST-0138` exercise the interactive adapter's
lower-level second launch for a native PE, a non-image batch candidate, a
native PE with a batch suffix, a competing sibling/fallback candidate, and a
pathname that is not round-trippable through Rust UTF-8 `str`.
`PR-TEST-0139` exercises that pathname through the full production Action,
including launcher resolution, Admission re-selection, `ProcessSupervisor`,
the production Pactrun adapter argv and `env::args_os()` boundary, and the
exact second launch. The Linux real-PTY harness
exercises the corresponding POSIX foreground-TTY path. `PR-TEST-0140` checks
that Windows Plan output renders the exact UTF-16 pathname without U+FFFD, and
`PR-TEST-0141` checks the corresponding invalid-UTF-8 POSIX byte rendering;
both also verify that preview creates no Run, launch, or staging mutation.
Complete configured-
remote `cargo xtask ci` verifies the remaining POSIX path, traceability, Frozen
conformance, and documentation.

Frozen `HookProtocolV1` has no persistent service-storage authority. M3 MUST NOT
reinterpret Workspace authority, pins, or `InstanceStateVersion` as authority
or linearization over service-owned live bytes.

Because recovery-risk requests are mandatory HookProtocolV1 facilities, M3
also owns the minimum durable Action recovery slice needed to acknowledge those
requests safely: Action Run ownership, durable risk entry and resolution,
self-sufficient Action recovery state, confirmed-owner-loss finalization, the
open-risk `ManualRecoveryRequired` consequence, and retained recovery
references. M6 completes and generalizes recovery for later managed-execution
types; it does not postpone the safety prerequisite for executing V1 Actions.

This milestone owns the first real automated coverage for `PR-REQ-0194`. It must
test host launcher lookup, Plan binding, Admission revalidation, exact process
selection, and argument-tail delivery without claiming that `argv[0]` is a
Pack-facing contract.

Completion gate:

- native and interpreter Hooks execute through the same typed lifecycle;
- Compiler, Admission, and Executor responsibilities remain distinct;
- stale Plans fail before launch and accepted execution continuity is tested;
- Run creation, phase, outcome, cancellation, timeout, process loss, and I/O
  transitions are durable;
- accepted Runs retain exact Revision, runtime-content, Input, and Secret pins
  across current-binding replacement and process failure until every execution
  or recovery reference permits release;
- real HookProtocolV1 integration covers Session construction, binding and
  output authorities, completion, cancellation, EOF, protocol failure, and
  terminal-channel separation;
- durable risk acknowledgments and crash injection prove that open-risk failure
  or owner loss cannot be reported as safe and cannot replay the Hook;
- sensitive parameters, Secrets, and revealing derivatives do not enter Run
  history, ordinary diagnostics, or default terminal transcripts;
- exact M3 human spelling is requirement-backed before the milestone is marked
  complete;
- runtime integration tests, not Revision Core vectors, verify launcher
  behavior;
- no Snapshot, Migration, Cleanup, ServiceStorage authority, generic DAG,
  public Rust API, or stable machine-output contract is claimed.

### M4 - Snapshot lifecycle

**State: Complete.**

The approved [M4 baseline](../spec/execution/m4-snapshot-lifecycle-approval-baseline.md)
and [slice execution record](./m4-implementation-status.md) define the completed
scope. Implementation commit `5a113b069bdce74ecb96b45b83748575891c1735` was
integrated into `develop` by no-fast-forward merge
`9b1fa8fc781c1941b02ccc4786ea2085d00a004c`. S0 contracts and S1 production V1/V2
codecs are integrated; V2 passed its independent conformance/freeze gate.
S2 V5 persistence, writer admission, consequence counters and explicit legacy
bootstrap, and S3 Snapshot storage, bounded bundle handling and application
services are integrated as well. S4 includes read-only Snapshot
compilation, parameter/runtime qualification, typed acceptance, Capture/Restore
Admission, complete registry/strong Snapshot pins and dual tokens, and the shared
one-shot owner-continuation substrate with real-process arbitration/reconciliation
coverage. S5 implements the shared Frozen Capture protocol and managed Capture
execution/publication, including complete pins, bounded submitted-content
acquisition and atomic V2 results. Final-tree verification is reported with the
S5 delivery. S6 adds Frozen Restore content authority, staged bindings, complete
replacement and dual-token-guarded atomic success/guard resolution, with
final-tree verification reported at delivery. S7 wires the public Snapshot CLI
and operation-neutral Run inspection, with fresh-process V1/V2 service-content
journeys. S8 adds the bounded integration audit, legacy requirement traceability,
actual ZIP64 size/offset and lifecycle checks, and final platform validation.
The completion gate passed before integration; the final closeout tree is
verified again and recorded with delivery. V2 integrity, bundle V1, V5
persistence, and fixed capabilities retain independent versions.

Completed capabilities include Capture binding-view consistency,
`SnapshotCandidate` validation and commit, complete managed binding state,
canonical integrity, sensitive export/import boundaries, and exact-compatible
cross-Instance Restore.

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

**State: Implemented and integrated into develop.**

The [approved M5 baseline](./design-notes/m5-migration-implementation-baseline.md)
records the 2026-09-13 decisions. The [M5 closeout](./m5-implementation-status.md)
records the implementation scope, authorized local Git integration and final-tree
verification gates. Integration does not imply a remote push, release or deployment.

Implemented target-owned Migration graphs, chaining, normalized transitions,
typed active/retained source references, target requirements and outputs,
single-writer validation, staged targets, and per-edge commits.

The listed transitions and staged binding commit are Managed Input semantics.
M5 MUST NOT extend `Carry`, `Keep`, `Discard`, or `Declassify` into an invented
ServiceStorage-backed resource schema or claim a service filesystem/database
transaction is atomic with Pactrun persistence.

Acceptance gates, retained as regression requirements:

- intrinsic target validation and exact-source relational validation remain
  distinct;
- exact source semantics are required before an edge becomes executable;
- Admission checks actual binding existence against the single managed-binding
  registry;
- incomplete intermediate state and per-edge failure behavior are tested.

### M6 - Recovery

**State: Proposed.**

Complete and generalize execution ownership, recovery-risk entry and resolution,
durable self-sufficient recovery state, crash-boundary injection, orphan
reconciliation, and manual recovery across Snapshot, Migration, Restore,
Cleanup, and their commit boundaries. Reuse the Action recovery substrate that
M3 must implement for HookProtocolV1 rather than creating a parallel model.

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

GitHub Pages publication and deployment-workflow changes remain outside this
roadmap unless separately approved.
