---
title: M6 Recovery Implementation Baseline
---

# M6 recovery bounded baseline

**Status: Approved bounded M6 scope, 2026-09-14. S0 review complete; implementation
and verification proceed through S1-S4 without per-slice approval.**

The operator approved continued M6 implementation after the earlier Pre-M6
scheduling approval. This baseline records that authorization and the S0 audit;
it is not an independent normative source. See the
[M6 implementation record](../m6-implementation-status.md) for evidence,
authorized local integration and exact-source verification discipline.
ServiceStorage representation/runtime remains a
separately approved M6.5 milestone under the
[staged alignment](./service-storage-staged-design-alignment.md).

## Owning contracts and approved limits

Read [recovery and reconciliation](../../spec/execution/recovery-and-reconciliation.md),
[execution and concurrency](../../spec/execution/execution-and-concurrency.md),
[HookProtocolV1](../../spec/contracts/hook-protocol.md),
[M4 publication](../../spec/execution/m4-snapshot-lifecycle-approval-baseline.md),
[M5 execution](../../spec/execution/m5-migration-execution.md), and
[exact V6](../../spec/persistence/persistence-baseline.md).

M6 covers existing Action, Capture, Restore and Managed Input Migration. Keep
M3-M5 owner leases, continuations, durable risk acknowledgments, exact pins,
terminal publication, per-edge Migration boundaries, and existing human CLI.
Recovery preserves Pactrun-owned committed state; it does not replay Hooks,
resume Interrupted Runs, salvage uncommitted outputs, or roll back service bytes.

No public Rust API, stable JSON envelope, new CLI spelling, ServiceStorage
schema, speculative extension framework, or Frozen Core/Hook/error change is
approved. Ordinary technical fixes and evidence work do not need another slice
approval. A genuine new observable semantic or persistence commitment still
requires review. Do not silently alter exact V6.

## S0 findings and V6 decision

**Decision: retain exact V6.** The operation kind, admission boundary, live or
terminal risk, outcome, guard provenance and operation-specific references
already contain the facts needed for the current recovery directives. A new
serialized Plan, directive column or generic recovery store is not needed.

The audit traced application lease probes and the shared transaction-based
`reconcile_managed_run` through operation-specific admission, publication,
reference release, guard publication and strict loading. The boundary matrix is:

| Operation | Durable state before terminal publication | Authorized recovery and roots | Finding |
| --- | --- | --- | --- |
| Action | Accepted owner; Admitted Revision/payload pins; durable Clear/Open risk | Keep current Instance state, finish Interrupted after confirmed loss, apply the open-risk consequence. Terminal guard roots the Run, not old execution bytes (V4 contract) | Existing implementation sufficient; broaden owner/risk crash matrix and generic traceability |
| Capture | Same owner/risk substrate; complete admitted binding registry; no result until Snapshot and terminal outcome publish atomically | Never salvage a candidate. Keep current Instance state and apply the consequence. Successful Snapshot owns its own bytes independently | Existing implementation sufficient; cross-operation recovery and override evidence needed |
| Restore | Invocation plus exact Snapshot admission pin, expected Instance/consequence tokens; old target unchanged before successful terminal transaction | Never replay Restore or use it as compensation. Failed/interrupted Restore preserves target and existing guard; successful qualified Restore clears guard atomically | Existing implementation sufficient; general guard-resolution traceability needed |
| Migration | Whole-path pins, progress and committed-edge records, last committed binding checkpoint; intermediate edge remains Running, final edge includes success | Preserve committed Instance Revision/bindings; no older checkpoint write-back. Release execution/checkpoint roots only with terminal publication; Instance owns committed state and guard roots Run provenance | Existing PR-REQ-0313 implementation sufficient; generic checkpoint/Plan-free mapping and post-terminal reference evidence needed |

For all four operations, Accepted+Open is invalid; ordinary recovery must not
normalize corruption. The application confirms owner loss both before and under
the Instance mutation lock; the transaction rechecks the recorded owner. A
held/inconclusive lease does not permit interruption. Owner identity is neither
a timestamp nor a PID. Concurrent reconcilers share the serialized terminal
boundary rather than maintaining another owner mechanism.

### Exact boundary and evidence inventory

