---
title: Actions, Plans, and Runs
---

# Actions, Plans, and Runs

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

**Verification: PR-TEST-0119, PR-TEST-0131, PR-TEST-0133, PR-TEST-0143,
PR-TEST-0146, PR-TEST-0150, PR-TEST-0152, PR-TEST-0166, PR-TEST-0167.**

### PR-REQ-0095 - Plan preview

A managed execution command SHOULD offer a side-effect-free plan view showing
exact references, requirements, Migration paths, projected incomplete state,
Cleanup requirements, and warnings when applicable. Host-native path fields in
the human projection MUST remain lossless, using readable terminal escaping for
valid UTF-8 and an explicit native code-unit or byte form for non-UTF-8 paths.
Producing a plan MUST NOT create a Run, reservation, lock, pin, or GC root.

**Verification: PR-TEST-0120, PR-TEST-0140, PR-TEST-0141, PR-TEST-0144,
PR-TEST-0148, PR-TEST-0165.**

### PR-REQ-0096 - Plan staleness

A displayed plan MUST be treated as a preview rather than a guarantee. Actual
execution MUST revalidate the expected Instance state version, exact resources,
policy, and environment. If they changed, execution MUST fail as invalidated
rather than silently changing the workflow.

**Verification: PR-TEST-0115.**

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

**Verification: PR-TEST-0114, PR-TEST-0143, PR-TEST-0146, PR-TEST-0151,
PR-TEST-0152, PR-TEST-0153, PR-TEST-0156, PR-TEST-0159, PR-TEST-0168,
PR-TEST-0169, PR-TEST-0170, PR-TEST-0171, PR-TEST-0176, PR-TEST-0177.**

### PR-REQ-0285 - Typed Run inspection and attributed Hook explanations {#pr-req-0285---structural-only-human-run-inspection}

The ordinary human `run show`, `run list`, and invocation summaries MUST use a
typed projection rather than formatting arbitrary persisted Run
text. They MAY show Run, Instance, Revision, and Action identities, phase,
outcome, timing, taxonomy references, failed logical step, structural Hook
success or failure, published output identities and byte lengths, terminal
recovery risk, the current same-snapshot recovery guard, and fixed Pactrun
diagnostics.

`run show` and live presentation MUST expose accepted, attributed Hook explanations
under the bounded retention policy. Lists MUST remain structural. Inspection MUST
distinguish disabled retention, omissions, truncated messages and collection not
normally closed; an unknown crash tail MUST NOT imply no diagnostic was emitted.
Events MUST be ordered by Run-local sequence rather than wall-clock time.
Arbitrary persisted failure messages and stored free-text fields MUST NOT
become new accepted events. Absent, present-empty, and present-value fields
remain distinct. Terminal-control escaping is mandatory but is not sensitive-data redaction.

**Verification: PR-TEST-0520, PR-TEST-0114, PR-TEST-0146, PR-TEST-0154, PR-TEST-0175.**

### PR-REQ-0288 - Invocation parameter sources and policy values

The CLI invocation MUST accept repeated `--param <parameter-id>=<text>`
sources as `Ordinary` values, repeated `--param-file
<parameter-id>=<host-path>` sources as complete UTF-8 `Protected` values, and
one `--param-stdin <parameter-id>` source as a complete UTF-8 `Protected`
value. The first `=` separates identity from text or path; values are not
trimmed, Unicode-normalized, BOM-stripped, shell-expanded, or otherwise
rewritten. Duplicate identities, unknown parameters, and conflicting stdin
sources MUST be rejected before source acquisition. An `interactive` Action
MUST reject `--param-stdin` even for preview. Effective redaction MUST remain
the declaration-or-Protected-source maximum. Pactrun MUST NOT project parameter
values into plans, Run history or Pactrun-owned diagnostics. Separately attributed
Hook text follows PR-REQ-0098 and PR-REQ-0351.

Non-negative decimal timeout values MUST be safely converted before deadline
construction. Zero means immediate expiry or no grace; omitted startup and
Action timeouts are unlimited and omitted termination grace is 5000ms. Values
above the runtime's signed-millisecond boundary, including `u64::MAX`, MUST be
rejected before Run acceptance; timeout construction MUST not fall back to the
start instant or saturate.

**Verification: PR-TEST-0116, PR-TEST-0129, PR-TEST-0130, PR-TEST-0145,
PR-TEST-0149, PR-TEST-0150, PR-TEST-0160, PR-TEST-0163, PR-TEST-0166,
PR-TEST-0167.**

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
terminal record MUST be published in the same transaction. A publication
failure after a Hook-owned failure or after a locked `Cancelled` or `TimedOut`
outcome is secondary; a first publication failure on an otherwise successful
Run makes the Run `Failed` with `PublishDeclaredOutputs` as its primary step.

Database transaction failure retains owner-held retry state and MUST NOT be
converted into a fabricated terminal outcome. Eligibility does not require
submitting every declaration, and it does not guarantee persistence success.

**Verification: PR-TEST-0106, PR-TEST-0107, PR-TEST-0109, PR-TEST-0151,
PR-TEST-0153, PR-TEST-0169, PR-TEST-0171, PR-TEST-0176, PR-TEST-0177.**

### PR-REQ-0098 - Sensitive Run data

Pactrun-generated Run facts, parameter/Input projections and ordinary Pactrun
errors MUST NOT copy sensitive parameter values, Secret values, or revealing
value-derived digests. Interactive terminal sessions MUST NOT be retained as
complete transcripts by default.

Hook-authored free text is separately attributed and governed by PR-REQ-0283 and
PR-REQ-0351. Hook authors MUST NOT disclose sensitive values. Default bounded
retention does not authorize Parameter/Input export or promise universal taint
tracking/redaction of arbitrary Hook text.

**Verification: PR-TEST-0099, PR-TEST-0154, PR-TEST-0167, PR-TEST-0175.**

## Concurrent operations

Long-running Observe Actions can continue with their admitted context while a
Mutate operation updates current Instance state. The running Action keeps its
old pinned view; subsequent operations see the newly committed state. Mutate
operations on the same Instance serialize or conflict.

This pinning model applies to the exact Revision and Pactrun-authoritative
managed context. It does not freeze or linearize service-owned live bytes. A
service may mutate a ServiceStorage-backed Managed Service Resource without
Pactrun observing the change or publishing a new `InstanceStateVersion`;
authority and operation-prerequisite representations must account for that
ownership boundary. The [ServiceStorage execution contract](../instances/service-storage.md)
defines the current admission and publication rules.
