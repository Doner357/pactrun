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

## PR-REQ-0094 - Action invocation

Users MUST invoke a Package-defined Action through the managed execution path.
Pactrun MUST resolve the exact Instance and Action, normalize parameters,
validate readiness and policy, compile the workflow, and complete admission
before launching the Hook.

**Verification: Pending automated coverage.**

## PR-REQ-0095 - Plan preview

A managed execution command SHOULD offer a side-effect-free plan view showing
exact references, requirements, Migration paths, projected incomplete state,
Cleanup requirements, and warnings when applicable. Producing a plan MUST NOT
create a Run, reservation, lock, pin, or GC root.

**Verification: Pending automated coverage.**

## PR-REQ-0096 - Plan staleness

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

## PR-REQ-0097 - User-visible Run detail

Run inspection MUST distinguish terminal outcome from detailed failures and
from the Instance trust consequence. It MUST be able to report the primary
failure, secondary failure-handling errors, failed logical step, Hook result,
diagnostics, output references, and timing when available.

**Verification: Pending automated coverage.**

## PR-REQ-0098 - Sensitive Run data

Run records and ordinary diagnostics MUST NOT store sensitive parameter values,
Secret values, or value-derived digests intended to reveal them. Interactive
terminal sessions MUST NOT be retained as complete transcripts by default;
structured Hook diagnostics MAY be retained.

**Verification: Pending automated coverage.**

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
