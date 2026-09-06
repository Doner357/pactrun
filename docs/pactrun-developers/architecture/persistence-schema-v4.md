---
title: Persistence Schema V4
---

# Persistence Schema V4

**Status: Current implemented normative internal persistence contract;
non-Frozen and non-public.**

This page extends the exact implemented
[PersistenceSchemaV3](./persistence-schema-v3.md) for M3 Slice 2. It does not
modify the V1, V2, or V3 tables or any Frozen identity, wire, or error format.
PersistenceSchemaV4 is the current implemented internal schema after M3
Slice 2 integration. It defines the durable representation of Action Runs,
execution ownership, durable execution pins, Action recovery state, the
`ManualRecoveryRequired` Instance trust guard, and Run Artifacts. Admission,
Hook launch, HookProtocolV1 runtime behavior, owner-held finalization, output
publication, and explicit owner-loss reconciliation compose over this schema in
crate-private M3 runtime code; human spelling remains outside this substrate.

### PR-REQ-0275 - Exact PersistenceSchemaV4

PersistenceSchemaV4 MUST contain the exact V3 schema followed by exactly the
twelve tables below, retain application ID `0x50414354`, and set SQLite
`user_version` to `4`. Every table remains `STRICT` and `WITHOUT ROWID`.
Correctness MUST NOT depend on rowid, insertion order, timestamp, query-plan
order, or a performance-only index.

```sql
CREATE TABLE runs (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    accepted_state_version BLOB NOT NULL CHECK(length(accepted_state_version) = 16),
    accepted_at_unix_ms INTEGER NOT NULL CHECK(accepted_at_unix_ms >= 0),
    PRIMARY KEY (run_id),
    FOREIGN KEY (instance_id) REFERENCES instances(instance_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE run_action_invocations (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    action_identity BLOB NOT NULL CHECK(length(action_identity) > 0),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES runs(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_executions (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    owner_session BLOB NOT NULL CHECK(length(owner_session) BETWEEN 1 AND 128),
    risk_state INTEGER NOT NULL CHECK(risk_state IN (0, 1)),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES runs(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_revision_pins (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES runs(run_id) ON DELETE CASCADE,
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE run_payload_pins (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    input_identity BLOB NOT NULL CHECK(length(input_identity) > 0),
    payload_id BLOB NOT NULL CHECK(length(payload_id) = 16),
    PRIMARY KEY (run_id, instance_id, input_identity),
    FOREIGN KEY (run_id) REFERENCES runs(run_id) ON DELETE CASCADE,
    FOREIGN KEY (instance_id, payload_id)
        REFERENCES managed_input_payloads(instance_id, payload_id)
        ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE run_outcomes (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    outcome_rank INTEGER NOT NULL CHECK(outcome_rank IN (0, 1, 2, 3, 4)),
    admitted_rank INTEGER NOT NULL CHECK(admitted_rank IN (0, 1)),
    risk_state INTEGER NOT NULL CHECK(risk_state IN (0, 1)),
    finished_at_unix_ms INTEGER NOT NULL CHECK(finished_at_unix_ms >= 0),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES runs(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_primary_failures (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    error_owner BLOB NOT NULL CHECK(length(error_owner) BETWEEN 1 AND 128),
    error_code BLOB NOT NULL CHECK(length(error_code) BETWEEN 1 AND 128),
    failed_step INTEGER NOT NULL CHECK(failed_step BETWEEN 0 AND 5),
    message_utf8 BLOB NOT NULL,
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES run_outcomes(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_secondary_failures (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    error_owner BLOB NOT NULL CHECK(length(error_owner) BETWEEN 1 AND 128),
    error_code BLOB NOT NULL CHECK(length(error_code) BETWEEN 1 AND 128),
    message_utf8 BLOB NOT NULL,
    PRIMARY KEY (run_id, ordinal),
    FOREIGN KEY (run_id) REFERENCES run_outcomes(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_hook_completions (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    status_rank INTEGER NOT NULL CHECK(status_rank IN (0, 1)),
    code_present INTEGER NOT NULL CHECK(code_present IN (0, 1)),
    code_utf8 BLOB NOT NULL
        CHECK((code_present = 0 AND length(code_utf8) = 0)
            OR (code_present = 1 AND length(code_utf8) > 0)),
    message_present INTEGER NOT NULL CHECK(message_present IN (0, 1)),
    message_utf8 BLOB NOT NULL CHECK(message_present = 1 OR length(message_utf8) = 0),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES run_outcomes(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_artifacts (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    output_identity BLOB NOT NULL CHECK(length(output_identity) > 0),
    byte_length INTEGER NOT NULL CHECK(byte_length BETWEEN 0 AND 536870912),
    PRIMARY KEY (run_id, output_identity),
    FOREIGN KEY (run_id) REFERENCES run_outcomes(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_artifact_chunks (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    output_identity BLOB NOT NULL CHECK(length(output_identity) > 0),
    chunk_index INTEGER NOT NULL CHECK(chunk_index >= 0),
    chunk_bytes BLOB NOT NULL CHECK(length(chunk_bytes) BETWEEN 1 AND 1048576),
    PRIMARY KEY (run_id, output_identity, chunk_index),
    FOREIGN KEY (run_id, output_identity)
        REFERENCES run_artifacts(run_id, output_identity)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE instance_recovery_guards (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    trigger_rank INTEGER NOT NULL CHECK(trigger_rank IN (0, 1, 2)),
    entered_at_unix_ms INTEGER NOT NULL CHECK(entered_at_unix_ms >= 0),
    PRIMARY KEY (instance_id),
    FOREIGN KEY (instance_id) REFERENCES instances(instance_id) ON DELETE CASCADE,
    FOREIGN KEY (run_id) REFERENCES run_outcomes(run_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

PRAGMA user_version = 4;
```

