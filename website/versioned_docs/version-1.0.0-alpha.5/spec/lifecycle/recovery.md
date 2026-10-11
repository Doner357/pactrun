---
title: Recovery and Reconciliation
---

# Recovery and Reconciliation

A durable history does not necessarily provide a replay mechanism.
[Temporal Workflow replay](https://docs.temporal.io/workflow-execution#replays)
reconstructs Workflow state against recorded events. Pactrun instead uses committed
state, execution ownership and durable risk markers to decide recovery. A retained
Run is not permission to invoke its Hook again; service-side effects are not
reconstructed by replaying the Run history.

## Recovery-risk protocol

### PR-REQ-0053 - Transition checkpoint boundary

A `TransitionCheckpoint` MUST contain the authoritative Pactrun-owned state or
references needed to return to the last committed boundary. It MUST NOT be
presented as a service Snapshot or as proof that external service state can be
rolled back. It does not capture or restore authoritative ServiceStorage-backed
Managed Service Resource bytes.

**Verification: PR-TEST-0304, PR-TEST-0317, PR-TEST-0327.**

### PR-REQ-0054 - Runtime risk state

Recovery consequence MUST be determined by the runtime protocol state
`Clear | Open`, not by Action names, Observe or Mutate classification,
operation category, or a Package-authored recovery-safety flag.

**Verification: PR-TEST-0085.**

### PR-REQ-0055 - Durable risk entry before acknowledgment

Before acknowledging `EnterRecoveryRisk`, Pactrun MUST durably publish an open
risk marker and an operation-specific recovery directive. A Hook MUST NOT cross
the associated recovery-relevant service-side-effect boundary before receiving
that acknowledgment. A crash around acknowledgment MAY conservatively produce
a false-positive manual-recovery obligation, but MUST NOT create a false claim
that an unrecorded risky boundary is safe.

**Verification: PR-TEST-0096, PR-TEST-0326.**

### PR-REQ-0056 - Risk resolution

`ResolveRecoveryRisk` MUST mean that the Package considers service-owned state
coherent enough for ordinary Pactrun management. It MUST NOT imply rollback to
the original value or require the Run itself to succeed. Pactrun MUST durably
clear risk before acknowledging resolution.

**Verification: PR-TEST-0096, PR-TEST-0326.**

### PR-REQ-0057 - Terminal recovery consequence

A terminal non-success with clear risk MUST NOT by itself require manual
recovery. A terminal non-success or execution loss with open risk MUST apply the
already durable Pactrun-owned recovery directive and place the Instance in
`ManualRecoveryRequired`. A Hook that reports success while risk remains open
MUST cause protocol failure and `ManualRecoveryRequired`. In that state,
Pactrun MUST NOT claim that service-owned live state remains coherent with the
currently committed source Revision merely because the Pactrun-owned boundary
was recovered.

**Verification: PR-TEST-0156, PR-TEST-0159, PR-TEST-0173, PR-TEST-0174, PR-TEST-0326, PR-TEST-0328, PR-TEST-0370.**

### PR-REQ-0058 - No inferred compensation

Pactrun MUST NOT automatically replay a Hook, infer a reverse operation, or
choose Snapshot Restore as compensation after failure or restart.

**Verification: PR-TEST-0159, PR-TEST-0174, PR-TEST-0327, PR-TEST-0328.**

### PR-REQ-0059 - Trusted Hook duty

A trusted Hook is responsible for entering recovery risk before a relevant
external mutation. Pactrun MUST NOT claim that native host-process behavior is
contained by an operating-system sandbox.

**Verification: PR-TEST-0328.**

The real native Hook fixture waits for risk acknowledgment before its external
write and that write survives Pactrun recovery. This verifies the supported
trust boundary, not confinement or enforcement against an arbitrary bad Hook.

## Durable recovery state

### PR-REQ-0060 - Single execution owner

Every Running Run MUST have exactly one execution owner. Reconciliation MUST NOT
mark a Run interrupted until owner loss is established by the selected ownership
mechanism.

**Verification: PR-TEST-0100, PR-TEST-0111, PR-TEST-0325.**

### PR-REQ-0277 - Action execution owner mechanism

The selected ownership mechanism for an Action Run is the accepting process's
staging session lease: the exclusive operating-system file lock that the
process holds on `staging/session-<hex>/.lease` below the dedicated Pactrun
storage root for its whole lifetime. The durable Run execution record MUST
store the exact staging session directory name of that owner, and a Running Run
MUST have exactly one such record.

Owner loss MUST be confirmed only by observing that the recorded session's lease
is not held: the lease file can be locked by the observer, or the session
directory no longer exists. A held lease MUST never be interpreted as loss, and
a timestamp, heartbeat age, process identifier, or hostname MUST NOT substitute
for the lock observation. The owner record MUST be removed only by the terminal
publication of the Run, so a Run whose owner process disappeared remains
Running with its stale owner record until reconciliation confirms the loss and
finishes it.

Managed execution types use this ownership mechanism rather than introducing
parallel definitions of owner loss.

**Verification: PR-TEST-0086, PR-TEST-0111, PR-TEST-0325.**

### PR-REQ-0061 - Plan is not a replay contract

An Execution Plan MUST remain ephemeral. Recovery MUST NOT require durable Plan
serialization and MUST NOT recompile the original request, resume workflow
steps, replay a Hook, or require globally deterministic Hook behavior.

**Verification: PR-TEST-0304, PR-TEST-0317, PR-TEST-0326.**

### PR-REQ-0062 - Self-sufficient RunRecoveryState

Before a recovery-relevant side effect can occur, Pactrun MUST durably maintain
a `RunRecoveryState` that identifies the last committed boundary, risk state,
Pactrun-owned recovery action, resulting Instance consequence, recovery
references, and sufficient diagnostics. A reconciler MUST be able to use this
state without the Plan.

**Verification: PR-TEST-0304, PR-TEST-0317, PR-TEST-0326.**

### PR-REQ-0063 - Atomic boundary advancement

A new authoritative Instance state, its new `InstanceStateVersion`, and the
corresponding recovery boundary and disposition MUST be published atomically.
Pactrun MUST NOT expose a new Instance commit with recovery state still pointing
at an earlier boundary. This atomicity is limited to Pactrun-owned authoritative
state and recovery records. It MUST NOT be described as a single atomic
transaction with a service filesystem, database, Docker volume, or external
resource. ServiceStorage-backed Managed Service Resource continuity and retention
are represented under the [ServiceStorage execution](../instances/service-storage.md)
and [persistence](../persistence/persistence-baseline.md) contracts.

**Verification: PR-TEST-0085, PR-TEST-0326, PR-TEST-0330.**

### PR-REQ-0246 - Service transformation target-publication boundary

For a Revision transition that transforms a ServiceStorage-backed Managed
Service Resource across a service-owned coherence boundary, the durable recovery
risk MUST remain `Open` after the service reaches target coherence until Pactrun
can publish the target Pactrun-owned boundary. Target coherence by itself MUST
NOT clear the durable risk or change the committed Revision.

The target Revision, staged Managed Input state and disposition, new
`InstanceStateVersion`, corresponding recovery boundary, and durable risk clear
MUST be published as one Pactrun-owned target boundary. This atomic publication
MUST NOT include the service-owned bytes or presume a persisted resource
association, continuity, retention, or discard representation. A supporting
Hook Protocol or runtime coordination mechanism MUST acknowledge risk
resolution consistently with this ordering. The current
[Hook Protocol](../interfaces/hook-protocol.md) defines the target proposal and
publication acknowledgement.

If execution is lost after the service may have reached target coherence but
before that Pactrun-owned publication succeeds, the committed source boundary
remains authoritative, risk remains `Open`, the Run MAY become `Interrupted`,
and the Instance MUST enter `ManualRecoveryRequired`. That guard means Pactrun
cannot assert that the service-owned state matches the committed source; it does
not authorize Hook replay, compensation, or service-state rollback.

**Verification: PR-TEST-0377, PR-TEST-0378, PR-TEST-0380, PR-TEST-0381, PR-TEST-0382.**

### PR-REQ-0064 - Orphan reconciliation

After confirmed owner loss, a reconciler MUST read only durable recovery state,
perform only authorized Pactrun-owned recovery, apply the materialized Instance
consequence, and finish the Run as `Interrupted`.

**Verification: PR-TEST-0112, PR-TEST-0325, PR-TEST-0330.**

### PR-REQ-0065 - Recovery-reference lifetime

Recovery references MUST remain strong GC roots while reconciliation or an
unresolved recovery obligation may need them. Execution-only recovery state MAY
be removed or compacted only after terminal reconciliation and after no managed
object requires the references.

**Verification: PR-TEST-0085, PR-TEST-0327, PR-TEST-0423, PR-TEST-0428.**

## ManualRecoveryRequired

### PR-REQ-0066 - Trust-guard meaning

`ManualRecoveryRequired` MUST mean that Pactrun-owned state is internally
consistent but Pactrun cannot assert that service-owned state matches it. It
MUST be an Instance trust guard, not a Run outcome, persistence-corruption
marker, or synonym for a failed Run.

**Verification: PR-TEST-0084, PR-TEST-0325.**

### PR-REQ-0067 - Guard behavior

While an Instance is in `ManualRecoveryRequired`, ordinary managed executions
MUST be blocked by default. Read-only inspection and otherwise legal
Pactrun-owned Input management MAY continue without clearing the guard.

**Verification: PR-TEST-0091, PR-TEST-0327, PR-TEST-0329.**

### PR-REQ-0068 - One-execution recovery override

An explicit recovery override MAY bypass the trust guard for one managed
execution. It MUST NOT clear the guard or bypass exact compatibility,
operation-specific requirements, stale-plan checks, exact references, Secret
declassification authorization, mutation conflicts, or other invariants.

**Verification: PR-TEST-0091, PR-TEST-0329.**

### PR-REQ-0069 - ResolveManualRecovery

`ResolveManualRecovery` MUST be an explicit operator assertion implemented as a
no-Hook, no-Compiler, no-Run management mutation. It MUST acquire the mutation
guard, clear the trust guard, and publish a new Instance state version. Pactrun
MUST NOT infer resolution by running an Action with a particular name.

**Verification: PR-TEST-0088, PR-TEST-0327, PR-TEST-0330.**

### PR-REQ-0070 - Automatic recovery resolution

A successful ordinary Action, Capture, or Migration performed with an override
MUST NOT automatically clear manual recovery. A successfully committed
exact-compatible Snapshot Restore with Clear recovery risk MUST clear an
existing manual-recovery guard in the same authoritative terminal transaction,
subject to the publication checks in
[PR-REQ-0291](../snapshots/execution.md#pr-req-0291---restore-authority-and-guarded-publication).
A failed, cancelled, timed-out, or interrupted Restore MUST NOT clear a
pre-existing guard. Successful Instance deletion removes the object; the
[retirement contract](./retirement.md) defines its finalization rules.

**Verification: PR-TEST-0259, PR-TEST-0260, PR-TEST-0264, PR-TEST-0306, PR-TEST-0329, PR-TEST-0370.**

### PR-REQ-0071 - Recovery provenance

An unresolved manual-recovery obligation MUST retain durable trigger and reason
information sufficient for operator diagnosis without depending on an
ephemeral Execution Plan.

**Verification: PR-TEST-0085, PR-TEST-0327, PR-TEST-0330.**