| Boundary or obligation | Existing evidence reviewed | M6 disposition |
| --- | --- | --- |
| Acceptance and Admission before/after commit | PR-TEST-0100, PR-TEST-0111; Snapshot admission and M5 persistence tests | Add PR-TEST-0325/0326 across Action, Capture, Restore and Migration |
| Risk entry and resolution before durable acknowledgment | PR-TEST-0096 and PR-TEST-0253 exercise runtime acknowledgments/retry | Add separate resolution fault points and PR-TEST-0326 so a clear-risk crash is not accidentally an earlier entry crash |
| Terminal failure/reconciliation before/after commit | PR-TEST-0085, PR-TEST-0112, PR-TEST-0156, PR-TEST-0159 | Cross-operation old-or-complete-new assertions in PR-TEST-0326; no duplicated outcome or consequence in PR-TEST-0325 |
| Capture result publication and loss | PR-TEST-0252 asserts uncommitted candidates are not salvaged | Retain real-process tests; no runtime rewrite |
| Restore target publication and guard | PR-TEST-0259, PR-TEST-0260, PR-TEST-0263, PR-TEST-0264 assert both tokens, atomic publication, failures and owner loss | Retain tests and link their PR-REQ-0070 coverage |
| Intermediate/final Migration publication | PR-TEST-0304 and PR-TEST-0317 inspect committed edge counts, Revision/bindings and fresh-process interruption without operator files or Hook replay | Link PR-REQ-0053/0061/0062; retain exact PR-REQ-0313 reference release semantics |
| Unresolved references and provenance | V4 guard FK, V5 Restore admission pin, V6 checkpoint/pin tables; PR-TEST-0085 | Add PR-TEST-0327: terminal provenance resists deletion; later legal Input management is not undone by repeat reconciliation |
| Explicit override and resolution | PR-TEST-0088, PR-TEST-0091, PR-TEST-0306 plus Restore tests | Add real Action/Capture success with override in PR-TEST-0329; no automatic guard clear |
| Reconciler and manual-resolution publication | Shared terminal transaction; guard deletion and fresh state token in one management transaction | Add manual-resolution fault points and PR-TEST-0330 for old-or-complete-new outcomes before/after both transactions, without another Run or repeated consequence |
| Service-authoritative side effects | Trusted native Hook boundary, not OS containment | Add PR-TEST-0328: mutate outside execution scratch after risk ack; bytes survive failure and recovery |

No missing authoritative durable fact or conflicting product rule was identified
for this bounded scope. Findings are additional test instrumentation, assertion
coverage and traceability, not a new production recovery policy. Adequate
existing implementations are retained rather than rewritten cosmetically.

## Delivery slices and exit gates

| Slice | Delivery |
| --- | --- |
| S0 | Contract/reader/writer/boundary audit above; exact V6 retained, no unresolved implementation-design decision |
| S1 | Shared owner-loss/reconciliation evidence across all four operations, including held/inconclusive leases and two independent reconciler processes |
| S2 | Distinct risk-resolution crash injection plus acceptance/Admission/risk/terminal matrix; Plan-free recovery, reference lifetime and committed Migration checkpoint evidence |
| S3 | Existing inspect/reconcile/override/manual-resolution contracts; real native trust-boundary and successful Action/Capture override regressions |
| S4 | Full regression, bidirectional traceability, Windows adapter checks, exact-source remote full CI and closeout before authorized integration |

Every mechanically verifiable in-scope obligation needs actual test artifacts.
PR-REQ-0246 ServiceStorage target publication belongs to M6.5. PR-REQ-0247 and
the successful-deletion clause of PR-REQ-0070 belong to M7. Related tests do not
claim those runtimes. Broader M5/ServiceStorage pending requirements are not
closed merely by this bounded recovery work.

## Verification and integration discipline

Keep stable PR-REQ/PR-TEST identifiers with both directions linked. The final
report must distinguish source inspection from automated proof and disclose
any failure or remaining uncertainty. Run workspace-contained local tests,
applicable Windows adapter tests, and full configured-remote `cargo xtask ci`
after the final code, requirement, traceability and closeout edit.

Use a persistent remote checkout with its own Cargo target and locally installed
locked document dependencies; do not share compiled-in workspace paths or
symlink another checkout's node_modules. Compare the source manifest before and
after CI and again against the final local tree. A passing earlier revision is
not evidence for later edits. The runtime gate does not authorize commit, merge,
push or Pages deployment; integration requires explicit operator authorization.