`run_id` is an independently generated opaque 128-bit value. It MUST NOT be
derived from the Plan, the Instance, the Action, or a timestamp, and MUST NOT
be reused. `action_identity`, `input_identity`, and `output_identity` store
exact UTF-8 semantic identifiers; `error_owner` and `error_code` store exact
UTF-8 Pactrun error names; `code_utf8` stores an exact UTF-8 `HookCodeV1`
semantic identifier; `message_utf8` stores exact UTF-8 text; `owner_session`
stores the exact staging session directory name selected by PR-REQ-0277. The
repository MUST validate each Domain syntax when writing and loading.

Closed ranks are the only representation of their vocabularies. `outcome_rank`
is `Succeeded = 0`, `Failed = 1`, `Cancelled = 2`, `TimedOut = 3`,
`Interrupted = 4`; the row check proves that `ManualRecoveryRequired` is not a
Run outcome. `admitted_rank` is `Accepted = 0`, `Admitted = 1`. `risk_state` is
`Clear = 0`, `Open = 1`. `status_rank` is `success = 0`, `failure = 1`.
`failed_step` is `Admission = 0` followed by the Action Plan steps
`EstablishSession = 1`, `LaunchHook = 2`, `AcceptCompletion = 3`,
`PublishDeclaredOutputs = 4`, `Finalize = 5`. `trigger_rank` is
`OpenRiskFailure = 0`, `OpenRiskOwnerLoss = 1`, `SuccessWithOpenRisk = 2`.

Optional state is represented only by row presence. A Run is `Running` if and
only if its `run_executions` row exists and `Finished` if and only if its
`run_outcomes` row exists; exactly one of the two MUST exist. A Running Run is
`Admitted` if and only if its `run_revision_pins` row exists. A Finished Run's
boundary is the recorded `admitted_rank`; released pins MUST NOT be used to
classify a Finished Run. Every V4 Run MUST have exactly one
`run_action_invocations` row. That row is the complete Action-specific
operation record and the operation-kind discriminator; a later schema adds
other managed-execution kinds as their own rows rather than a kind column.

