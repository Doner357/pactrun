---
title: Pre-E Readiness Review
---

# Pre-E readiness review

**Status: Implementation and review candidate; final validation and local integration pending.**

This 2026-09-25 follow-up implements the authorized pre-E closure plan on
`feature/obligation-inventory`, starting at develop `f81c308`. It is an informative
evidence record, not a new product contract or E S0 approval. Local commit and
integration into develop are authorized; push, deployment, publication and E
implementation are not.

## Baseline and evidence audit

The prior [closure map](./verification-gap-closure.md#clause-backed-closure-map)
contains 53 original Pending-block dispositions. The retained runtime manifest
has 291 entries, all matching at the start of this work. The retained full-CI
log and separate final documentation logs are present. Those facts establish
provenance, not a new pass and not clause completeness by themselves.

The existing diff adds tests, fixture behavior and requirement references; its
Rust executable changes are in test-only modules. The prior assertions were
checked by boundary: canonical production/oracle vectors and transport tuples;
atomic Instance/Input publication and byte opacity; Run/token/guard changes;
management operations without invented Runs; active/retained Session views;
Hook completion and recovery ACK order; output and Snapshot publication; and
historical V5/V6 schema fixtures. No prior closed row is reopened by this review.
The 17 remaining rows below add missing boundary cases or an explicit review
rather than treating nearby test names as sufficient proof.

New tests PR-TEST-0605 through PR-TEST-0615 cover direct adapter dependencies,
public product exclusions and uninstalled execution, source defaults and closed
transitions, retained lifetime, detached Snapshot plans, guarded operations,
Workspace authority, retirement and pre-acceptance refusal. Existing Migration
admission, service continuity and real owner-crash tests are strengthened.
No production behavior, public API, CLI spelling, schema, Frozen format,
product timeout, or dependency is changed.

## Existing-contract dispositions

| Requirement | Evidence and disposition |
| --- | --- |
| PR-REQ-0001 | PR-TEST-0606 rejects excluded top-level commands without storage creation; PR-TEST-0142/0143 cover actual one-shot use. Composition review below checks the wider boundary. |
| PR-REQ-0003 | PR-TEST-0607 refuses uninstalled exact identities and source paths without Instance, Revision or Run publication. PR-TEST-0072/0143 cover staged validation, immutable installation and installed execution after source removal. |
| PR-REQ-0007 | The acceptance matrix below maps all five families, including durable post-acceptance refusals and management's separate no-Run path. |
| PR-REQ-0008 | Compiler/executor review below plus PR-TEST-0090/0115/0595 and actual changed-source Capture/Restore, Migration and Cleanup/abandonment in PR-TEST-0610/0611/0614. |
| PR-REQ-0009 | Explicit composition/dependency review plus PR-TEST-0605 and its negative fixtures. The source guard is intentionally bounded, not a Rust macro/whole-program proof. |
| PR-REQ-0010 | Domain/adapter review, typed normalization PR-TEST-0041/0068, and the direct dependency guard PR-TEST-0605. |
| PR-REQ-0028 | PR-TEST-0075/0147/0590 exercise empty/binary bytes and detached acquisition. Same-ID/same-meaning is an author obligation; content/filename guessing is not a validator. |
| PR-REQ-0030 | PR-TEST-0610 migrates active to retained to reactivated to retained, explicitly discards one binding, and deletes another while unrelated retained bytes survive. PR-TEST-0614 checks delete and abandon remove the selected Instance's retained references without touching another Instance. |
| PR-REQ-0093 | PR-TEST-0147/0591 verify disclosure and managed-reference deletion. The wording review below checks the negative guarantee; physical media erasure is neither claimed nor tested. |
| PR-REQ-0110 | PR-TEST-0612 combines inspection, Input set/export/delete, denied ordinary admission, resolution, exact Restore and abandonment under a real guard. PR-TEST-0329 proves one-run Action/Capture override preserves the guard; PR-TEST-0259/0156/0173 retain exactness and public-path coverage. |
| PR-REQ-0123 | PR-TEST-0608 compares supported source versions' shorthand/defaults to explicit typed values and refuses omitted transitions. PR-TEST-0611 proves installed execution ignores replaced/deleted source. No new shorthand is implemented. |
| PR-REQ-0127 | PR-TEST-0039/0069 cover grammar and normalized identity; author semantic non-reuse and independent frontend evolution receive the explicit responsibility review below. |
| PR-REQ-0131 | PR-TEST-0076/0147/0346 cover protection metadata, opaque bytes and disclosure. Security wording review rejects vault/erasure claims. |
| PR-REQ-0143 | PR-TEST-0613 writes real scratch and checks success, rejected completion and timeout, cleanup, unchanged bindings and no ServiceStorage allocation. Strengthened PR-TEST-0252 kills an actual owner before publication and checks orphan scratch cleanup with no Snapshot salvage. PR-TEST-0100/0110 cover delayed cleanup and non-authoritative residue; PR-TEST-0263/0313 cover failed managed publication. |
| PR-REQ-0159 | PR-TEST-0609 rejects declarative split/merge/transform and added transformation fields in source V1/V2/V3, with a legal Carry control. PR-TEST-0312 proves real typed Hook-produced opaque output. |
| PR-REQ-0166 | Strengthened PR-TEST-0374 preserves non-JSON binary service bytes in the same allocation with no Hook launch; PR-TEST-0388 checks explicit presence without copying/adopting bytes. PR-TEST-0610 proves Hook-free binding migration. Service schema correctness remains Package-owned. |
| PR-REQ-0170 | PR-TEST-0328/0602 exercise native external writes without rollback. The host-authority review below explicitly includes filesystem, network, processes and another CLI; these are not isolation promises. |

### Acceptance and admission matrix

PR-TEST-0615 invokes valid public command shapes for all five families and
asserts resolution failure returns operation failure (not syntax failure),
creates no Run, and leaves the existing Instance intact.

| Family | Post-acceptance refusal evidence | Actual execution/completion evidence |
| --- | --- | --- |
| Action | PR-TEST-0089, PR-TEST-0612: durable Accepted/Finished failure before any Hook | PR-TEST-0106, PR-TEST-0613 |
| Capture | PR-TEST-0225: readiness/fact refusal, Accepted boundary, no completion or pin | PR-TEST-0246, PR-TEST-0611 |
| Restore | PR-TEST-0233: compilation refusal has zero Runs; stale/deleted/corrupt source after acceptance has durable failure and no launch | PR-TEST-0258, PR-TEST-0611 |
| Migration | Strengthened PR-TEST-0302: conflict/stale refusals remain Failed at Accepted boundary after reopen, no completion | PR-TEST-0312, PR-TEST-0610 |
| Cleanup | PR-TEST-0397: stale accepted deletion ends Failed without launching or publishing a deletion obligation | PR-TEST-0404, PR-TEST-0614 |

PR-TEST-0558 and PR-TEST-0612 verify management mutations remain outside this
execution pipeline; compiling or inspecting does not manufacture an attempt.

## Architecture and responsibility review

### Composition, dependencies and policy ownership

The production binary enters `src/main.rs`, then the deliberately narrow library
facade and the explicit CLI dispatcher. Application services compose persistence,
workflow compilation and Hook supervision for a bounded invocation. The worker
threads, IPC listeners and owner continuations serve that invocation; they are
not a daemon, listening web service, scheduler or remote orchestration API.
No capability registration or service locator is used to discover operations.

Production Domain modules import typed Domain values and commodity standard
library/Serde mechanisms, not CLI, persistence, YAML/argument-parser or runtime
adapter types. Typed identities, protection, operation/plan enums, Migration
writer rules, service transitions and compatibility rules remain Pactrun-owned.
SQLite adapters enforce the owning atomic/CAS/pin constraints at transactions;
SQLite row order, YAML implicit typing and JSON library defaults do not define
product semantics. Third-party codecs, hashing, storage and OS calls implement
mechanisms. The source guard checks named direct dependencies and mutable
statics, including negative examples; aliases, macros, new module layouts and
future global-state patterns still require review. It is not a substitute for
this ownership review.

`compile_action`, `compile_snapshot`, `compile_migration` and `compile_deletion`
consume resolved typed facts. Executor/owner continuations retain typed plans
and revalidate admitted facts, not YAML or CLI strings. Migration reevaluates
the already-selected installed edge over committed payload references after
its own edge commits; this is required fact revalidation, not source-default
inference or path reselection.

### Authoring defaults and author-owned meaning

Source V1 materializes empty top-level collections, Input required/protection
and runtime executable defaults. V2/V3 additionally materialize service
collections, exposure/mutation defaults and empty Hook service grants and
service transition lists. Input Migration transitions are required explicit
lists: these frontends do not offer source-dependent Carry/Keep shorthand.
The single retained binding registry's continuity/reactivation is the existing
identity/lifetime rule, not an authoring shorthand guessed at execution.

Stable Input meaning is the author's responsibility across one Package lineage.
Pactrun checks grammar, normalized identity, writer/protection rules and byte
preservation; it cannot decide arbitrary semantic sameness from names or bytes.
A future frontend needs its own approved versioned contract. Service-specific
schema conversions and correctness remain Package responsibilities; the
Pactrun commit governs managed associations and outputs, not external rollback.

### Security wording and native host authority

Reviewed product help/dispatch, Input deletion/export, Snapshot/Pack warnings,
Hook protocol/runtime descriptions and the owning security/ownership Spec.
Deletion reports a managed reference operation, not wiped media. Database,
WAL, backup, staging and authorized export bytes have no secure-erasure promise.
Secret is protection/disclosure metadata, not encryption or a vault.

Native Hook launch uses the trusted host process identity, without an OS
confinement backend. Session grants restrict Pactrun-mediated operations;
they do not stop the same OS identity using files, sockets, subprocesses or a
separate CLI. Those out-of-Session actions can violate a Package contract
without being a sandbox escape. No network access or unrelated host mutation
was performed merely to demonstrate this absence of confinement. Actual
filesystem effects are tested in isolated fixture directories. No universal
Secret redaction, network/process isolation or service rollback is claimed.

### Unnumbered obligations and historical scope

All 12 previously identified keyword-bearing unnumbered groups were checked
against current consumers, not only requirement IDs:

| Group | Review/disposition |
| --- | --- |
| Open command spelling | Current Migration/retirement/transport routes are linked; stale scheduling language is corrected, not treated as missing runtime. |
| Persistent/execution scopes | Detached Input and live service state remain distinct (PR-TEST-0346); no implicit synchronization. |
| ServiceStorage is not Input | Implemented Core V2 is distinguished from unsupported broader external resources. |
| Hook wire verification | Original wire-only oracle scope is preserved; runtime proof is separately linked under PR-REQ-0218. |
| Candidate evolution | Closed selected-version dispatch and malformed-version refusal remain enforced; no fallback or public Candidate API. |
| Presentation/provenance | Typed target existence, metadata preservation and no inferred trust remain covered by PR-TEST-0062/0544. |
| Shared recovery/ownership | PR-TEST-0325/0326 retain live/unknown-owner refusal, no replay/adoption/rollback and committed outcomes. |
| Product-design constraints | Minimal Packs still need no service storage, Migration or recovery bookkeeping; closed capability shapes and existing actionable help/diagnostics preserve the low floor. This is a design review, not a universal usability proof. |
| Data ownership | Input, service bytes, scratch, committed Artifact and Snapshot lifetimes remain separate; new scratch/lifetime matrices strengthen this boundary. |
| Repository ordering | Explicit byte/rank ORDER BY clauses remain in metadata reads, including fixed leading components; merged presentation uses typed target/field ranks. PR-TEST-0066 covers the ordering contract. |
| Typed metadata mutations | Typed batches and CAS validate targets and reject contradictory mutations atomically (PR-TEST-0064/0065/0067); historical interface names do not create a new API. |
| Snapshot loading/mutation | Current typed discriminants, exact registry closure, consequence counters and Restore tokens retain corruption rejection and commit atomicity; V5/V6 records are historical, not current admission claims. |

PR-REQ-0115, PR-REQ-0218 and PR-REQ-0308 already received scoped wording fixes
in the prior uncommitted closure and remain correct. PR-REQ-0084, PR-REQ-0142,
PR-REQ-0040 and the unnumbered service/command summaries are checked against
current versioned owners without changing Frozen V1 semantics. Old command
paragraphs are qualified where later short-ID or machine-output contracts apply.

## E handoff and exclusions

| Current dimension | Baseline and support boundary |
| --- | --- |
| Product | Cargo 0.1.0; no automatic 1.0.0 promotion or publication |
| Persistence | Ordinary V11; explicit exact V8/V9/V10 upgrade. Older schema fixtures/history do not add current upgrade routes. |
| Pack source / Core | Explicit source V1/V2/V3 and corresponding Core dispatch; Core V1/V2/V3 Frozen. No unknown-version fallback. |
| Hook | Independently Frozen V1/V2; Core and Hook numbers are not coupled |
| Snapshots | Integrity V2 writer with specified V1 reading; Snapshot bundle V1 transport |
| Revision transport | Pack bundle V1; preserve exact immutable identity and typed metadata |
| CLI | `pactrun.cli.v1` results and `pactrun.cli.events.v1` JSONL; human rendering is not a machine API |
| Identity / errors | Existing Frozen canonicalization, lexical, error-identity and vector contracts remain binding |

E S0 must decide concrete product support/version declarations and the formal
schema/format/interface consolidation and removal inventory before runtime work.
Historical fixtures, development-only compatibility paths, old milestone naming
and redundant navigation are candidates to review, not deletions authorized by
this task. Final-contract conformance, not development provenance, governs any
future data-acceptance decision.

PR-REQ-0329 through PR-REQ-0333 remain unimplemented E obligations.
PR-REQ-0171 remains conditional on separately approved enforced isolation.
Retired Recipes, broader external resources, encrypted storage/vault behavior,
registry/signing, public Candidate API, full tutorials, Pages deployment and
publication remain excluded. Formal-baseline actual-use evaluation and release
mechanism acceptance remain pre-publication work, not invented pre-E gates.

## Validation and integration

Final candidate checks, source hashes, ignored-test disposition and the authorized
local integration result will be recorded here after they complete. No current
candidate full-CI or integration pass is implied by earlier evidence above.
