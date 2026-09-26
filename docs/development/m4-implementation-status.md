---
title: M4 Implementation Status
---

# M4 Implementation Status

**Status: Complete. The approved M4 scope is implemented, verified and integrated
into develop. This is a milestone closeout, not a release or Pages deployment.**

The approved work started from `b78c02310facd0f460f9099580051776a4ba5a77`
on `feature/m4-snapshot-lifecycle`. The user subsequently authorized M4 closeout
and Git integration. Implementation commit
`5a113b069bdce74ecb96b45b83748575891c1735` was integrated by no-fast-forward merge
`9b1fa8fc781c1941b02ccc4786ea2085d00a004c` on September 12, 2026. The retained
feature branch records the implementation; develop is the integrated baseline.
No push, release tag, feature-branch deletion or Pages deployment is included.

| Slice | State | Completion evidence |
| --- | --- | --- |
| S0 normative contracts, exact DDL, traceability | Complete; integrated into develop | Approved contracts; PR-TEST-0182/0183 execute target DDL without enabling V5 |
| S1 Snapshot domain and V2 conformance | Complete; integrated into develop; V2 Frozen | Typed manifests, versioned codecs, streamed content checks, 15 valid/32 invalid V2 vectors, independent Node 24 oracle, and ten production conformance tests |
| S2 V5, writable admission, legacy bootstrap | Complete; integrated into develop | PR-TEST-0195 through PR-TEST-0205 cover version gates, writer leases, multi-process TOCTOU, crash boundaries, orphan preservation, counters, operation kinds, and explicit storage upgrade |
| S3 Snapshot storage and bundle | Complete; integrated into develop | PR-TEST-0206 through PR-TEST-0219 cover bounded ZIP/ZIP64, exact closure, original-version round trips, private authorization/publication, rollback/crash, and real large payloads |
| S4 ManagedExecution substrate | Complete; integrated into develop | PR-TEST-0220 through PR-TEST-0241 cover read-only compilation, typed parameters/runtime qualification, real Capture/Restore Admission and pins, shared one-shot owner continuations, process races/crashes, and explicit reconciliation; Hook/result boundaries below |
| S5 Capture runtime | Complete; integrated into develop | PR-TEST-0242 through PR-TEST-0256 cover Frozen protocol, real Observe/Mutate Capture, pinned complete registry, submitted-source acquisition, atomic V2 publication, stable timestamp/identity on retry, greater-than-512-MiB content, risk acknowledgment and owner-loss/commit crashes |
| S6 Restore runtime | Complete; integrated into develop | PR-TEST-0257 through PR-TEST-0267 cover Frozen readonly authority, real V1/V2 Restore, complete replacement, dual-token conflicts, guard resolution, publication rollback, process loss/crashes and large service-content materialization |
| S7 human CLI | Complete; integrated into develop | PR-TEST-0268 through PR-TEST-0273 cover public V1/V2 journeys, exact command spelling, source/target separation, read-only planning/verification, redaction, live inspection/reconciliation and owner retention with broken diagnostics |
| S8 integration closeout | Complete; integrated into develop | PR-TEST-0274/0275/0276 strengthen public compatibility/capability/source-safety, actual captured-object lifetime and greater-than-u32 ZIP64 profile evidence; scope and platform report below |

Validation applies only to the exact tested workspace state. Pending runtime
coverage is not evidence of implementation. Keep requirements covering later
milestones partially pending; do not mark Snapshot deletion implemented.

PR-TEST-0178 through PR-TEST-0181 cover capability/source accounting;
PR-TEST-0182/0183 exercise the documented SQL in memory. S1 adds PR-TEST-0184
through PR-TEST-0194 for versioned production codecs, typed invariants, exact
canonical/digest parity, streamed content checks, parser preflight, safe
diagnostics, and the independent Node 24 oracle. These are not evidence of
Snapshot execution, ZIP/parser enforcement, or safe migration. Actual commands
and final exact-tree validation results are reported with delivery.

## Canonical contract map

