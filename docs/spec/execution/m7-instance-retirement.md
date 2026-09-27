---
title: M7 Instance Retirement
---

# M7 Instance retirement

**Status: Approved M7 contract; runtime activation and verification are separate.**
The 2026-09-16 implementation authorization accepts the bounded operator choices
in the [M7 baseline](../../development/design-notes/m7-cleanup-deletion-implementation-baseline.md).
This extends lifecycle representation without modifying Frozen Hook/Core formats.

<!-- spec-navigation:start -->
Related contracts: [Cleanup](../contracts/hooks-recovery-and-cleanup.md),
[ServiceStorage custody](./m6-5-service-storage-execution.md), and
[V8 persistence](../persistence/persistence-baseline.md).
Delivery evidence: [M7 implementation status](../../development/m7-implementation-status.md).
<!-- spec-navigation:end -->

### PR-REQ-0334 - Evidence-directed deletion recovery

Deletion MUST reuse confirmed owner-loss and durable risk reconciliation.
Before a Cleanup process may start, Pactrun MUST durably authorize that attempt's
launch. An authorization without reliable no-launch or terminal disposition
after owner loss MUST remain unresolved. Clear risk does not establish Cleanup
completion, and unresolved completion does not by itself set Open risk or
ManualRecoveryRequired. A reconciler MUST NOT replay Cleanup or infer service
compensation. The old Run finishes Interrupted, never resumes its Plan.

Known non-success follows existing risk rules. Only reliably completed success
with Clear risk and the required supervised-process termination may produce a
Hook-completed finalization authorization. An existing guard is not implicitly
cleared by failure with Clear risk. Finalization authority remains effective
after owner loss; only Pactrun-owned finalization may be retried.

**Verification: PR-TEST-0389, PR-TEST-0391, PR-TEST-0397, PR-TEST-0399, PR-TEST-0404, PR-TEST-0405, PR-TEST-0407, PR-TEST-0413, PR-TEST-0425, PR-TEST-0447.**

Coverage includes pure evidence/risk policy, durable refusal, real V1/V2 Cleanup
and fresh-process loss around the Cleanup boundary. Finalization interruption
tests cover persisted incarnation and absence evidence. The M7 delivery record
separately tracks remaining execution/concurrency scenarios and platform results;
these references are not a milestone completion claim.

### PR-REQ-0335 - Explicit Cleanup completion assertion

Only an unresolved Cleanup attempt may be confirmed by the operator. The
assertion MUST identify the exact InstanceId and attempt RunId, use the current
InstanceStateVersion under the mutation guard, and explicitly assert that
external verification/repair has completed Cleanup. Stale or different attempts
MUST be rejected. Publish a new Instance state version and an operator-sourced
finalization authorization atomically. Do not fabricate Hook success, modify
the original Run outcome, or clear the trust guard for ordinary management.
No Hook, Compiler or Run is created by the assertion itself. Later finalization
is a new managed deletion execution.

**Verification: PR-TEST-0390, PR-TEST-0398, PR-TEST-0407, PR-TEST-0415.**

Coverage includes pure exact-identity/version policy and transactional assertion
after interrupted Cleanup, without rewriting its outcome. The real-process
boundary test also exercises the CLI's version-bound assertion and stale retry.

### PR-REQ-0336 - Owned finalization and partial abandonment

Normal deletion MUST freeze its owned allocation work set before destructive
finalization. Include active/retained and unpublished protected allocations;
never adopt unknown filesystem entries. Absence of a declared Cleanup requires
its own explicit authority, not a fabricated successful Hook result.

Finalization MUST operate only on authorized owned storage, must not follow
symlinks/reparse points out of scope, and MUST fail closed when ownership or
object identity is uncertain. Record completion so a retry cannot destroy an
unrelated replacement. Filesystem and SQL operations are not one atomic commit.
Instance removal and deletion success require all lifetime obligations to end.

Qualification and removal MUST remain attached to the authorized objects despite
namespace replacement or reparenting. Rechecking a mutable name alone is not
sufficient. Windows uses namespace-pinned handles through the affected ancestry;
Linux claims each selected object into a fresh, allocation-owned processing
parent, validates the captured identity, and only then traverses or removes it.
The ordinary service-facing path is not a continued live-service guarantee once
authorized finalization starts. This bounded retirement isolation is not a
general storage-relocation or attachment operation.

Unexposed creation intents retain their earlier conservative rules. Neither
normal retirement nor Abandon may adopt an existing preparation path merely
because an intent names it. Their unresolved intent/origin evidence remains;
only protected surviving allocations enter detached managed custody.

While an obligation is unresolved, ordinary managed execution and mutations
that bypass it MUST be rejected, even with a recovery override. Inspection,
qualified confirmation, finalization-only retry and Abandon remain available.
Do not add public Deleting/DeletionFailed states.

Abandon MUST remain a managed execution with explicit history and no Package
code, including after partial finalization. Publish detached custody and remove
management atomically. Warn that already removed bytes cannot be restored and
that remaining data does not imply a coherent/running service. GC and ordinary
maintenance MUST preserve detached bytes.

