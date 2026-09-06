---
title: Actions, Plans, and Runs
---

# Actions, Plans, and Runs

**Status: Normative product behavior specification.**

## Actions

An Action is a Package-defined operation such as `start`, `status`, `logs`, or
`vacuum`. Pactrun does not infer service semantics from the name. An Action named
`backup`, for example, does not create a Pactrun Snapshot unless the Package
also uses the Snapshot capability.

### PR-REQ-0094 - Action invocation

Users MUST invoke a Package-defined Action through the managed execution path.
Pactrun MUST resolve the exact Instance and Action, normalize parameters,
validate readiness and policy, compile the workflow, and complete admission
before launching the Hook.

**Verification: Pending automated coverage.**

### PR-REQ-0095 - Plan preview

A managed execution command SHOULD offer a side-effect-free plan view showing
exact references, requirements, Migration paths, projected incomplete state,
Cleanup requirements, and warnings when applicable. Producing a plan MUST NOT
create a Run, reservation, lock, pin, or GC root.

**Verification: Pending automated coverage.**

### PR-REQ-0096 - Plan staleness

A displayed plan MUST be treated as a preview rather than a guarantee. Actual
execution MUST revalidate the expected Instance state version, exact resources,
policy, and environment. If they changed, execution MUST fail as invalidated
rather than silently changing the workflow.

**Verification: Pending automated coverage.**

## Run history

A Run begins when Pactrun accepts a compiled Plan as an execution attempt,
before admission checks. Resolution and compile errors therefore do not create
Runs. Admission invalidation and later execution failures are durable history
in that Run.

### PR-REQ-0097 - User-visible Run detail

Run inspection MUST distinguish terminal outcome from detailed failures and
from the Instance trust consequence. It MUST be able to report the primary
failure, secondary failure-handling errors, failed logical step, Hook result,
diagnostics, output references, and timing when available.

**Verification: Pending automated coverage.**

Slice 5 provides only the crate-private RunView, ordered Run enumeration, a
single-snapshot Run-plus-current-recovery-guard inspection substrate, and
independent Run Artifact streaming and expiry needed by a later inspection
surface. Supporting coverage is provided by `PR-TEST-0105`, `PR-TEST-0106`,
`PR-TEST-0107`, `PR-TEST-0109`, `PR-TEST-0110`, and the Slice 5 inspection
snapshot test; these tests do not establish user-visible inspection. Human
inspection and exact presentation remain deferred to Slice 6. This is
supporting coverage only and is not verification of `PR-REQ-0097`.

### PR-REQ-0281 - Atomic managed-output publication and late completion

Action Managed Output eligibility is determined by the protocol-accepted
completion and its valid submitted output handles, not by whether the Run
outcome is `Succeeded`.

If cancellation or timeout has already won outcome arbitration, but a valid
Action completion is subsequently protocol-accepted before process termination,
its valid submitted output handles MUST remain eligible for Managed Artifact
publication. The Run outcome MUST remain `Cancelled` or `TimedOut`,
respectively.

If no completion is protocol-accepted, no Action output slot may be published.
Preallocated slots, files written before cancellation, or an unaccepted
completion MUST NOT be treated as submissions.

Publication of the eligible submitted set is atomic at the Action-publication
level. Pactrun MUST validate and independently stage the entire set before
publishing it. If any required publication input cannot be validated, read, or
staged completely, none of that set may be published. Artifacts and the Run
terminal record MUST be published in the same V4 transaction. A publication
failure after a Hook-owned failure or after a locked `Cancelled` or `TimedOut`
outcome is secondary; a first publication failure on an otherwise successful
Run makes the Run `Failed` with `PublishDeclaredOutputs` as its primary step.

Database transaction failure retains owner-held retry state and MUST NOT be
converted into a fabricated terminal outcome. Eligibility does not require
submitting every declaration, and it does not guarantee persistence success.

**Verification: PR-TEST-0106, PR-TEST-0107, PR-TEST-0109.**

### PR-REQ-0098 - Sensitive Run data

Run records and ordinary diagnostics MUST NOT store sensitive parameter values,
Secret values, or value-derived digests intended to reveal them. Interactive
terminal sessions MUST NOT be retained as complete transcripts by default;
structured Hook diagnostics MAY be retained.

**Verification: PR-TEST-0099.**

## Concurrent operations

Long-running Observe Actions can continue with their admitted context while a
Mutate operation updates current Instance state. The running Action keeps its
old pinned view; subsequent operations see the newly committed state. Mutate
operations on the same Instance serialize or conflict.

This pinning model applies to the exact Revision and Pactrun-authoritative
managed context. It does not freeze or linearize service-owned live bytes. A
service may mutate a ServiceStorage-backed Managed Service Resource without
Pactrun observing the change or publishing a new `InstanceStateVersion`;
future authority and operation-prerequisite representations must account for
that ownership boundary.