- [M4 execution, Capture, Restore, and recovery](../spec/execution/m4-snapshot-lifecycle-approval-baseline.md): PR-REQ-0289 through PR-REQ-0291.
- [Stored-ZIP transport](../spec/contracts/snapshot-bundle.md): PR-REQ-0292.
- [Separate inclusive capabilities and acquisition accounting](../spec/behavior/m4-runtime-capabilities.md): PR-REQ-0293 and PR-REQ-0294.
- [Frozen integrity V2](../spec/contracts/snapshot-integrity.md): PR-REQ-0295 through PR-REQ-0297.
- [Exact V5, writable admission, and legacy bootstrap](../spec/persistence/persistence-baseline.md): PR-REQ-0298 through PR-REQ-0300.
- [Human command and verification contract](../spec/behavior/m4-snapshot-command-reference.md): PR-REQ-0301 and PR-REQ-0302.

## Required runtime traceability (partially automated)

| Contract | Existing authority | Required proof | Slices |
| --- | --- | --- | --- |
| Capture authored access; Restore fixed Mutate | PR-REQ-0191, PR-REQ-0045, PR-REQ-0278, PR-REQ-0279 | Observe Capture coexists; Mutate Capture competes with Mutate Action/Restore; Accepted-only occupies nothing; fake caller access cannot waive authoritative access; same/cross-process arbitration | S4, S5, S6, S7 |
| Capture readiness | PR-REQ-0091, PR-REQ-0191, PR-REQ-0031, PR-REQ-0090, PR-REQ-0100, PR-REQ-0150 | Missing required input refuses with actual launch counter zero; empty bound input is present; optional absence is recorded; format-level required absence never authorizes incomplete Capture | S4, S5, S7 |
| Restore content authority | PR-REQ-0211, PR-REQ-0213, PR-REQ-0102, PR-REQ-0151, PR-REQ-0200 | A real Restore Hook reads captured bytes using authoritative role/path/digest mapping; unrelated Snapshot sentinels and unsubmitted content are absent; readonly authority and locator separation; no ServiceStorage scan | S6, S7 |

Do not invent PR-TEST references for these pending scenarios. Assign stable
unique IDs when real tests are written, and update both directions. Format
vectors and mocked success codes do not substitute for runtime evidence.

Current runtime links: PR-TEST-0225/0231/0238 cover Capture readiness in Compiler
and Admission; PR-TEST-0227/0228/0236 cover authoritative access and actual
cross-process Capture/Action/Restore arbitration. PR-TEST-0234 counts calls at
the shared one-shot launch boundary and covers owner-held admission/finalization
retries and cancellation. Service-content Hook authority remains S6 evidence;
launch-seam tests do not pretend to implement a Frozen Snapshot Hook session.

## Ordered continuation gates

S1's Candidate full remote CI passed for source archive SHA-256
`2a4320170ae405537e580ed89d844d9b19bb33150a68f335e43cfad74764055d`.
The freeze changes the V2 corpus status only, not its data or expected outputs.
Post-freeze exact-tree CI is required and reported with delivery. The dedicated
entry point is `cargo xtask snapshot-integrity-v2-verify`; `cargo xtask ci`
also includes it without adding a public Rust API. The original Frozen V1
verifier, corpus, and oracle are unchanged.

S2 now routes production writable openings through exact V5 and durable writer
admission, and exposes only the already-approved storage upgrade maintenance
command. Snapshot CLI/runtime was not yet implemented at that checkpoint. The four historical
V1/V2/V3-to-V4 migration regression tests explicitly use the test-only legacy
V4 open protocol and still assert V4 results; they no longer accidentally ask
the M4 production binary to perform prohibited implicit upgrades. New S2 tests
independently prove that ordinary M4 opening refuses older schemas and that
only explicit exact-V4 bootstrap migrates them. Legacy fixture adapters cannot
perform V5-qualified writes. No production missing-admission fallback exists.

Pre-admission preparation now touches only the operation's own session;
publisher maintenance and abandoned-session cleanup wait for successful
admission. Every schema-dependent write revalidates the exact schema and its
durable admission under the write transaction. The existing PTY E2E driver's
readiness matcher now preserves bytes following a consumed marker, with a
coalesced-marker assertion; no production timeout or expected result is relaxed.
S2 does not implement Snapshot invocation, storage/ZIP services, or the Restore
dual-token commit itself.