`run_action_invocations` records the exact historical Revision and Action
identity without a foreign key. Only `run_revision_pins` references
`revisions`, with `ON DELETE RESTRICT`, so a durably pinned Revision cannot be
deleted while historical Run identity never prevents deletion. `run_payload_pins`
references `managed_input_payloads` with `ON DELETE RESTRICT`, so a pinned
payload cannot be reclaimed. Payload reachability is a current binding row or
an execution pin row; the repository MUST reclaim a payload only when neither
exists and MUST decide that by explicit lookup rather than by relying on a
foreign-key failure. `runs` references `instances` with `ON DELETE RESTRICT`;
Instance deletion remains M7 behavior and MUST address Run history explicitly.

Execution pins are established only for an accepted Run, atomically with the
token-first comparison of the current `InstanceStateVersion` against the
accepted state version, and only while the Run is Running and unpinned. The
pin set for an Action Run is exactly one Revision pin and one payload pin per
active binding referenced at admission. Pins are released in the same
transaction that publishes the terminal outcome. This Action-specific lifetime
is admissible because the recovery reference of an Action obligation is the
Run record itself, never the pinned bytes: no Pactrun-owned recovery action
reads pinned content, and Hook replay is forbidden. Pins are internal lifetime
bookkeeping; they MUST NOT block `SetInput`, `DeleteInput`, or any other
management mutation, and MUST NOT be presented as a lease or guard.

`run_executions.risk_state` is the live protocol risk state of a Running Run.
It MUST change only `Clear` to `Open` on durable risk entry and `Open` to
`Clear` on durable risk resolution, and only while the Run is Admitted.
`run_outcomes.risk_state` is the terminal risk state copied from the live state
in the terminal transaction and retained as provenance. The Action recovery
directive is fixed by the operation kind: an Action changes no
Pactrun-authoritative Instance state, so the only Pactrun-owned recovery action
is applying the open-risk Instance consequence. No directive column exists.

The terminal consequence is derived in Domain code from the outcome, the live
risk state, and the Hook completion, in this order: `Clear` risk yields no
consequence; `Open` risk with `Succeeded` is an invalid transition that
publishes nothing; `Open` risk with a Hook completion whose status is success
yields `SuccessWithOpenRisk`; otherwise `Open` risk with `Interrupted` yields
`OpenRiskOwnerLoss`; every remaining `Open` non-success yields
`OpenRiskFailure`. A consequence publishes an `instance_recovery_guards` row
and a fresh `InstanceStateVersion` in the terminal transaction. The guard row
exists if and only if the Instance is in `ManualRecoveryRequired`; its
`run_id` references `run_outcomes` with `ON DELETE RESTRICT` so the triggering
Run remains a strong recovery reference while the obligation is unresolved. If
a guard already exists when another open-risk Run finishes, the existing row
MUST remain unchanged and no new state version is published; the newer Run's
terminal `risk_state` records the fact. Run creation, pin establishment, risk
transitions, and a terminal publication without a consequence MUST NOT publish
a new `InstanceStateVersion`. `ResolveManualRecovery` is a token-first
management mutation that deletes the guard row and publishes a fresh state
version.

Hook completion `code` and `message` use the presence representation. An
absent message is `message_present = 0` with empty bytes; a present empty
message is `message_present = 1` with empty bytes; a present message stores its
exact bytes. An absent code is `code_present = 0` with empty bytes; a present
code is `code_present = 1` with a non-empty valid `HookCodeV1`. No byte-length
bound applies to completion or failure messages. Failure records are Pactrun
error references; a Hook-reported failure keeps its Hook-owned completion and
does not require a Pactrun primary failure.

