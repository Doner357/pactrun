---
title: Resource Lifetimes
---

# Resource Lifetimes

## Resource lifecycle

Object lifetimes are independent: keeping Run history does not keep every
Artifact, and deleting an Instance does not delete its Snapshots. The
[managed-object lifecycle contract](./objects-gc.md)
defines explicit deletion, Artifact delivery, and foreground collection.

### PR-REQ-0072 - Snapshot lifetime

A committed Snapshot MUST be a durable first-class object with no default
expiry. Instance and Revision deletion MUST NOT
cascade-delete Snapshots, and Snapshot content MUST NOT depend on the creating
Run or Run Artifact remaining available.

**Verification: PR-TEST-0421.**

### PR-REQ-0073 - Run, Artifact, and workspace lifetime

A Run Artifact MUST belong to its Run and MAY expire independently of retained
Run metadata. A Run Record MAY have a separate retention policy. Workspaces and
uncommitted Snapshot Candidates MUST be execution-scoped; their uncommitted content is cleaned up after execution terminates.

Action Workspaces MUST remain execution-scoped. For an Action, execution
termination means that the supervised Hook execution has ended and required
process-tree termination has been observed. A path on which no Hook process was
started does not require a Hook-termination event.

Direct-child exit, accepted Hook completion, and stdout/stderr EOF are not
substitutes for observed process-group or Job termination. The owner retains
execution resources and cancellation supervision while descendants remain.

**Verification: PR-TEST-0684.**

Execution termination is distinct from durable Run terminal publication: the
transaction that publishes the Finished Run outcome and its associated
authoritative records. “After terminal execution” MUST NOT be interpreted as
requiring Workspace cleanup to wait until that transaction has committed.

Once execution has terminated and output preparation no longer needs the
execution Workspace, Pactrun MUST attempt Workspace cleanup. The attempt MAY
precede durable Run terminal publication. Successful cleanup MUST NOT be a
prerequisite for terminal publication.

Cleanup failure MAY leave non-authoritative residue. It MUST NOT change the Run
outcome, create or reopen recovery risk, create a manual-recovery obligation,
or prevent terminal publication. Residue remains eligible for subsequent
owner-session teardown or confirmed-owner-loss housekeeping; it MUST NOT become
managed content merely because it remains on disk.

After confirmed owner loss, orphaned Action Workspace bytes are cleanup-eligible
under the selected ownership mechanism. Their cleanup MUST NOT recover or
publish uncommitted output slots.

**Verification: PR-TEST-0106, PR-TEST-0110, PR-TEST-0421.**

### PR-REQ-0282 - Execution-workspace housekeeping failures

Cleanup is non-authoritative housekeeping and MUST NOT determine Action
success. A cleanup failure MUST NOT change an already-selected outcome,
replace a primary failure, open recovery risk, create a manual-recovery
obligation, or prevent terminal publication. An error observed before
terminalization MUST be recorded as a secondary failure under
[PR-REQ-0051](../operations/execution.md#pr-req-0051---failure-detail-ordering).

This cleanup MUST be limited to the current Run's execution tree and MUST
protect other Runs, the owner lease, and prepared output staging. It MUST NOT
create a durable cleanup queue or extend execution pins. Later housekeeping
MUST NOT rewrite an existing terminal outcome.

**Verification: PR-TEST-0110.**

### PR-REQ-0074 - Checkpoint and pin lifetime

Transition checkpoints and durable recovery state MUST remain until no recovery
or reference obligation exists. Execution-only pins MAY be released only after
the Run is terminal and no recovery state needs them.

**Verification: PR-TEST-0423, PR-TEST-0428.**

### PR-REQ-0075 - Revision deletion guards

Pactrun MUST reject deletion of a Revision referenced by an active Instance or
durably pinned by an accepted Run. A compile-only Plan MUST NOT prevent
deletion. Historical Run identity and Snapshot provenance MUST NOT permanently
prevent Revision deletion.

The [Persistence baseline](../persistence/persistence-baseline.md) represents
the active-Instance guard with an exact `ON DELETE RESTRICT` foreign key. The
[Instance retirement contract](./retirement.md) separately
owns the deletion workflow; a relational guard does not perform Cleanup.

**Verification: PR-TEST-0459, PR-TEST-0465, PR-TEST-0470.**

### PR-REQ-0076 - Physical content reachability

Physical GC MUST NOT remove content reachable from any managed object,
Snapshot, accepted-Run pin, checkpoint, or recovery state. GC MUST use strong
references or reachability rather than interpreting operation names or service
semantics. This rule governs Pactrun-owned content stores; it does not authorize
Pactrun to delete service-owned live resources. In particular, abandonment does
not make ServiceStorage-backed state collectible as unreferenced storage. The
non-destruction policy in PR-REQ-0247 and its durable implementation in the
[retirement contract](./retirement.md) preserve that boundary.

**Verification: PR-TEST-0461, PR-TEST-0463, PR-TEST-0464, PR-TEST-0467, PR-TEST-0469, PR-TEST-0471, PR-TEST-0474.**