S3 implements Snapshot-owned blob storage and Import/Export/Verify services,
with a bounded Stored-ZIP adapter and independent per-operation capabilities.
The general ZIP reader receives only a validated directory projection; the
original private archive and payload bytes remain unchanged. Export uses the
same private atomic no-clobber publisher as Managed Input export. Large-payload
and ZIP64 tests are real I/O tests, not only counter simulations. Snapshot CLI
dispatch was assigned to S7. Later S4-S8 sections supersede that checkpoint.

S4's first implementation checkpoint introduces typed managed operation
identities with no caller-authoritative access field. The common conflict
predicate resolves Action/Capture/Restore against exact persisted Revisions,
enumerates every Running-and-Admitted operation without an Action-only join,
and validates invocation/pin consistency. Production Action Admission uses
this predicate inside its existing serialized transaction. Shared owned
failure/interruption releases Snapshot pins and advances any open-risk
consequence atomically. The Action finalizer cannot publish bare Snapshot
Succeeded outcomes or Action Artifacts for Snapshot operations.

PR-TEST-0220 exercises 25 operation pairs and Accepted-only exclusion;
PR-TEST-0221 exercises actual Action Admission against Snapshot fixtures;
PR-TEST-0222 rejects malformed competitors without terminalizing the incoming
Run; PR-TEST-0223 checks owner mismatch, rejected bare success, interruption,
pin release and no repeated consequence. These are persistence tests, not
Snapshot Hook-launch or complete Snapshot Admission evidence.

S4's second checkpoint adds shared typed acceptance for all three operation
kinds, preserving cancellation/uncertain-commit handling, and operation-aware
Run loading with unchanged historical Action failed-step ranks. Capture uses
an owner-checked Admission transaction that independently checks required
bindings, exact active binding facts/protection, runtime closure, and launch
declaration. Admission pins all bound registry entries, including retained;
the read view derives active/retained/absence/protection from those pins and
the immutable Revision, never later current bindings. Refusal is durable and
creates no pins. Empty required payloads count as bound.

Explicit reconciliation now shares the managed Run path. Real subprocess tests
race Capture against Action, keep live owners untouched, kill the winner, and
prove Interrupted plus a single consequence with no result salvage. Unknown
leases remain untouched. Restore is representable at acceptance and inspection;
its admitted shape is validated for exact Snapshot/state/producer references,
but the ordinary Action/Capture Admission entry point refuses to qualify it.
It does not impose target readiness as a Restore prerequisite.

S4 now completes the remaining substrate work:

- A read-only Compiler observes one database read snapshot, binds the selected
  Capture/Restore parameter schema with the shared M3 binder, checks Capture
  readiness, validates supported runtime/protocol and ordered launcher selection,
  and emits a private immutable Snapshot Plan without a lease or Run side effect.
- Restore qualification verifies exactly the selected Snapshot, original-version
  producer/content rules, runtime capability and sticky/removal transitions.
  Admission repeats these checks under the serialized boundary, then atomically
  establishes its Revision pin, Snapshot strong pin and both exact tokens.
  It never requires target pre-Restore completeness or clears a guard.
- Snapshot owners use the existing continuation registry, cancellation arbiter
  and process-supervisor handoff seam. Admission/finalization infrastructure
  failures retain the same Run and owner; already committed terminal outcomes
  are observed, not rewritten. A claimed launch cannot be acquired or replayed
  again, including after callback failure or an abandoned claim. Real process
  races/crashes exercise the shared durable admission and reconciliation paths.

S4's completion boundary is the qualified, one-shot execution claim and the
existing process-supervisor/continuation handoff, not an invented successful
Snapshot. S5/S6 attach actual Frozen Snapshot sessions, materialization, candidate
handling and atomic success publication; S7 attaches human CLI commands. The
generic finalizer still refuses Snapshot Succeeded without the dedicated result
publisher. These are the original slice boundaries, not deferred S4 work or a
new locking/recovery authority. Final acceptance and the subsequently authorized
integration into develop have now completed; the milestone is Complete.

1. S0: review the exact DDL and normative links; verify traceability and docs.
2. S1: typed V1/V2 production manifests/codecs, strict parsing, independent
   Node V2 oracle, positive/negative vectors, and a separate freeze gate.
   Capability counter tests alone do not finish this slice.
