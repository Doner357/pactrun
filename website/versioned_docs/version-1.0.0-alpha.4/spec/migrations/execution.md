---
title: Migration Execution
---

# Migration Execution

This supplements the [Migration contract](./definition.md), shared
[execution](../operations/execution.md), and
[recovery](../lifecycle/recovery.md) rules. Frozen representations are
unchanged.

### PR-REQ-0303 - Exact Migration path selection

The resolver MUST use installed Revisions in one Package lineage and only
target-declared inbound edges with Valid exact-source relational validation.
It MUST reject zero-edge, repeated-node, or cross-lineage requests. Without a
selected path it MUST select the unique simple path and reject no-path requests.
When multiple paths exist, it MUST display candidates and decline execution
until the operator selects a path ID; it MUST NOT rank by readiness, length,
labels, installation order, or content to choose on the operator's behalf.
The resolver MUST expand a selected ID to the full ordered exact Revision
sequence before compilation. Each consecutive pair needs a declared edge.
No missing Revision may be fetched and no intermediate or edge invented.
NotEvaluated is not executable. Direct paths use the same selection mechanism
as chains; neither --direct nor human-authored --via is introduced.

**Verification: PR-TEST-0277, PR-TEST-0278, PR-TEST-0286, PR-TEST-0288,
PR-TEST-0290.**

### PR-REQ-0304 - Optional source absence and target writers

An absent requires_source MUST fail requirements. Otherwise Carry and Declassify
of an absent source MUST produce absence, while Keep and Discard MUST be no-ops.
Absence MUST NOT become an empty payload. Carry and Declassify declarations
reserve their target writer even when absent. Existing continuity/reactivation,
operator inputs, and Hook outputs MUST NOT introduce overwrite precedence.
requires_target must be present before its Hook; produces_target is mandatory
at successful completion, not a substitute for requires_target at admission.

**Verification: PR-TEST-0279, PR-TEST-0280, PR-TEST-0281, PR-TEST-0283, PR-TEST-0319.**

### PR-REQ-0305 - Whole-chain symbolic binding preflight

The Compiler MUST symbolically evaluate the full selected chain before workflow
side effects: source roles, presence requirements, writers, protection,
declared outputs, and supported available Hook runtime. Mandatory outputs MAY
establish symbolic presence for later edges, never inferred contents. Declared
produces_target without a Hook MUST fail compilation. RequiredInputsSatisfied
remains separate from requirements and completion; incomplete intermediate and
final Revisions are allowed. Retained bindings survive unless consumed or
discarded; roles remain relative to the current Revision over one registry.

Carry preserves the stored protection floor, including when the target has a
Normal declaration but the resulting binding remains effectively Secret under
PR-REQ-0034. That is not declassification. Only declared, operator-authorized
Declassify may lower the binding to Normal protection.

**Verification: PR-TEST-0280, PR-TEST-0281, PR-TEST-0282, PR-TEST-0283,
PR-TEST-0284, PR-TEST-0285.**

### PR-REQ-0306 - Accepted execution and edge publication {#pr-req-0306---m5-accepted-execution-and-edge-publication}

A chain MUST use one accepted Run, owner, and continuing Mutate exclusivity.
Initial Admission MUST revalidate the state token, exact resources, and actual
bindings, establishing durable pins before workflow side effects. The Run's own
commits MUST NOT invalidate it. Each edge isolates any staging; Hook edges use
independent Sessions. A declarative edge does not manufacture a Hook Session.

Each edge MUST atomically publish target Revision, binding state/disposition,
fresh InstanceStateVersion, committed-edge evidence, and recovery boundary.
Intermediate publication leaves Running; the final edge MUST publish Succeeded
in the same transaction. Later failure preserves the last committed boundary.
Staging files and Hook completion are not commit evidence. Recovery MUST use
durable evidence without a Plan or operator files; necessary references remain
strong roots. Confirmed owner loss becomes Interrupted without replay or auto
continuation. Existing Clear/Open risk and manual recovery rules apply, without
claims of service rollback or coherence.

**Verification: PR-TEST-0312, PR-TEST-0313, PR-TEST-0314, PR-TEST-0315,
PR-TEST-0316, PR-TEST-0317, PR-TEST-0321, PR-TEST-0322.**

Declarative edges follow PR-REQ-0313; Hook-backed edges additionally follow
PR-REQ-0316. Service transformations use the service-publication contract.

### PR-REQ-0313 - Declarative Migration Run and commit protocol

A no-Hook chain MUST use a typed Migration invocation with the complete exact
path and declassification intent, one accepted Run, and the accepting session's
lease. Rank 3 is decoded as Migration, never Action or Snapshot. Durable path
rows MUST be contiguous, same-lineage, non-repeating, and agree with invocation
endpoints. Inspection MUST validate progress/boundary/ref consistency and show
the number of committed edges and last committed Revision/state version without
exposing payload IDs or values. Historical boundary tokens are not current
Instance tokens after subsequent management or recovery-consequence changes.

