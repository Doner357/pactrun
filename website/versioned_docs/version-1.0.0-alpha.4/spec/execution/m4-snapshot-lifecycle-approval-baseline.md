---
title: M4 Snapshot Lifecycle Approval Baseline
---

# M4 Snapshot Lifecycle Approval Baseline

**Status: Approved normative implementation boundary. M4 implementation and
verification are complete and integrated into develop.**

<!-- spec-navigation:start -->
## Reading map (informative)

This is a normative operation boundary, not merely a historical plan. Read it when extending the shared Action, Capture, and Restore acceptance and recovery substrate.

Start with the [specification map](../index.md)
and [shared vocabulary](../glossary.md) if a term is unfamiliar.
Check [implementation status and remaining decisions](../../development/next-milestone.md)
before treating an approved contract as available runtime behavior.
The original status, rules, exceptions, and verification declarations below retain their meaning.
<!-- spec-navigation:end -->

This baseline records the M4 plan approved on September 11, 2026. The completed
implementation and its bounded coverage are recorded in the
[execution record](../../development/m4-implementation-status.md).
M3 remains integrated; M4 extends its typed modular-monolith substrate without
a generic workflow engine or a second execution ownership model.

### PR-REQ-0289 - Operation-specific managed execution admission

Action, Snapshot Capture, and Snapshot Restore MUST share typed acceptance,
Admission, execution ownership, mutation-conflict, and terminalization machinery.
Sharing machinery MUST NOT classify all three operations as Mutate.

| Operation | Authoritative access |
| --- | --- |
| Action | Its persisted exact Action declaration |
| Capture | Its persisted exact Capture capability declaration, Observe or Mutate |
| Restore | Always Mutate under PR-REQ-0191 |

The admitting operation MUST use its revalidated exact Revision; competing
Admitted Runs MUST use their pinned Revisions. Caller-supplied access is never
correctness authority. Observe Capture MUST coexist with Observe and Mutate
according to PR-REQ-0045. Mutate Capture, Mutate Action, and Restore MUST share
the same per-Instance Running-and-Admitted Mutate exclusion. Accepted-only Runs
occupy no exclusion. Conflict evaluation and pin establishment MUST be atomic.
Management mutations retain their existing per-transaction coordination and
MUST NOT acquire a new execution-long management lock.

PR-REQ-0091 and PR-REQ-0191 remain binding: Compiler MUST model Capture's
RequiredInputsSatisfied, parameters, and runtime prerequisites, and Admission
MUST independently revalidate them before Hook launch. Missing required active
bindings MUST cause actionable refusal with no Hook launch. Compile refusal
creates no Run; refusal after Run acceptance terminalizes that Run under the
existing refusal precedence. An empty bound payload satisfies presence.
Snapshot format support for required absence MUST NOT permit production
Capture from an incomplete Instance. Active optional absence remains legal.

Capture MUST pin one complete binding registry, including retained bindings;
its Hook receives only the active view. The pinned Revision and registry define
roles, protection, and absence. Later current-binding changes MUST NOT replace
this view. Capture has no Restore-style terminal state-token comparison.

**Verification: PR-TEST-0220, PR-TEST-0221, PR-TEST-0222, PR-TEST-0223, PR-TEST-0224, PR-TEST-0225, PR-TEST-0226, PR-TEST-0227, PR-TEST-0228, PR-TEST-0229, PR-TEST-0230, PR-TEST-0231, PR-TEST-0232, PR-TEST-0233, PR-TEST-0234, PR-TEST-0235, PR-TEST-0236, PR-TEST-0237, PR-TEST-0238, PR-TEST-0239, PR-TEST-0240, PR-TEST-0246, PR-TEST-0248, PR-TEST-0249, PR-TEST-0253, PR-TEST-0262, PR-TEST-0267, PR-TEST-0270, PR-TEST-0271, PR-TEST-0273.**

**Historical S4–S6 coverage (informative):** the following paragraphs describe
the evidence accumulated before the completed M4 CLI closeout. They do not
reopen that delivery or claim that early fixtures already proved later stages.