3. S2: implement exact V5, current-writer admission, consequence versions,
   and exact V4 bootstrap. Prove late-opening legacy writers with real processes.
4. S3: bounded ZIP preflight before untrusted allocation, Snapshot-owned
   chunk storage, import/export/verify, idempotency, and private no-clobber output.
5. S4: operation-specific access and readiness, exact pins, lease ownership,
   durable risk, atomic terminalization, and explicit no-replay reconciliation.
6. S5: true Observe/Mutate Capture, pinned complete registry, actual acquisition
   limits, one-time captured_at, candidate validation, and V2-only publication.
7. S6: staged bindings plus selected readonly service content, transition
   checks, dual-token conflict guard, atomic replacement and guard resolution.
8. S7: fresh-process CLI journeys, structural inspection, explicit verification,
   sensitive authorization, timeouts, and storage upgrade without scope expansion.
9. S8: all required Windows/POSIX integration, crash, source-closure, capacity,
   privacy, large-blob and ZIP64 cases; final exact-tree CI and documentation.

## Validation and delivery rules

### S5 protocol checkpoint

The shared HookProtocolV1 implementation now has explicit Action and Capture
decoder/Session modes. The owner chooses the mode before the peer stream is
read; a Hook cannot select another operation with its completion field. The
Capture completion contains typed role/path/candidate-path submissions, no
caller-authoritative digest. Logical role/path pairs are unique and ordered
independently of Session locators; distinct pairs may share one candidate file.
Failure submissions must have empty service content. Risk requests, pending
request exclusion, cancellation controls and terminal state rules reuse the
existing state machine without changing Frozen wire/schema semantics.

Outbound serialization now enforces the existing inclusive 16 MiB frame limit
while building its buffer, rather than allocating an unbounded payload before
checking. A rejected Session emits neither preamble nor a partial frame. Buffer
allocation failure retains its host-resource error classification. The inbound
Capture path retains the same pre-allocation frame-length check as Action.

PR-TEST-0242 consumes all three valid and eleven invalid existing Frozen Capture
transcripts unchanged. PR-TEST-0243 adds operation-isolation, semantic-set and
risk-ordering checks. PR-TEST-0244 covers limit-minus-one/limit/limit-plus-one,
escaped byte lengths and zero partial output; PR-TEST-0245 uses a real supervised
process and private platform transport to submit candidate content and receive
completion_accepted. These are protocol-level tests, not a managed Run success.

### S5 managed Capture implementation

The S4 claim now launches Capture through the existing process supervisor,
private protocol transport, cancellation/deadline arbitration and owner retry
registry. The exact admitted Revision supplies Observe/Mutate access. Compiler
and Admission readiness checks remain independent; only active bindings are
materialized for the Hook, from the same complete pinned registry used to form
the Snapshot. Retained bindings, empty bound payloads and optional absence are
preserved without granting retained-binding authority to the Hook.

Only submitted candidate paths are opened relative to the pre-opened candidate
root, using the shared opened-object acquisition mechanism. Symlinks/reparse
traversal and nonregular files are refused. Managed immutable source identities
and candidate source paths drive distinct-source accounting; hardlink aliases
do not reduce acquisition charges. Snapshot-scoped digest equality deduplicates
stored bytes, not logical acquisition. Streaming and independent service,
acquisition and storage budgets do not reuse the Managed Input limit.

Successful completion samples captured_at once before acknowledgment. The live
owner retains that instant, SnapshotId and private staged bytes across terminal
transaction retries. The dedicated V2 publisher validates exact pins and payload
closure and atomically inserts Snapshot/chunks/result reference with Succeeded
and pin release. It does not advance InstanceStateVersion or create Artifacts.
Failed, cancelled, timed-out or interrupted Capture never publishes a candidate.
No persisted PreparedCommit or reconciler publication authority was introduced.

