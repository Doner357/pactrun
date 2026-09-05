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

In M2, Instance creation and Input set/delete are Mutate management operations;
Input export and list/show are Observe. Install publication uses its own
Revision transaction and does not acquire an Instance guard. The exact CAS and
Observe coexistence rules are PR-REQ-0267.

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

**Verification: PR-TEST-0089.**

### PR-REQ-0040 - Typed immutable plan

An Execution Plan MUST be immutable, typed, and sequential-first. It MUST fix an
exact `InstanceId`, expected `InstanceStateVersion`, operation requirements,
exact managed references, Hook Session authority, steps, recovery directives,
and commit semantics. The initial implementation MUST NOT introduce a generic
DAG engine. This requirement fixes Pactrun-owned authoritative context; it MUST
NOT be interpreted as an immutable snapshot or linearization claim over
service-owned live bytes. Future ServiceStorage-backed Managed Service Resource
planning representation remains a separate versioned design gate.

**Verification: Pending automated coverage.**

### PR-REQ-0041 - Plan references are not pins

Exact references in a compiled Plan MUST NOT reserve resource lifetime. Only an
accepted Run may establish durable execution pins. A compile-only plan MUST NOT
create a Run, lock, lease, pin, GC root, or reservation and MAY become stale.

**Verification: PR-TEST-0089.**

## Admission

### PR-REQ-0042 - Admission linearization

After the Run is created at the execution-acceptance boundary, Admission MUST
acquire the mutation guard when needed, compare the current Instance state
version with the Plan token, validate exact resources and all remaining
preconditions, and atomically establish durable pins before the first workflow
side effect.

**Verification: PR-TEST-0089.**

### PR-REQ-0043 - Plan invalidation

If the state token, exact references, policy, environment, or operation
preconditions fail at admission, the existing Run MUST report
`PlanInvalidated`. The Executor MUST NOT silently re-resolve or recompile the
workflow.

**Verification: PR-TEST-0090.**

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

**Verification: PR-TEST-0092.**

### PR-REQ-0278 - Action admission exclusivity

Action Admission MUST acquire the process-local mutation guard only for the
acceptance and Admission call and MUST NOT hold it across Hook execution. The
Mutate-conflict predicate and durable pin establishment MUST occur atomically
in the same Admission `BEGIN IMMEDIATE` transaction, so that two concurrent
Mutate Admissions on one Instance cannot both succeed in any process
arrangement.

The access mode of every Run the predicate considers, including the Run being
admitted, MUST be derived from the persisted Action declaration of that Run's
Revision: the active Revision for the Run being admitted and the pinned
Revision for a competing Run. A caller-supplied access flag MUST NOT be
correctness authority.

A Mutate Action Run conflicts only with another Mutate Action Run on the same
Instance that is both Running and Admitted. An Accepted Run that has not been
admitted occupies no exclusivity, so a process failure between acceptance and
Admission blocks nothing. Observe Action Runs never conflict in either
direction. Management mutations remain admissible while a Run is Running: the
same guard serializes them per transaction, and they cannot alter the Run's
pinned context. The refusal is reported as `admission.mutation_conflict`, and
the one-execution recovery override MUST NOT bypass it.

**Verification: PR-TEST-0092.**

### PR-REQ-0279 - Admission refusal precedence

Any check performed outside the Admission transaction is advisory only and MUST
NOT publish a refusal, pin, or outcome. Inside one Admission `BEGIN IMMEDIATE`
transaction Admission MUST evaluate the following in order and MUST publish the
first refusal that applies as the Run's terminal `Failed` outcome, with the
`PrimaryFailure` recorded at the Admission step, in that same transaction:

1. the Instance is in `ManualRecoveryRequired` and no explicit one-execution
   override was supplied: `admission.recovery_guard_active`;
2. a stale compilation or state fact: the expected `InstanceStateVersion`, an
   exact Revision or binding reference, readiness, interpreter launcher
   re-selection, or runtime-content availability: `admission.plan_invalidated`;
