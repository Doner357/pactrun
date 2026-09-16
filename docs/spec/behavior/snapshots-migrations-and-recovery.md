---
title: Snapshots, Migration, and Recovery
---

# Snapshots, Migration, and Recovery

**Status: Normative product behavior specification.**

<!-- spec-navigation:start -->
## Reading map (informative)

Compare observable Snapshot, Migration, and recovery behavior. Check the implementation record before treating specified future operations as available.

Start with the [specification map](../index.md)
and [shared vocabulary](../glossary.md) if a term is unfamiliar.
Check [implementation status and remaining decisions](../../development/next-milestone.md)
before treating an approved contract as available runtime behavior.
The original status, rules, exceptions, and verification declarations below retain their meaning.
<!-- spec-navigation:end -->

## Snapshots

A Snapshot is an immutable recovery object produced under one exact Revision.
It contains the complete managed binding state, including active and retained
bindings, optional absence, protection metadata, and Package-produced service
recovery content. It is not an Instance clone and does not contain Run history
or old binding history. Pactrun does not automatically scan `ServiceStorage` or
mirror every ServiceStorage-backed Managed Service Resource into a Snapshot. The
Capture Hook selects and transforms recovery-relevant service-owned state at an
appropriate service consistency boundary before Pactrun validates, hashes, and
commits it.

### PR-REQ-0099 - Snapshot operations

Pactrun MUST provide management and execution operations to create, list, show,
restore, export, import, and delete Snapshots. Import and export MUST NOT launch
a Hook, compile a workflow, or create a Run. Snapshot import MUST NOT require a
target Instance.

**Verification: PR-TEST-0268.**

### PR-REQ-0100 - Capture consistency

Capture MUST use one admitted binding view for both the Capture Hook context and
the Snapshot's managed binding state. It MUST include retained bindings even
though they are not visible to the ordinary Hook context.

**Verification: PR-TEST-0246, PR-TEST-0249.**

### PR-REQ-0101 - Direct Restore compatibility

Restore MUST require the Snapshot producer `RevisionIdentity` to equal the
target Instance's active `RevisionIdentity`, including both Package ID and
Revision content digest. Successful import MUST NOT imply Restore compatibility.

**Verification: PR-TEST-0274.**

### PR-REQ-0102 - Restore replacement

Restore MUST preserve the target Instance identity and name. It MUST stage the
Snapshot's complete managed binding state, give the Restore Hook the same active
view that will be committed, and replace rather than merge the target's earlier
bindings after success.

**Verification: PR-TEST-0258, PR-TEST-0268.**

### PR-REQ-0103 - Snapshot provenance and lifetime

Origin Instance information MUST be provenance rather than ownership. A
Snapshot MAY restore to another exact-compatible Instance and MAY outlive its
origin Instance, producer Revision installation, and creator Run.

**Verification: PR-TEST-0275, PR-TEST-0421.**

### PR-REQ-0104 - Sensitive Snapshot export

Capture MUST include Secrets automatically as managed recovery state. Exporting
a Snapshot containing Secrets MUST require explicit sensitive-data
authorization and clear handling warnings. Ordinary Snapshot inspection MUST
NOT reveal Secret values or value-derived digests.

**Verification: PR-TEST-0268.**

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

### ServiceStorage-backed resource continuity

The existing `Carry`, `Keep`, `Discard`, and `Declassify` model applies to
Pactrun-authoritative Managed Input Bindings. It is not a Service Resource
transition schema. This section does not classify Docker volumes, external
databases, remote objects, or other non-ServiceStorage-backed service state.

Compatible source and target Revisions use the same Instance persistent live
resource without copying or rematerializing its bytes. A source-only resource
is retained conservatively, and destructive removal must be explicit. If a path
or representation changes, a resource splits or merges, or another
service-specific transformation is required, target-owned inbound Migration
semantics and a Migration Hook own the transformation and use the existing
recovery-risk handshake.

This policy does not define a retained-resource registry or any other durable
representation. Compatibility, continuity, retention, discard, association,
access, prerequisites, target-commit coordination, and persistence ownership
remain future-version design gates.

## Failure and recovery

### PR-REQ-0109 - Failure is not manual recovery

A failed, cancelled, timed-out, or interrupted Run MUST NOT by itself place an
Instance in `ManualRecoveryRequired`. That consequence occurs only when durable
recovery state says an unresolved recovery-risk boundary was open. If execution
is lost during service-owned transformation while risk is open, recovery of the
Pactrun-owned committed boundary does not prove that service state remains
coherent with the source Revision.