| Runtime evidence | Test IDs |
| --- | --- |
| Real Observe/Mutate Session, immutable admitted view, redaction and atomic result | PR-TEST-0246 |
| One-time clock/identity, rollback retry and hardlink source accounting | PR-TEST-0247 |
| Failure, open risk, invalid/missing/nonregular candidates, cancellation and timeout | PR-TEST-0248 |
| Retained Secret, empty required binding, active-only Hook view | PR-TEST-0249 |
| Intermediate POSIX symlink / native Windows junction refusal | PR-TEST-0250 |
| Actual 512 MiB + 1 byte service capture, chunk storage and full verification | PR-TEST-0251 |
| Live owner exclusion, owner loss and before/after terminal commit crashes | PR-TEST-0252 |
| Durable risk retry acknowledgment without Hook replay | PR-TEST-0253 |
| Pre-epoch UTC clock normalization | PR-TEST-0254 |
| Staged-content corruption after rollback cannot publish a Snapshot/result | PR-TEST-0255 |
| Linux FIFO candidate refusal without a blocking open | PR-TEST-0256 |

Frozen semantic text, schema and vectors are unchanged; verification metadata
links the new runtime evidence. S6 Restore execution follows below; S7 human
Snapshot CLI and the integration audit are covered by S7/S8 below. S5 alone does not claim an end-to-end CLI journey
or complete M4. No Pages deployment is part of this work.

### S6 Restore implementation

Restore uses the same one-shot Snapshot execution claim, process supervisor,
private transport, risk acknowledgment and terminal-owner retry machinery as
Capture. The owner selects the Frozen Restore decoder before reading peer data;
Restore completion cannot submit Capture content or Action outputs. Restore
remains Mutate and does not acquire a blanket target-readiness requirement.

Before launch, materialization revalidates the admitted Snapshot closure and
exposes active Snapshot bindings, never pre-Restore target bytes or retained
binding authorities. The separate readonly snapshot_content root contains only
the selected Snapshot's logical service descriptors and matching immutable
bytes. Portable materialized paths are generated Session locators, distinct
from logical role/path/digest mappings. The implementation does not enumerate
other Snapshots or scan ServiceStorage and is not an OS sandbox for native Hooks.

The successful terminal transaction compares both admitted tokens before any
replacement. A mismatch terminalizes Failed without clearing the guard, changing
bindings, refreshing qualification or replaying the Hook. Otherwise it validates
the selected closure, copies Snapshot chunks into fresh target-scoped payload
identities, replaces all bindings (including retained/removal semantics),
publishes one fresh InstanceStateVersion, clears the existing guard and commits
Succeeded together. Old payloads still pinned by other executions are retained.
The V5 DDL and ranks are unchanged; no durable PreparedCommit was introduced.

| Runtime evidence | Test IDs |
| --- | --- |
| Frozen Restore transcript and rejection of foreign/output completion authority | PR-TEST-0257 |
| Real Capture-to-Restore, cross-Instance identity preservation, selected content and replacement | PR-TEST-0258 |
| State drift, value ABA, consequence-only drift and success-only guard clearing | PR-TEST-0259 |
| Failure, cancellation, timeout, open risk and invalid protocol preserve the old target/guard | PR-TEST-0260 |
| V1/V2 readers, retained/removal semantics, unrelated-content exclusion and incomplete target | PR-TEST-0261 |
| Sticky downgrade and required-bound removal refuse before launch | PR-TEST-0262 |
| Transaction rollback preserves target, guard and strong pin; retry launches no new Hook | PR-TEST-0263 |
| Live-owner exclusion, explicit owner-loss reconciliation and before/after commit crashes | PR-TEST-0264 |
| Corrupt selected content refuses materialization with no Hook launch | PR-TEST-0265 |
| Actual service content above 512 MiB, repeated descriptor expansion and streamed Hook reads | PR-TEST-0266 |
| A separate process changes either publication token after Hook completion without replay | PR-TEST-0267 |

The shared S4 admission matrix remains the access/conflict/readiness evidence;
S5 Action/Capture regressions remain required. S6 adds no CLI spelling, format
version, declassification authority, migration behavior or ServiceStorage runtime.

### S7 implementation and self-review

Snapshot capture/restore/import/export/verify/list/show are wired to the existing
application services. Parameter/deadline parsing is shared with invoke without
renaming invoke's action-timeout-ms option. Capture/Restore use an owner-held
progress loop: acceptance/admission/runtime retries retain the same Run and the
one-shot claim prevents replay. Output failure cannot abandon a running owner.
Read-only commands and plans open no writable session and perform no implicit
migration or reconciliation. Run list/show now use coherent operation-neutral
reads while preserving the Action projection for Action records.