3. after successful revalidation, a conflicting Running and Admitted Mutate
   Action Run: `admission.mutation_conflict`.

Only when no refusal applies MUST Admission establish durable pins and make the
Run Admitted, in the same transaction. Human-readable refusal messages are
diagnostics and carry no normative content.

**Verification: PR-TEST-0091.**

### PR-REQ-0267 - M2 Instance CAS and mutation guard

Every typed M2 Instance mutation request MUST carry one exact expected
`InstanceStateVersion`. Inside the write transaction Pactrun MUST compare the
current token before evaluating desired state. A mismatch is always a stale
conflict, even when the requested desired binding already exists. When the
token matches, Pactrun validates and applies the mutation. Every actual
publication MUST generate a fresh opaque token from the operating-system
cryptographic random source; a semantic no-op MUST leave the token unchanged.

This is token-first Instance concurrency and MUST NOT be implemented with the
M1-D desired-first semantic CAS rule. Pactrun MUST NOT automatically refresh and
retry a typed request, create a request-idempotency key, or use current field
equality as mutation history. Returning to earlier field values still publishes
a new token, preventing ABA for Pactrun-authoritative Instance state.

M2 MUST provide per-Instance Mutate exclusivity: management mutations on the
same Instance serialize or conflict. Observe operations, including
`ExportInput`, MUST NOT acquire a reader guard and MAY coexist with Mutate.
Their exact-byte lifetime is provided by transient observation and staging, not
by holding the mutation guard. SQLite token comparison is the cross-process
correctness boundary. M2 MUST NOT create M3 durable execution pins, Run
ownership, or Action Admission state.

**Verification: PR-TEST-0076, PR-TEST-0077.**

### PR-REQ-0046 - Immutable Pactrun-owned admitted context

An admitted execution MUST continue to observe the exact Revision, Inputs,
Secrets, and Pactrun-owned managed content pinned at admission. Later
replacement or deletion from current Instance state MUST NOT alter that pinned
context.

Service-owned bytes of a ServiceStorage-backed Managed Service Resource are
explicitly outside this immutable binding-snapshot guarantee. The service may
mutate live contents while an execution is running, and Pactrun MUST NOT treat an
`InstanceStateVersion`, mutation guard, or execution pin as proof that those
bytes were frozen or linearized. How a future Plan binds resource declarations,
associations, and operation prerequisites remains design work.

**Verification: PR-TEST-0094.**

### PR-REQ-0047 - Durable execution pins

Execution pins MUST be durable rather than process-memory-only. A pinned
Revision or payload MUST remain available across process failure until the Run
is terminal and no execution or recovery reference remains. This Pactrun-owned
content lifetime guarantee does not content-address, copy, or freeze a
ServiceStorage-backed Managed Service Resource's live bytes.

**Verification: PR-TEST-0083, PR-TEST-0100.**

### PR-REQ-0048 - Pin and guard separation

An `ExecutionPinSet` MUST be internal lifetime bookkeeping and MUST NOT be
treated as a mutation guard or user-visible lease. Releasing a current binding
MAY make it unreachable from the Instance, but physical content MUST remain
until its last strong reference disappears.

**Verification: PR-TEST-0083.**

## Run records

### PR-REQ-0049 - Run creation boundary

Resolution and compilation failures MUST NOT create a Run. Once a Plan is
accepted as an execution attempt, Pactrun MUST create a durable Run before
admission checks. Admission failures, including stale
`InstanceStateVersion`, missing exact references, and failed remaining
preconditions, and all later execution failures MUST be recorded in that Run.

**Verification: PR-TEST-0089.**

### PR-REQ-0050 - Run phase and outcome

A Run MUST distinguish `Running` from `Finished`. A finished Run MUST use one of
`Succeeded`, `Failed`, `Cancelled`, `TimedOut`, or `Interrupted` and MAY retain
failure details, failed logical step, Hook result, diagnostics, outputs, and
timing. `ManualRecoveryRequired` MUST NOT be a Run outcome.

**Verification: PR-TEST-0084.**

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

**Verification: PR-TEST-0097.**