Run Artifacts use the exact payload physical representation: a header plus
contiguous chunks from index zero, every non-final chunk of exactly 1,048,576
bytes, a final chunk of 1 through 1,048,576 bytes, concatenated length equal to
the header length, and a 536,870,912-byte maximum. An empty Artifact has
`byte_length = 0` and no chunk rows. The typed repository MUST validate
cross-row contiguity and aggregate length before commit and on every logical
load. Artifacts reference `run_outcomes`, so an Artifact can exist only for a
Finished Run; a terminal publication MAY store any subset of the declared
outputs, a failed Run MAY retain submitted files as diagnostic Artifacts, and an
Artifact MAY be deleted independently of the retained Run record.

Every deterministic Run enumeration MUST order by `instance_id`, then `run_id`,
using BLOB byte order. Secondary failures MUST order by `ordinal`; chunk reads
MUST state `ORDER BY chunk_index`. Domain code MUST verify SQL/Domain ordering
parity after load. `accepted_at_unix_ms`, `finished_at_unix_ms`, and
`entered_at_unix_ms` are informational timing values; they MUST NOT carry
identity, ordering, or correctness.

Logical load MUST treat the following as corruption rather than another
representation: a Run with both or neither of its execution and outcome rows;
a Run without exactly one `run_action_invocations` row; a payload pin whose
`instance_id` differs from the Run's Instance; a `Succeeded` outcome with a
primary failure, with a Hook completion whose status is failure, or with
terminal risk `Open`; a guard whose Run has terminal risk `Clear`; a chunk
gap, non-final short chunk, or length mismatch; an invalid Domain string; or an
`owner_session` that is not a valid staging session name.

A Running Action Run MUST be in exactly one of these boundary/risk states:
`Accepted + Clear`, `Admitted + Clear`, or `Admitted + Open`. Recovery risk
MUST NOT become `Open` before Admission. A persisted `Accepted + Open` Running
Run is impossible and corrupt. Logical loading, recovery, and terminal
transitions MUST reject it without publishing an outcome, changing the Instance
recovery guard or state version, releasing references, or normalizing it into a
valid state.

V4 has no dedicated sensitive-value fields: no table stores invocation
parameter values, Secret payload bytes, or value-derived digests for a Run.
This is a schema fact only. Runtime redaction of Hook-authored and
Pactrun-authored diagnostic text remains the obligation of the slices that
produce Run text.

**Verification: PR-TEST-0082, PR-TEST-0084, PR-TEST-0087, PR-TEST-0104, PR-TEST-0105, PR-TEST-0112.**

### PR-REQ-0276 - Persistence migration to V4

Opening persistence MUST classify the database as pristine, exact V1, exact
V2, exact V3, exact V4, or inadmissible. Exact versions have Pactrun's
application ID, their matching user version, and the complete schema manifest
for that version. Foreign, unmarked non-empty, partial, drifted, and newer
databases MUST be rejected rather than repaired or guessed.

After establishing the existing WAL and connection profile, Pactrun MUST use
`BEGIN IMMEDIATE` and re-read ownership markers and schema. A pristine database
MUST create exact V4 directly. Exact V1 MUST create the unchanged V2, V3, and
V4 relations in the same transaction. Exact V2 MUST add the V3 and V4
relations. Exact V3 MUST add only the twelve V4 tables. Exact V4 is
validate-only. Schema validation MUST compare table names, columns, declared
types, nullability, defaults, checks, primary and unique keys, foreign keys and
delete actions, `STRICT`, `WITHOUT ROWID`, and every correctness-bearing index
or trigger in the version manifest.

The version marker MUST advance to `4` only after the complete resulting schema
has been validated inside the transaction. A failure or crash before commit
leaves the exact previous version; a crash after commit exposes exact V4.
Reopen and retry MUST converge, and concurrent initializers or migrators MUST
use SQLite locking rather than a second cross-process migration lock.