Self-review checked the closed CLI forms, authoritative access/readiness,
parameter-source ordering, publication/verification distinctions, retry lifetime
and sensitive output. Two test assumptions were corrected against the baseline,
not by changing product semantics: Snapshot Runs are not Action projections;
and declared-sensitive ordinary parameter text remains allowed with effective
redaction, while Protected sources also force redaction. Snapshot diagnostics
withhold source paths/raw input and distinguish capability, integrity, bundle
profile and host I/O failures. Historical relational verification is explicitly
not_recorded because V5 does not persist that history.

The fresh-process journeys consume captured service bytes in Restore, include
producer-unavailable import/verify, preserve V1/V2 canonical representations,
and exercise no-clobber export, exact IDs, required readiness, read-only plans,
timeouts, live-owner inspection and explicit reconciliation. The separate CLI
adapter test injects durable retries with broken diagnostic output and proves
one successful Capture and one completion-time sample. These checks do not
implement Snapshot delete, a machine JSON API or ServiceStorage runtime.

S7 self-review and full CI passed before S8 changes began. Its source archive
SHA-256 was `91a70893bf6482313db01d54a139de3737e299e302238e2ebc00d2d802a15387`.
Native Windows suites initially overlapped and produced timing failures plus a
sharing violation. The unchanged tree was then rerun one suite at a time:
225 library tests and 43 system tests passed without changing deadlines or
expected results. POSIX cargo xtask ci, including docs, also passed. Final native
verification continues to run suites sequentially. Neither slice authorizes
Git integration or Pages deployment.

### S8 bounded audit and self-review

The audit did not reopen the approved architecture. It reviewed version dispatch,
exact compatibility, access/readiness authority, whole-state replacement,
dual-token publication, recovery/owner loss, migration closure, independent
capacity profiles, hostile containers and ordinary-output privacy against the
product requirements rather than merely counting test exits.

Bounded corrections and additional evidence:

- Unix bundle input is opened nonblocking and qualified using the opened object,
  so a FIFO source cannot hold a writable CLI owner waiting for a writer. The
  Windows regular-file checks remain in place. This changes no bundle profile.
- Public CLI tests distinguish incomplete Capture, exact-incompatible Restore,
  capability-limited verification and invalid source types, with no Hook launch
  or partial Snapshot publication. Protected stdin redaction is explicit.
- A real successful Capture is followed by foreign-key-valid removal of its
  creator Run, origin Instance and producer installation in an isolated storage
  test. Its Snapshot bytes/identity remain intact and producer-relative checks
  become not_evaluated. No deletion CLI was added for this test.
- A real file beyond 4 GiB exercises ZIP64 member sizes and directory offsets,
  including rejection of a forged overflowing locator before data allocation.
  Its sparse/zero payload is transport-profile evidence, not a claim of full
  multi-GiB Snapshot verification. PR-TEST-0215/0251/0266 separately read/hash and
  store/materialize actual service bytes above 512 MiB; PR-TEST-0216 validates an
  actual 65,536-blob ZIP64 entry-count boundary through the full bundle verifier.
- Existing tests are linked back to the original Snapshot requirements instead
  of leaving covered behavior attributed only to the M4 approval supplement.

| Contract group | Principal evidence |
| --- | --- |
| Original V1 and current-writer V2 identity/normalization | PR-TEST-0184/0185/0186/0192/0193; unchanged Frozen verifiers and independent Node oracle |
| Capture is a lifecycle capability, preserves pinned complete bindings and has no Artifact prerequisite | PR-TEST-0246/0247/0249/0252; shared S4 authority/readiness matrix |
| Restore exact compatibility, staged bindings and selected service content | PR-TEST-0258/0261/0262/0268/0269/0274 |
| Snapshot identity/lifetime and sensitive export | PR-TEST-0208/0213/0268/0275 |
| Atomic publication, guard resolution and no replay | PR-TEST-0252/0255/0259/0260/0263/0264/0267/0273 |
| V5 exact schema, writer-admission/bootstrap TOCTOU closure | PR-TEST-0195 through PR-TEST-0205 and S4/S6 persistence regressions |
| Separate fixed limits, streaming, ZIP/ZIP64 and source boundaries | PR-TEST-0178/0179/0181/0207/0215/0216/0217/0250/0251/0256/0266/0274/0276 |
| Human CLI and safe read-only projections | PR-TEST-0268 through PR-TEST-0274; existing Action/system regressions |

