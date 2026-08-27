---
title: Snapshots, Migration, and Recovery
---

# Snapshots, Migration, and Recovery

**Status: Normative product behavior specification.**

## Snapshots

A Snapshot is an immutable recovery object produced under one exact Revision.
It contains the complete managed binding state, including active and retained
bindings, optional absence, protection metadata, and Package-produced service
recovery content. It is not an Instance clone and does not contain Run history
or old binding history.

### PR-REQ-0099 - Snapshot operations

Pactrun MUST provide management and execution operations to create, list, show,
restore, export, import, and delete Snapshots. Import and export MUST NOT launch
a Hook, compile a workflow, or create a Run. Snapshot import MUST NOT require a
target Instance.

**Verification: Pending automated coverage.**

### PR-REQ-0100 - Capture consistency

Capture MUST use one admitted binding view for both the Capture Hook context and
the Snapshot's managed binding state. It MUST include retained bindings even
though they are not visible to the ordinary Hook context.

**Verification: Pending automated coverage.**

### PR-REQ-0101 - Direct Restore compatibility

Restore MUST require the Snapshot producer `RevisionIdentity` to equal the
target Instance's active `RevisionIdentity`, including both Package ID and
Revision content digest. Successful import MUST NOT imply Restore compatibility.

**Verification: Pending automated coverage.**

### PR-REQ-0102 - Restore replacement

Restore MUST preserve the target Instance identity and name. It MUST stage the
Snapshot's complete managed binding state, give the Restore Hook the same active
view that will be committed, and replace rather than merge the target's earlier
bindings after success.

**Verification: Pending automated coverage.**

### PR-REQ-0103 - Snapshot provenance and lifetime

Origin Instance information MUST be provenance rather than ownership. A
Snapshot MAY restore to another exact-compatible Instance and MAY outlive its
origin Instance, producer Revision installation, and creator Run.

**Verification: Pending automated coverage.**

### PR-REQ-0104 - Sensitive Snapshot export

Capture MUST include Secrets automatically as managed recovery state. Exporting
a Snapshot containing Secrets MUST require explicit sensitive-data
authorization and clear handling warnings. Ordinary Snapshot inspection MUST
NOT reveal Secret values or value-derived digests.

**Verification: Pending automated coverage.**

## Revision Migration

### PR-REQ-0105 - Declared Migration path

Migration MUST follow exact inbound edges declared by target Revisions within
the same Package lineage. Pactrun MUST NOT invent an undeclared direct edge.

**Verification: Pending automated coverage.**

### PR-REQ-0106 - Chained progress

When a path contains multiple edges, each edge MUST be an independent durable
commit boundary. If a later edge fails, the Instance MUST remain at the last
successfully committed intermediate Revision.

**Verification: Pending automated coverage.**

### PR-REQ-0107 - Incomplete Migration result

A successful Migration MAY leave the target Instance incomplete. This MUST be
reported as configuration readiness, not failure or manual recovery. A chain
MAY continue through an incomplete intermediate Revision when the next edge's
own requirements are satisfied.

**Verification: Pending automated coverage.**

### PR-REQ-0108 - Declassification authorization

A Secret-to-Normal transition MUST be explicitly declared by the Migration edge
and explicitly authorized by the operator for that Migration. Users MAY instead
retain or discard the old Secret and provide a new Normal binding.

**Verification: Pending automated coverage.**

## Failure and recovery

### PR-REQ-0109 - Failure is not manual recovery

A failed, cancelled, timed-out, or interrupted Run MUST NOT by itself place an
Instance in `ManualRecoveryRequired`. That consequence occurs only when durable
recovery state says an unresolved recovery-risk boundary was open.

**Verification: Pending automated coverage.**

### PR-REQ-0110 - Recovery options

An Instance in `ManualRecoveryRequired` MUST allow inspection, legal Input
management, explicit one-execution override, `ResolveManualRecovery`,
exact-compatible Restore, and `AbandonManagement` according to their separate
preconditions. Ordinary managed execution remains blocked by default.

**Verification: Pending automated coverage.**

## Instance deletion

### PR-REQ-0111 - Normal managed deletion

If the active Revision defines Cleanup, normal deletion MUST run Cleanup and
remove the Instance only after Hook success with clear recovery risk. Without a
Cleanup capability, normal deletion MAY remove Pactrun-owned Instance state
directly.

**Verification: Pending automated coverage.**

### PR-REQ-0112 - Cleanup failure result

Missing Cleanup requirements MUST prevent Hook launch and leave the Instance
unchanged. A terminal Cleanup non-success with clear risk MUST retain a Normal
Instance. Open risk MUST retain the Instance in `ManualRecoveryRequired`.
Pactrun MUST NOT create persistent `Deleting` or `DeletionFailed` states.

**Verification: Pending automated coverage.**

### PR-REQ-0113 - AbandonManagement intent

`AbandonManagement` MUST be an explicit destructive intent, not a generic
`--force`. It MUST skip Package Cleanup and remove Pactrun-owned Instance state
even when Cleanup is unavailable or the Instance requires manual recovery. It
MUST warn that external resources, files, processes, or credentials may remain.
It MUST remain a managed execution with Run history that distinguishes explicit
abandonment from successful managed cleanup.

**Verification: Pending automated coverage.**