Partial S4 coverage: the shared persisted-access predicate covers all operation
pairs; production Action Admission includes Snapshot competitors and commits
its refusal or pins atomically. Snapshot competitors in these tests are durable
shape fixtures, not production Capture/Restore Admission. Owned Snapshot
failure/interruption uses the shared terminal transaction; the generic Action
finalizer refuses Snapshot success without an operation-specific atomic result
publisher. PR-TEST-0224 through PR-TEST-0230 add real typed Snapshot acceptance,
operation-aware loading, owner-checked Capture Admission, independent readiness
and exact-fact validation, immutable complete registry pins, and real-process
Capture/Action arbitration with lease-based explicit reconciliation. Restore
competitors in the earlier tests use durable admitted-shape fixtures. S4 now
also covers the read-only Snapshot Compiler, typed parameters and runtime
qualification, actual staged Restore Admission, atomic strong pins and dual
tokens, and the shared owner-continuation registry. Tests exercise real-process
Restore races/crashes and count calls at the one-shot, cancellation-arbitrated
launch boundary. A claim cannot be checked out or used to launch twice; its
success or failure retains owner state rather than replaying the Hook.

S5 adds real Capture Hook execution, active-only materialization from complete
pins, candidate acquisition and atomic V2 publication in PR-TEST-0246 through
PR-TEST-0254. S4 readiness and conflict tests remain the admission evidence.
S6 adds RestoreSession content authorities and successful atomic replacement.
At the S6 stage, the full multi-slice requirement was not yet end-to-end
complete through the human CLI. The subsequent [M4 closeout](../../development/m4-implementation-status.md)
records completed CLI delivery and its tested source. For current baseline
qualification, use the [E ledger](../../development/e-implementation-status.md);
earlier stage evidence is not relabeled as a fresh pass.

### PR-REQ-0290 - Capture publication and capture time

Capture MUST emit only the current [Snapshot integrity baseline](../contracts/snapshot-integrity.md),
using its exact verifier. Historical format-type names do not select retired
numeric writers or readers.
Pactrun MUST validate the Hook-submitted candidate and independently determine
its complete content digests. Only explicitly submitted service content belongs
to the candidate; Pactrun MUST NOT scan ServiceStorage, unrelated storage, or
all Workspace files. No SnapshotCandidate must first become a Run Artifact.

Pactrun MUST sample captured_at exactly once from the UTC wall clock when the
Hook's successful Capture completion is first accepted by the protocol. The
time denotes completion of the submission, not service-wide consistency,
Admission, or terminal commit. Validation and owner-held publication retries
MUST retain this value and the same generated opaque SnapshotId. A failed or
unpublished Capture produces no authoritative Snapshot.

One terminal transaction MUST publish the Snapshot, its complete owned payload
references, the Run's result reference, recovery disposition, and Succeeded
outcome. Snapshot creation itself MUST NOT advance InstanceStateVersion.
Private or durable candidate staging does not constitute a PreparedCommit and
MUST NOT permit a reconciler to publish the Snapshot after owner loss.

**Verification: PR-TEST-0246, PR-TEST-0247, PR-TEST-0248, PR-TEST-0249, PR-TEST-0250, PR-TEST-0251, PR-TEST-0252, PR-TEST-0253, PR-TEST-0254, PR-TEST-0255, PR-TEST-0256, PR-TEST-0273, PR-TEST-0275.**

### PR-REQ-0291 - Restore authority and guarded publication

Restore MUST require exact producer/target RevisionIdentity equality. The Hook
MUST receive both the staged active Snapshot binding view and the selected
immutable Snapshot's read-only snapshot_content authority under Frozen
PR-REQ-0211 and PR-REQ-0213. Its logical role/path/blob_digest mapping MUST refer
to exactly matching materialized bytes. The materialized_path is a Session
locator, not identity. No other Snapshot content, global Snapshot browsing
authority, Capture write authority, or ServiceStorage scan is allowed. This
reuses Frozen HookProtocolV1 and makes no native-process sandbox claim.

Restore MUST replace the complete managed binding state, not merge it, while
preserving target Instance identity and name. Before Hook launch it MUST reject
the same bound Input's Secret-to-Normal replacement and an active-required
bound-to-absent transition. It MUST NOT simulate delete/recreate, silently
promote protection, or acquire declassification authority. Otherwise-legal
optional or retained removal is allowed and ends that binding's sticky
continuity without implying secure erasure. Required absent-to-absent is not
refused merely for absence. Restore MUST NOT acquire a blanket target
RequiredInputsSatisfied prerequisite; it validates staged Snapshot state.