**Verification: PR-TEST-0085.**

### PR-REQ-0110 - Recovery options

An Instance in `ManualRecoveryRequired` MUST allow inspection, legal Input
management, explicit one-execution override, `ResolveManualRecovery`,
exact-compatible Restore, and `AbandonManagement` according to their separate
preconditions. Ordinary managed execution remains blocked by default.

**Verification: Pending automated coverage.**

## Instance deletion

### PR-REQ-0111 - Normal managed deletion

If the active Revision defines Cleanup, normal deletion MUST run Cleanup and
MUST NOT proceed to Pactrun-provided storage-lifetime finalization until Cleanup
has reported success with clear recovery risk and Pactrun has durably published
the Cleanup-completed boundary defined by PR-REQ-0247. With or without a Cleanup
capability, successful normal deletion requires the Pactrun-provided Instance
storage lifetime to have ended before Pactrun removes the Instance state.

**Verification: PR-TEST-0404, PR-TEST-0408.**

### PR-REQ-0112 - Cleanup failure result

Missing Cleanup requirements MUST prevent Hook launch and leave the Instance
unchanged. A terminal Cleanup non-success with clear risk MUST retain a Normal
Instance. Open risk MUST retain the Instance in `ManualRecoveryRequired`.
Pactrun MUST NOT create persistent `Deleting` or `DeletionFailed` states.

**Verification: PR-TEST-0405.**

### PR-REQ-0113 - AbandonManagement intent

`AbandonManagement` MUST be an explicit intent to destroy the Pactrun management
relationship, not a generic `--force` and not authorization to destroy service-
owned state. It MUST skip Package Cleanup and remove Pactrun-owned Instance
management state even when Cleanup is unavailable or the Instance requires
manual recovery. It MUST warn that service-owned resources, files, processes,
or credentials may remain. It MUST remain a managed execution with Run history
that distinguishes explicit abandonment from successful managed cleanup.

**Verification: PR-TEST-0406.**

### PR-REQ-0247 - Cleanup finalization and abandonment

A Cleanup Hook success with clear recovery risk MUST NOT by itself be treated as
a durable Cleanup-completed boundary. Before beginning Pactrun-provided storage-
lifetime finalization, Pactrun MUST durably publish that Cleanup completed and
MUST NOT be replayed. Only after that publication MAY Pactrun perform its own
storage-lifetime finalization, and normal deletion MUST NOT succeed until the
provided storage lifetime has ended.

The Frozen completion rule in
[PR-REQ-0216](../contracts/hook-protocol-v1.md#pr-req-0216---operation-completion-and-terminal-states)
remains authoritative: loss before accepted completion prevents protocol
success, Pactrun MUST NOT infer replay or compensation, and
`completion_accepted` does not itself imply a Run or Instance commit. Therefore,
loss after a Hook sends Cleanup success but before Pactrun durably publishes the
Cleanup-completed boundary MUST NOT be treated as proof that replay is safe or
that Cleanup durably completed. Exact coordination of that ambiguous window
remains a future Hook Protocol and recovery design gate; this requirement adds
no replay, compensation, manual-recovery, or finalization inference.

After the durable Cleanup-completed boundary exists, a crash or failure during
Pactrun-provided storage-lifetime finalization MUST retry or resume only that
Pactrun-owned finalization obligation and MUST NOT replay Cleanup. Until it
finishes, deletion MUST NOT report success or allow ordinary management to
bypass the obligation. The obligation MUST NOT require a public persistent
`Deleting` or `DeletionFailed` Instance state; its representation and retry
mechanism remain future persistence and runtime design.

`AbandonManagement` MUST NOT destructively remove the abandoned service-owned
state during abandonment and MUST NOT authorize later ordinary garbage
collection, unreferenced-storage cleanup, or maintenance to remove it. The
durable non-destruction representation, later discoverability, operator handoff,
and explicit discard remain future persistence and runtime design gates. This
requirement introduces no orphan-storage or retained-resource registry.

**Verification: PR-TEST-0407, PR-TEST-0408, PR-TEST-0409, PR-TEST-0410, PR-TEST-0411, PR-TEST-0412, PR-TEST-0413, PR-TEST-0414, PR-TEST-0433, PR-TEST-0445.**
