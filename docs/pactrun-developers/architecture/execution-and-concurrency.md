---
title: Execution and Concurrency
---

# Execution and Concurrency

**Status: Normative architecture.**

## Operation classification

### PR-REQ-0035 - Management operations

Install and import, resource inspection, Instance creation, Input management,
Snapshot and Revision import/export/delete, and `ResolveManualRecovery` MAY be
handled directly by Application and Domain services. Instance mutations MUST
still use the mutation guard and atomically publish a new state version. A
management operation MUST NOT create an Execution Plan or Run.

**Verification: Pending automated coverage.**

### PR-REQ-0036 - Managed executions

Action invocation, Snapshot Capture, Snapshot Restore, Revision transition, and
Instance deletion MUST be managed executions. After resolution and compilation
succeed and a compiled Plan is accepted as an execution attempt, Pactrun MUST
create a durable Run before evaluating admission. Successful admission MUST
continue execution in that same Run.

**Verification: Pending automated coverage.**

## Resolution and compilation

### PR-REQ-0037 - Closed resolved intent

The Compiler input operation MUST be a closed sum of `InvokeAction`,
`CaptureSnapshot`, `RestoreSnapshot`, `TransitionRevision`, and
`DeleteInstance`, with deletion distinguishing `ManagedCleanup` from
`AbandonManagement`.

**Verification: Pending automated coverage.**

### PR-REQ-0038 - Resolver ownership

Before compilation, the Application Resolver MUST convert human references to
exact identities, reject ambiguity, parse and normalize parameters, and parse
explicit security-sensitive authorization intent. The Compiler MUST NOT depend
on CLI spelling, tags, `latest`, or raw argument syntax.

**Verification: Pending automated coverage.**

### PR-REQ-0039 - Side-effect-free compilation

Compilation MUST be side-effect free. It MUST NOT acquire a long-lived lock,
start a Hook, mutate an Instance, create a Run, or create a lease, pin, GC root,
or reservation. It SHOULD reject every statically detectable error before the
first workflow side effect.

**Verification: Pending automated coverage.**

### PR-REQ-0040 - Typed immutable plan

An Execution Plan MUST be immutable, typed, and sequential-first. It MUST fix an
exact `InstanceId`, expected `InstanceStateVersion`, operation requirements,
exact managed references, Hook Session authority, steps, recovery directives,
and commit semantics. The initial implementation MUST NOT introduce a generic
DAG engine.

**Verification: Pending automated coverage.**

### PR-REQ-0041 - Plan references are not pins

Exact references in a compiled Plan MUST NOT reserve resource lifetime. Only an
accepted Run may establish durable execution pins. A compile-only plan MUST NOT
create a Run, lock, lease, pin, GC root, or reservation and MAY become stale.

**Verification: Pending automated coverage.**

## Admission

### PR-REQ-0042 - Admission linearization

After the Run is created at the execution-acceptance boundary, Admission MUST
acquire the mutation guard when needed, compare the current Instance state
version with the Plan token, validate exact resources and all remaining
preconditions, and atomically establish durable pins before the first workflow
side effect.

**Verification: Pending automated coverage.**

### PR-REQ-0043 - Plan invalidation

If the state token, exact references, policy, environment, or operation
preconditions fail at admission, the existing Run MUST report
`PlanInvalidated`. The Executor MUST NOT silently re-resolve or recompile the
workflow.

**Verification: Pending automated coverage.**

### PR-REQ-0044 - Accepted execution continuity

An accepted Run MUST NOT invalidate itself when its own committed Migration edge
or Restore advances Instance state. After initial admission, correctness MUST be
maintained by pins, the mutation guard, and durable commit and recovery
boundaries.

**Verification: Pending automated coverage.**

## Concurrency and pins

### PR-REQ-0045 - Observe and Mutate access

Pactrun MUST model access as `Observe` or `Mutate`. Observe executions MAY
coexist with other Observe and Mutate operations. Mutate operations on the same
Instance MUST serialize or conflict. The same mutation guard MUST protect both
managed execution and management mutations.

**Verification: Pending automated coverage.**

### PR-REQ-0046 - Immutable admitted context

An admitted execution MUST continue to observe the exact Revision, Inputs,
Secrets, and managed content pinned at admission. Later replacement or deletion
from current Instance state MUST NOT alter the running context.

**Verification: Pending automated coverage.**

### PR-REQ-0047 - Durable execution pins

Execution pins MUST be durable rather than process-memory-only. A pinned
Revision or payload MUST remain available across process failure until the Run
is terminal and no execution or recovery reference remains.

**Verification: Pending automated coverage.**

### PR-REQ-0048 - Pin and guard separation

An `ExecutionPinSet` MUST be internal lifetime bookkeeping and MUST NOT be
treated as a mutation guard or user-visible lease. Releasing a current binding
MAY make it unreachable from the Instance, but physical content MUST remain
until its last strong reference disappears.

**Verification: Pending automated coverage.**

## Run records

### PR-REQ-0049 - Run creation boundary

Resolution and compilation failures MUST NOT create a Run. Once a Plan is
accepted as an execution attempt, Pactrun MUST create a durable Run before
admission checks. Admission failures, including stale
`InstanceStateVersion`, missing exact references, and failed remaining
preconditions, and all later execution failures MUST be recorded in that Run.

**Verification: Pending automated coverage.**

### PR-REQ-0050 - Run phase and outcome

A Run MUST distinguish `Running` from `Finished`. A finished Run MUST use one of
`Succeeded`, `Failed`, `Cancelled`, `TimedOut`, or `Interrupted` and MAY retain
failure details, failed logical step, Hook result, diagnostics, outputs, and
timing. `ManualRecoveryRequired` MUST NOT be a Run outcome.

**Verification: Pending automated coverage.**

### PR-REQ-0051 - Failure detail ordering

The first failure that prevents intended workflow completion MUST be the
`PrimaryFailure`. Failures arising during cleanup, rollback, or failure handling
MUST be secondary unless no earlier failure exists and the cleanup failure itself
prevents completion.

**Verification: Pending automated coverage.**

### PR-REQ-0052 - Cancellation and timeout finalization

A cancellation request MUST NOT immediately mark a Run cancelled. Pactrun MUST
request cancellation, propagate it, observe Hook termination, perform required
finalization, and only then publish a terminal outcome. `TimedOut` MUST identify
termination caused by Pactrun policy.

**Verification: Pending automated coverage.**