Self-review found no need for a new protocol version, V5 DDL change, locking or
recovery framework, PreparedCommit, quota/bypass switch or cross-resource dedup
ownership. Frozen semantic text, schemas and expected vectors remain unchanged;
verification metadata alone links the additional evidence. Final-tree full CI
and native Windows results are recorded with the delivery archive, not inferred
from the earlier S7 checkpoint.

### Scope and completion accounting

The approved M4 implementation covers Capture, Import, Export, Verify, inspection,
exact-compatible Restore, explicit reconciliation and storage upgrade. It does
not implement Snapshot delete; Revision deployment; ServiceStorage runtime or
automatic scanning; Migration/Cleanup execution; background reconciliation;
workflow replay/resume; a stable JSON CLI/public Rust API; or Pages deployment.
PR-REQ-0099 and PR-REQ-0117 have M4-subclause coverage only: their delete clauses
remain deferred. PR-REQ-0239's M4 selection/no-scan rule is covered without
claiming a ServiceStorage resource execution implementation. Other later-stage
requirements remain pending, not silently completed by this acceptance record.

The user authorized the previously pending local Git integration. M4 code,
specifications, vectors, traceability and tests are committed and merged into
develop. The implementation feature branch is retained. No Git remote is
configured, and no push, release or public documentation deployment was made.

### Git integration and acceptance ledger

| Item | Result |
| --- | --- |
| Pre-M4 develop baseline | `b78c02310facd0f460f9099580051776a4ba5a77` |
| Implementation commit | `5a113b069bdce74ecb96b45b83748575891c1735` (`feat(snapshot): implement M4 snapshot lifecycle`) |
| Integration merge | `9b1fa8fc781c1941b02ccc4786ea2085d00a004c` (`--no-ff`, no conflicts) |
| Integrated implementation tree | `24ed7d1c957efb914d639d5e6f509d6076e7a883`, exactly equal to the feature tree |
| Pre-integration verified source | SHA-256 `f265a4b22b0b8ac7764cc92f93e552935ae3059b36d7a7d69cbf28250a9c4447`; all 165 source files matched before staging |
| Reviewed implementation write set | 79 M4 files; unrelated workflow/archive and local agent/SSH state excluded |
| Native Windows acceptance | Passed: 227 library tests and 44 system tests, suites run sequentially with four outer test threads; case-internal concurrency retained |
| POSIX acceptance | Passed: full cargo xtask ci, including 222 library tests, 43 system tests, all workspace/Frozen/oracle/traceability checks, documentation typechecking and production build |
| Closeout document tree | Reverified before its final documentation commit; exact source/archive and results are reported with delivery |
| Remote push, tag, Pages deployment | Not run; not part of the authorized local integration |

The integration introduced no merge-resolution code changes. This closeout
updates milestone/current-baseline metadata, preserves the corrected S5 test
range through PR-TEST-0256 and the closed M3 CLI-spelling gate, and aligns the M3
heading with its already-Complete baseline. Normative requirements, Frozen
schemas/vectors and runtime behavior are unchanged by the closeout edits.

### Final-tree verification

Use the configured persistent remote workspace for full cargo xtask ci. It
includes all Frozen verifiers, Rust format/Clippy/tests, traceability, website
typechecking and production docs build. Add V2 conformance without weakening
any V1 checks. Native Windows process/ACL/atomic-publication tests remain a
separate required gate. Counter limit-1/limit/limit+1 tests are not evidence
of streaming or large-object persistence; the completed real greater-than-512-MiB
service-content and ZIP64 tests are identified in the S8 evidence table above.

Stop on a Frozen conflict, oracle mismatch, inability to prove writer/migration
TOCTOU closure, unbounded ZIP allocation, unauthorized new semantics, or missing
mandatory platform verification. Never replace such a blocker with weaker
tests or guessed recovery. Report the tested source, every validation status,
remaining exclusions, and retained remote resources. The completion gate and
user-authorized integration into develop are satisfied; later changes must not
reuse this acceptance claim without the required final-tree verification.
