---
title: M7 Cleanup and Deletion Implementation Baseline
---

# M7 Cleanup and deletion implementation baseline

**Status: Approved implementation scope, not a completion claim.** On 2026-09-16
the operator approved continuous S0-S7 implementation without per-slice approval
stops. This does not authorize Frozen-contract changes, commits, integration,
push, release or deployment. New normative conflicts return to design review.

## Scope and approved decisions

Normal deletion and AbandonManagement are managed executions: pure compilation,
Run acceptance before Admission, single-owner execution and durable outcome.
Cleanup is a deletion capability, not an Action. Reuse M6 leases/reconciliation
and M6.5 custody. Do not introduce replayable Plans or another recovery engine.

- Ambiguous Cleanup permits external verification/repair followed by explicit
  operator confirmation, or Abandon. Confirmation is not a Hook result and does
  not rewrite the original Run or restore trust for ordinary management.
- Partial finalization may be abandoned. Protect only surviving bytes; disclose
  irreversible prior deletion and do not claim service completeness.
- Preserve Run history, Artifacts and Snapshots after Instance removal. Do not
  add automatic expiration; existing independent Artifact deletion remains.
- Clear/Open decides trust consequence; durable deletion evidence separately
  decides whether another Cleanup or physical finalization is authorized.
- Frozen Core/Hook V1/V2 remain unchanged. V8 is independent persistence
  evolution. Exclude Snapshot deletion, active-Instance retained-resource
  discard, broader external resource kinds, M8, stable JSON and public Rust APIs.

## Evidence and crash ordering

1. Compile exact Instance/version/Revision and typed active/retained Cleanup
   requirements without locks, pins, Runs or filesystem mutations.
2. Accept a Run, then admit under existing ownership/mutation gates. Abandon
   does not inherit Cleanup requirements or ordinary readiness.
3. Publish operation-specific launch authorization before process creation.
   In-process launch arbitration is not durable evidence after restart.
4. Record reliable no-launch or terminal non-success when established. After
   owner loss, authorization without reliable disposition is ambiguous even if
   the process actually never started. Do not replay or infer completion.
5. Valid success with Clear risk and established process termination permits
   atomic finalization authorization. completion_accepted is not that commit.
6. Authority explicitly records Hook completion, operator assertion or absence
   of an active Cleanup declaration. Never fabricate Hook success.
7. Freeze the allocation work set; finish only Pactrun-owned lifetime duties.
   Filesystem and SQL changes are not one atomic transaction.
8. Remove Instance management state and publish completion only after the whole
   storage work set finishes.

Confirmed owner loss finishes the original Run as Interrupted. Later execution
uses durable obligations, not the old Plan. No public Deleting/DeletionFailed
state is introduced. Existing guards are not cleared by ordinary success.
Ambiguity alone does not manufacture Open risk or ManualRecoveryRequired.

| Durable evidence | Authorized recovery |
| --- | --- |
| No launch authorization | Reconcile Pactrun-owned state; do not claim Cleanup ran |
| Authorization without reliable result | Preserve unresolved duty; no replay or inferred finalization |
| Reliable non-success | Apply durable risk consequence; a later explicit attempt is a new execution |
| Finalization authorization | Finalization-only retry, never Cleanup replay |
| Deletion receipt | Inspect prior result; never act on a new same-name Instance |

## Custody and historical state

V8 separates historical Instance identity from live management so history and
Artifacts survive deletion. Historical provenance must not permanently pin a
Revision; actual unresolved references remain roots as long as needed.

Finalization includes owned active/retained allocations and exposed/protected
unpublished attempts. An identity disappearing from a Revision is not deletion
authority. Unknown filesystem entries are never adopted. Unexposed preparations
retain their existing conservative cleanup rules.

Abandon atomically publishes detached custody and removes management, with
explicit Run history and no Package code. GC, housekeeping and maintenance must
not destroy retained bytes. Handoff is explicit; ordinary diagnostics do not
disclose native paths or contents.

Discard requires one exact detached AllocationId, explicit destructive intent,
allocation-scoped exclusion and absence of active/unresolved references. It has
durable intent and completion receipts. Startup/GC do not continue discard.
A completed receipt never authorizes deletion of a replacement at the same
pathname. Neither discard nor Abandon claims to stop external processes.

Finalization must use safe platform operations beneath recorded owned roots,
without following symlinks/reparse points or deleting replacements on retry.
When ownership/identity is uncertain, fail closed and preserve evidence.

## Human interface

Use instance delete/abandon, instance deletion show and confirm-complete
(InstanceId plus attempt RunId), and service-storage detached list/show/discard
(AllocationId). Confirmation/discard require explicit intent, not generic force.
Instance mutations use existing version checks. Ordinary recovery override and
ResolveManualRecovery do not bypass deletion obligations. No stable JSON is added.

## Slices and evidence

| Slice | Implementation and exit evidence |
| --- | --- |
| S0 | Canonical contract/schema/CLI and requirement matrix, no runtime claim |
| S1 | V8 exact-V7 upgrade, strict readers, history and custody; atomicity/reference tests |
| S2 | Typed compiler/admission and shared lifecycle gates; purity, missing requirements, competing/stale admissions |
| S3 | Cleanup runtime/results; real Hook risk, cancellation and launch/completion crash windows |
| S4 | Finalizer/removal; partial I/O, replacement safety, finalization-only retry and history retention |
| S5 | Confirmation/managed Abandon; stale assertions, zero Hook launches, partial handoff and GC protection |
| S6 | Detached inspection/discard; exact authority, references, interruption and replacement protection |
| S7 | CLI, traceability, Windows adapters, remote full CI and exact-source closeout |

Tests accompany each slice. A destructive command is not activated before its
admission, execution, recovery and direct verification path is complete.

| Scenario (not a test ID) | Owning requirements | Required evidence |
| --- | --- | --- |
| D7-acceptance | PR-REQ-0036, PR-REQ-0037 | Delete/Abandon Runs before admission; pure Compiler |
| D7-context | PR-REQ-0176, PR-REQ-0177, PR-REQ-0178 | Active/retained context, no ordinary-readiness inheritance, no launch on missing inputs |
| D7-risk | PR-REQ-0112, PR-REQ-0179, PR-REQ-0216 | Clear/Open matrix, no false success or guard clear |
| D7-ambiguity | PR-REQ-0058, PR-REQ-0180, PR-REQ-0247 | Fresh-process loss, no replay/compensation/finalization inference |
| D7-finalization | PR-REQ-0111, PR-REQ-0247 | Authority before deletion, partial I/O and finalization-only retry |
| D7-abandon | PR-REQ-0113, PR-REQ-0181, PR-REQ-0247 | Run history, no Package code, surviving custody and GC protection |
| D7-lifetimes | PR-REQ-0065, PR-REQ-0072, PR-REQ-0073, PR-REQ-0074 | History/Artifacts/Snapshots survive and needed references stay pinned |
| D7-discard | PR-REQ-0247 | Exact detached target, explicit authority and safe traversal |

Final acceptance requires focused tests, Windows adapter tests and complete
cargo xtask ci in the configured persistent remote workspace with pre/post
source-manifest checks. Documentation-only evidence is recorded separately.