**Verification: PR-TEST-0406, PR-TEST-0408, PR-TEST-0409, PR-TEST-0413, PR-TEST-0414, PR-TEST-0416, PR-TEST-0417, PR-TEST-0420, PR-TEST-0422, PR-TEST-0423, PR-TEST-0426, PR-TEST-0428, PR-TEST-0429, PR-TEST-0430, PR-TEST-0431, PR-TEST-0432, PR-TEST-0433, PR-TEST-0434, PR-TEST-0435, PR-TEST-0436, PR-TEST-0438, PR-TEST-0439, PR-TEST-0440, PR-TEST-0441, PR-TEST-0442, PR-TEST-0443, PR-TEST-0444, PR-TEST-0445, PR-TEST-0446, PR-TEST-0447.**

### PR-REQ-0337 - Detached handoff and discard

Detached allocations MUST remain discoverable by immutable ID after Instance
removal. Explicit handoff may reveal their native location; ordinary diagnostics
must not disclose paths or contents. No historical contract reference may
permanently pin a Revision solely as audit provenance.

After partial physical work, handoff may identify multiple surviving data
locations with their original relative positions. The bytes remain part of the
same allocation's custody, not copies in Inputs, Workspace or Run Artifacts.
Do not claim a coherent service tree or Snapshot. Handoff remains read-only;
busy or unqualified physical progress must not be silently repaired by a query.

Discard requires exact AllocationId and explicit destructive intent, with
allocation-scoped exclusion and rejection of active or unresolved references.
Persist authority before destructive I/O and completion before considering the
obligation discharged. Only an explicit retry may continue interrupted discard;
ordinary GC/startup maintenance does not inherit discard authorization. Completed
receipts do not authorize removal of later objects at the same path. Discard
does not claim to stop service processes or clean external resources.

**Verification: PR-TEST-0409, PR-TEST-0410, PR-TEST-0411, PR-TEST-0412, PR-TEST-0414, PR-TEST-0415, PR-TEST-0417, PR-TEST-0419, PR-TEST-0427, PR-TEST-0429, PR-TEST-0430, PR-TEST-0431, PR-TEST-0432, PR-TEST-0433, PR-TEST-0434, PR-TEST-0435, PR-TEST-0437, PR-TEST-0438, PR-TEST-0439, PR-TEST-0440, PR-TEST-0441, PR-TEST-0442, PR-TEST-0443, PR-TEST-0444, PR-TEST-0445.**

### PR-REQ-0340 - Human retirement commands

The approved human command surface is:

```text
pactrun instance delete <name> [--plan] [--if-version <token>] [execution-options]
pactrun instance abandon <name> [--plan] [--if-version <token>] [execution-options]
pactrun instance deletion show <instance-id>
pactrun instance deletion confirm-complete <instance-id> --attempt <run-id> --if-version <token> --assert-cleanup-complete
pactrun service-storage detached list
pactrun service-storage detached show <allocation-id> [--reveal-location]
pactrun service-storage detached discard <allocation-id> --confirm-discard
```

Execution options are `--authorize-recovery-override`, `--startup-timeout-ms`,
`--execution-timeout-ms`, and `--termination-grace-ms`. Omitted startup/execution
timeouts are unlimited; termination grace defaults to 5000 ms. They do not create
a Hook for Abandon or finalization-only work. Cleanup accepts no parameters.
Duplicate, unknown, malformed and unsupported flags MUST fail before execution.
No generic force flag, stable JSON, replayable Plan or path-based discard exists.

Planning and inspection MUST be read-only: no Run, launch, pin or writer admission
may result. Execution resolves the name once to exact Instance identity/version;
admission checks that compiled identity. Historical inspection uses InstanceId
and MUST NOT act on a later Instance that reuses its name. Delete and Abandon are
managed Runs, including when no Cleanup is declared. Confirmation and detached
discard are explicit management operations, not fabricated Hook Runs.

Confirmation requires all three exact identity/version operands and the specific
external-completion assertion. A missing assertion cannot be replaced with a
recovery override. Ordinary detached list/show MUST reveal neither a native path
nor contents. Only explicit `--reveal-location` authorizes native handoff.
Each disclosed location is paired with `original_relative_location`; partial
retirement may produce more than one pair. These paths identify data objects,
not a grant to modify Pactrun's control journal. The CLI warns when multiple
locations remain rather than presenting them as an intact service tree.
Discard requires the exact immutable AllocationId and `--confirm-discard`; no
active allocation or different path may be substituted. Already-completed
discard reports its existing receipt without acting on a replacement object.

Before destructive execution the CLI MUST warn about irreversible owned-storage
deletion. Abandon MUST warn that prior deletion cannot be undone and surviving
bytes do not establish a coherent/running service. Discard MUST warn that it
neither stops service processes nor cleans external resources. Errors and failed
Runs return nonzero status; ambiguous Cleanup directs the operator to inspect,
reconcile confirmed owner loss, then externally verify/confirm or abandon.

**Verification: PR-TEST-0407, PR-TEST-0415, PR-TEST-0416, PR-TEST-0417, PR-TEST-0418, PR-TEST-0436, PR-TEST-0437.**