Migration MUST preserve every Package and Revision identity, canonical Revision
component byte string, runtime-content reference, M1-D metadata value, Instance,
binding, payload header, and payload chunk. It MUST NOT infer a Run, invocation,
execution owner, pin, outcome, failure, Hook completion, Artifact, or recovery
guard from existing rows, and it MUST NOT rewrite Frozen canonical content.

**Verification: PR-TEST-0082.**

## Candidate crate-private repository contract

The M3 Slice 2 implementation derives typed operations equivalent to:

```text
create_accepted_run(instance, accepted_state_version, action_identity, owner_session) -> RunId
admit_run(run, admission_facts, launcher_check, override_guard) -> admitted | refusal
open_recovery_risk(run)
clear_recovery_risk(run)
finish_run(run, outcome, failures, hook_completion, staged_artifacts) -> published state version?
finish_run_owned(owner, run, outcome, hook_completion, submitted_outputs, staged_artifacts)
    -> published state version?
advance_owner_continuation(run) -> durable terminal published | owner retry retained
resolve_manual_recovery(instance, expected_state_version) -> InstanceStateVersion
list_runs(instance) -> ordered RunSummary[]
load_run(run) -> RunView
load_instance_recovery_guard(instance) -> RecoveryGuardView?
load_run_inspection(run) -> RunInspectionData
reconcile_action_runs() -> reconciled RunId[]
open_run_artifact(run, output) -> streamed bytes
delete_run_artifact(run, output)
stream_admitted_payload(run, input) -> streamed pinned bytes
open_admitted_runtime_blob(run, runtime_file) -> verified pinned bytes
```

Every mutation uses `BEGIN IMMEDIATE`. Run creation records the accepted state
version without comparing it. Admission evaluates, inside one transaction and in
the order of PR-REQ-0279, the recovery guard, the current Instance state
version against the expected and accepted versions, the active Revision, every
referenced current binding, readiness derived from the persisted bindings and
Revision declarations, runtime-content availability, interpreter launcher
re-selection, and the Mutate-conflict predicate. Both access modes the predicate
compares are read from the persisted `run_action_invocations` identity and the
decoded Revision core, and the predicate itself is derived from existing
`run_executions`, `run_revision_pins`, and `run_action_invocations` rows, so no
schema change is needed. A refusal publishes the terminal `Failed` outcome in
that same transaction; success inserts the pins. Terminal publication computes the
consequence before writing, deletes the execution row, writes the outcome and
its records, releases pins, reclaims unreferenced payloads, and publishes the
guard and state version only when a consequence exists. Artifact bytes stream
from transient staging in one-megabyte chunks. The two pinned-view readers
added by M3 Slice 4 resolve an admitted Run's payload and runtime-content pins
from the existing `run_payload_pins`, `run_revision_pins`, and
`revision_runtime_content_refs` rows and refuse a Run that is not Running and
Admitted; they add no schema. A transaction failure publishes no Run, pin,
risk transition, outcome, Artifact, guard, or state version. The concrete Rust
names remain crate-private and are not a stable API.

M3 Slice 5 composes the crate-private owner continuation and finalization
substrate over V4: eligible Action output bytes are independently staged,
execution cleanup is attempted before the terminal transaction, and the
Run outcome, failures, Hook structural result, and Artifacts are published
atomically. The production finalizer retains Hook completion status while
omitting Hook-authored code and message text; generic historical V4 reads keep
their exact optional-value semantics. The explicit owner-loss reconciler reads
the durable owner and risk state, confirms the staging lease, and finishes only
confirmed-lost valid Running Action records. `load_run_inspection` reads a Run
and its Instance's current recovery guard from one SQLite read snapshot. These
additions do not alter the V4 table manifest or expose a public inspection or
CLI surface.

## Deferred work

V4 does not define non-sensitive parameter recording, Run retention policy,
Snapshot, Migration, Restore, or Cleanup execution records, Instance deletion,
generic garbage collection, Artifact export, ServiceStorage, a stable public
API, or human spelling.