Each Instance MUST have a monotonic RecoveryConsequenceVersion. Every newly
published durable open-risk consequence MUST advance it once in the same
terminal transaction, even if the existing guard and InstanceStateVersion do
not change. Risk entry/clear alone does not advance it. Repeating a terminal
publication MUST NOT advance it twice. It is a conflict token, not a new
recovery state, lock, or replay authority.

Restore Admission MUST retain exact InstanceStateVersion and
RecoveryConsequenceVersion observations. Its successful terminal transaction
MUST atomically compare both current values. Either mismatch is a
post-execution publication conflict: publish no target replacement or
Succeeded outcome, clear no guard, and never refresh, recompile, retry, or
replay the Hook. A failed Restore follows its own durable risk disposition.

When both comparisons match and risk is Clear, one terminal transaction MUST
publish target-instance-scoped fresh payload identities, complete replacement,
a fresh InstanceStateVersion, recovery boundary/disposition, Succeeded, and
clear any existing ManualRecoveryRequired guard. Failed, cancelled, timed-out,
or interrupted Restore MUST NOT clear a pre-existing guard.

**Verification: PR-TEST-0203, PR-TEST-0232, PR-TEST-0233, PR-TEST-0239, PR-TEST-0241, PR-TEST-0257, PR-TEST-0258, PR-TEST-0259, PR-TEST-0260, PR-TEST-0261, PR-TEST-0262, PR-TEST-0263, PR-TEST-0264, PR-TEST-0265, PR-TEST-0266, PR-TEST-0267, PR-TEST-0268, PR-TEST-0269, PR-TEST-0271, PR-TEST-0479, PR-TEST-0482, PR-TEST-0485.**

Coverage includes atomic consequence-counter advancement, existing guards and
terminal retries, staged Restore eligibility/protection checks, and admission
of exact dual tokens without refreshing or clearing guards. Frozen Hook content
authority and successful dual-token-guarded replacement/publication now have S6
runtime coverage below. These tests use real Hooks, not admission-only fixtures:
PR-TEST-0257 through PR-TEST-0267 include selected service bytes, forbidden
authority, cross-Instance replacement, failure/guard preservation, token-only
conflicts, large content and atomic crash/retry boundaries.

## Shared recovery and ownership boundary

The single-owner staging lease and explicit run reconcile substrate extends
from Action to Capture and Restore. A confirmed-lost non-terminal owner MUST
be reconciled as Interrupted according to PR-REQ-0064/0287; clear risk creates
no new guard, while open risk applies the durable consequence. A live or
unknown owner MUST remain untouched. Already committed outcomes are unchanged.
Corrupt states MUST fail closed. Reconciliation MUST NOT replay Hooks, resume
workflow steps, infer external state, recreate a candidate, salvage output, or
complete a staged Restore. Only non-success recovery/housekeeping is allowed.
Protocol completion_accepted is not a managed publication boundary.

## Scope, versions, and ownership

Use the current [integrity](../contracts/snapshot-integrity.md),
[bundle](../contracts/snapshot-bundle.md),
[persistence](../persistence/persistence-baseline.md), and
[fixed capabilities](../behavior/m4-runtime-capabilities.md).
Their versions remain independent of the Revision and Hook protocol domains.
Snapshot owns its payload closure independently of its origin,
creator Run, or installed producer. Restore copies into new target-scoped
payload identities. There is no cross-resource CAS/dedup ownership model.

Historical M4 scope (informative): that delivery included human
capture/restore/import/export/verify and minimum inspection. It excluded
Snapshot deletion, Revision deployment, Migration/Cleanup execution,
ServiceStorage runtime, background reconciliation, public Rust APIs, stable
machine output, and Pages deployment. Later delivery evidence belongs to the
[current handoff](../../development/next-milestone.md) and its linked records.
An M4 test is evidence for its covered clause; it does not establish coverage
of later lifecycle or presentation work. These historical exclusions do not
describe current runtime availability.