Initial Admission MUST run under the serialized writer boundary, validate the
recorded owner, exact plan/invocation, trust guard, initial state token and full
binding registry, immutable edge declarations, and actual runtime/payload
availability. It establishes the common source Revision admission anchor, the
entire exact path's Revision pins, payload pins, and full current checkpoint in
one transaction. Refusal terminalizes that same Run with existing Admission
error identities. Operator acquisition follows PR-REQ-0315 and Hook invocation
follows PR-REQ-0316. Accepted-only Runs do not exclude other mutations.

An admitted Migration is Mutate for its entire chain. Other Mutate executions
must conflict through the shared predicate. Input set/delete and manual guard
resolution MUST check for an admitted Migration inside their token-first write
transaction and conflict rather than interleave with that chain. Observe
executions continue to coexist; their independent recovery consequences are
not erased. This Migration-specific exclusion does not replace the existing
optimistic Restore publication protocol.

Each edge publication MUST check the live owner, current committed edge cursor,
state token and clear risk. It evaluates only the already selected immutable
edge against the committed registry, resolving payload references after its own
earlier commits; it MUST NOT reselect or silently recompile a path. Declarative
steps skip Session/launch/completion and durably identify publication (step 3).
Changed protection creates a new immutable payload; Carry never downgrades its
source floor. The original payload remains unchanged for other observers/pins.

An edge's bindings/dispositions, target Revision, fresh state token, boundary
evidence and checkpoint advance MUST publish atomically. Intermediate edges
leave the Run Running; the final edge and Succeeded MUST be one transaction.
The shared generic finalizer MUST NOT independently claim Migration success.
Retries acknowledge already committed progress without repeating an edge.
Later cancellation or failure preserves committed intermediate progress.
Publication refusal (including a changed boundary or unresolved risk) reports
execution.migration_publication_rejected, with no staged target publication.

This operation's recovery directive preserves the already committed Instance;
it never writes back an older checkpoint, replays an edge, or infers service
compensation. At terminal publication/reconciliation, once that directive has
been applied, execution and checkpoint references are released together. The
Instance itself strongly owns the committed Revision and bindings. A service
trust guard may remain, and later explicitly authorized management is a new
state change, not replay or implicit history restoration. Boundary/progress
metadata remains inspectable after reference release. Open risk never permits
success or a false claim of service coherence.

**Verification: PR-TEST-0301, PR-TEST-0302, PR-TEST-0303, PR-TEST-0304,
PR-TEST-0305, PR-TEST-0306, PR-TEST-0307, PR-TEST-0308, PR-TEST-0309,
PR-TEST-0311, PR-TEST-0313, PR-TEST-0317.**

### PR-REQ-0316 - Migration Hook invocation and staged target publication

Each Hook edge MUST use the shared supervisor and owner continuation, not a new
Run or recovery engine. The exact target Revision supplies its Hook runtime.
Initial Admission MUST revalidate interpreter selection with the compiled search
configuration. Runtime materialization MUST resolve files through that exact
target's path pin, not the common initial source Revision pin.

The Frozen MigrationSessionV1 source context MUST include all present pinned
bindings with source-relative active/retained roles. The target context MUST
include all present staged active bindings and no retained target bindings.
Requirements are not visibility ACLs. Authorities remain detached files;
changing a materialized read view cannot change the authoritative registry.
Every edge has its own execution directory, output slots and Session identity.

The owner-selected Migration completion decoder MUST accept only
produced_target_outputs, never Action produced_outputs or Hook-selected
protection. Success submits every preallocated target handle exactly once;
failure submits none. Undeclared, duplicate and missing handles fail closed.
Only successful, accepted completion with observed process-tree termination
can supply bounded, safely acquired target output bytes for edge publication.
The target declaration, never Hook metadata, owns new-output protection.
Completion messages and codes are not copied into structural Run inspection.

Startup and execution deadlines apply independently to each invocation. Risk
requests reuse the durable-before-ack handshake and Clear/Open consequences.
Success with open risk is invalid and publishes no outputs. Cancellation and
transport/process failure MUST retain supervision until termination is observed.
Persistence retries retain the same continuation and staged bytes. A lost edge
commit acknowledgment MUST consult durable progress and never invoke the Hook
again. Confirmed owner loss is reconciled in a new process as Interrupted,
without original operator files, Hook replay, continuation, or rollback of an
already committed edge. Final-edge commit still atomically publishes Succeeded.

**Verification: PR-TEST-0312, PR-TEST-0313, PR-TEST-0314, PR-TEST-0315,
PR-TEST-0316, PR-TEST-0317, PR-TEST-0321, PR-TEST-0322, PR-TEST-0323.**
