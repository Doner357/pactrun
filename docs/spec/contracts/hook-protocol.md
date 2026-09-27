---
title: Hook Protocol Baseline
---

# Hook Protocol Baseline

**Status: Approved E normative baseline, 1.0-alpha.1; implementation verified for local E delivery.**

## Reading map (informative)

<!-- spec-navigation:start -->
This is the current owning contract after the approved one-time E reset.
Read [versioning](../foundations/product-versioning-and-compatibility.md) and
[implementation status](../../development/e-implementation-status.md).
The older contract pages are non-normative link explanations, not supported readers.
<!-- spec-navigation:end -->

## Supported baseline

One owner-selected protocol baseline includes ordinary operations, persistent
service authority and Migration target proposals. Every direction starts with
`ASCII("pactrun.hook-protocol\0") || U16BE(version_byte_length) || ASCII(version)`.
Length is 1..128; the supported value is `1.0-alpha.1`. Unknown, malformed,
truncated and oversized preambles fail before frame allocation. Frames retain
U32BE payload length and the inclusive 16 MiB bound.

Both Session messages confirm the selected string and exact Session identity.
The complete session_start always includes service_authorities, even when empty;
target_commit is present only when admitted target publication requires it.
There is no protocol negotiation, numeric-version reader or retry at another
version. Request/control IDs remain positive safe integers; this change does
not turn them into version identifiers or relax authority and recovery rules.

Existing objects are never relabeled, rehashed, rebound or rewritten merely
because the product is updated or later promoted. Unknown contracts are refused;
refusal does not authorize deleting real data or touching service resources.
Stable requirement IDs and incoming heading anchors are retained below. Historical
heading labels do not advertise support for retired numeric formats.

## Protocol lifecycle

### PR-REQ-0204 - Candidate and Frozen protocol contract

Candidate and Frozen use the same message schema, framing, validation profile,
state machine, and fixtures. Freezing is a status change only. An
identity-bearing `hook.protocol_version = "1.0-alpha.1"` selects this exact protocol; a
Candidate implementation MUST NOT be used to publish or execute a stable Pack
contract as though that protocol contract were Frozen. After Freeze, an incompatible
wire or semantic change requires a new Hook Protocol version.

Hook Protocol, Revision Core, Snapshot Integrity, persistence, bundle, and CLI
output formats remain independent version domains. The string `1.0-alpha.1` in
`hook.protocol_version` does not select a Revision Core format merely because
both domains currently use the same spelling.

**Verification: PR-TEST-0020, PR-TEST-0031, PR-TEST-0032.**

## Transport and input profile

### PR-REQ-0205 - Dedicated stream, preamble, and frames

The Hook Protocol MUST use a dedicated reliable, ordered, full-duplex byte
stream separate from Pack-facing terminal input and output. Each direction is
independently encoded as:

```text
ASCII("pactrun.hook-protocol\0")
U16BE(version_byte_length) ASCII("1.0-alpha.1")
U32BE(json_payload_length)
json_payload_utf8
...
```

The preamble occurs exactly once at the start of each direction. Lengths count
octets. A JSON payload MUST be no larger than 16 MiB (`16 * 1024 * 1024`
octets). A wrong, truncated, or repeated preamble, a truncated frame, or an
oversized declared length terminates the protocol. Transport selection, pipe or
socket spelling, buffering, and process-launch integration are not part of the
wire contract. The separate Pack-facing runtime discovery ABI is defined by
[PR-REQ-0280](./hooks-recovery-and-cleanup.md#pr-req-0280---hook-protocol-runtime-transport-discovery);
it selects this stream without changing Frozen V1 bytes or messages.

**Verification: PR-TEST-0020, PR-TEST-0031, PR-TEST-0093, PR-TEST-0244.**

### PR-REQ-0206 - Strict JSON message profile

Each payload MUST be one UTF-8 JSON object. The receiver MUST reject malformed
UTF-8, malformed JSON, duplicate object properties at any depth, non-scalar
Unicode strings, and values outside the closed message schema. It MUST NOT
repair Unicode or replace malformed input. JSON numbers must be finite
I-JSON-compatible binary64 values; fields defined as integers additionally
must be exact mathematical safe integers.

Hook Protocol JSON is not JCS. Object-member order and insignificant JSON
whitespace have no semantic meaning, but a sender is not required to emit one
canonical byte spelling. Semantic-set arrays use their defined keys; ordered
arrays retain order. Negative conformance fixtures SHOULD isolate one fault.
Unless this specification explicitly says otherwise, a multi-fault message
does not establish a stable global first-error precedence.

**Verification: PR-TEST-0021, PR-TEST-0093, PR-TEST-0242, PR-TEST-0244.**

### PR-REQ-0207 - Exact version confirmation

The preamble selects protocol version `1`. The first Pactrun message MUST be a
`session_start` carrying `protocol_version: "1.0-alpha.1"`; the first Hook message MUST be a
matching `session_ready`. There is no range negotiation, downgrade, feature
probing, or best-effort interpretation. A peer that cannot implement the exact
version MUST reject it and terminate without guessing a supported version.

`HookSessionSpec` has no `features` field. Recovery-risk requests and
structured diagnostics are mandatory V1 protocol facilities, not optional
features that can form undefined combinations with version `1`.

**Verification: PR-TEST-0022, PR-TEST-0093.**

## Message envelope and session schema

### PR-REQ-0208 - Direction, request IDs, and control ordering

Every message is a closed object selected by its `type`. Messages are valid
only in their specified direction and state. Hook-originated requests use a
positive safe-integer `request_id`. At most one Hook-originated request may be
outstanding; an acknowledgment MUST repeat that exact ID. Request IDs MUST NOT
be reused within a Session.

Pactrun cancellation is asynchronous and uses an independently sequenced
positive safe-integer `control_id`. A cancellation message may be sent while a
Hook request is outstanding and does not act as that request's acknowledgment.
A `cancel_ack` confirms receipt only; Pactrun still owns termination, Run
outcome, and finalization. Cancellation does not bypass recovery-risk or
completion validation.

**Verification: PR-TEST-0023, PR-TEST-0030, PR-TEST-0093.**

### PR-REQ-0209 - Exact Session identity and values

`session_start` uses this closed shape:

```text
HookSessionSpec
|- type: session_start
|- protocol_version: "1.0-alpha.1"
|- session_id: OpaqueMachineIdV1
|- run_id: OpaqueMachineIdV1
|- revision: RevisionIdentityV1
|- parameters: ParameterBindingV1[]
|- workspace: WorkspaceAuthorityV1
|- io: IOContractV1
`- operation: OperationSessionV1

RevisionIdentityV1
|- package_id: OpaqueMachineIdV1
`- revision_content_digest: Sha256Digest

ParameterBindingV1
|- parameter_id: ParameterIdentity
`- value: integer | finite float | boolean | string

IOContractV1
`- terminal: none | output | interactive
```

`OpaqueMachineIdV1` and every authority handle are exactly 32 lowercase ASCII
hexadecimal characters. They are machine identities, not names or labels.
Parameter bindings are a semantic set unique and sorted by `parameter_id`.
Their values have already passed operation-specific parameter binding before
Session start. Sensitive parameter values remain sensitive and are not made
safe merely by appearing on the dedicated protocol stream.

`session_ready` is exactly:

```text
{ type: session_ready, protocol_version: "1.0-alpha.1", session_id: OpaqueMachineIdV1 }
```

**Verification: PR-TEST-0022, PR-TEST-0024.**

## Filesystem and binding authorities

### PR-REQ-0210 - Workspace and authority boundaries

Every Session receives:

```text
WorkspaceAuthorityV1
|- handle: AuthorityHandleV1
`- root_path: HostNativeAbsolutePath
```

The Workspace is execution-scoped writable scratch space. A Hook MAY freely
create files and directories below its root without predeclared per-file
handles. Workspace bytes are not automatically Managed Outputs, Run Artifacts,
Snapshot contents, Instance Inputs, Revision content, or persistent
service-owned state. A service MUST NOT depend on them surviving the execution
lifetime. They are cleanup-eligible at the terminal lifecycle boundary unless a
stronger Pactrun recovery reference requires otherwise.

`access: observe` does not make the Workspace read-only. Observe classifies the
operation's allowed managed/service semantic mutation and guard behavior; it
does not prohibit temporary filesystem writes in execution scratch. Workspace
writes do not waive recovery-risk duties or authorize unrelated external side
effects.

`HostNativeAbsolutePath` is a non-empty valid Unicode scalar sequence without
NUL. Pactrun MUST supply a path absolute under the execution host's native path
rules. Host-visible paths are Session facts, not identity. Protocol fixtures
validate the wire shape but do not claim OS permission, containment, symlink,
or cleanup enforcement.

Workspace, Action or Migration output staging, SnapshotCandidate, and
SnapshotContent are distinct Domain authorities even if a future runtime shares
filesystem primitives among them. A Hook cannot request a new authority or
enlarge an existing one.

**Verification: PR-TEST-0025, PR-TEST-0027, PR-TEST-0029, PR-TEST-0094.**

### PR-REQ-0211 - Operation-specific binding visibility

Read-only binding authorities use:

```text
BindingAuthorityV1
|- handle: AuthorityHandleV1
|- input_id: InputIdentity
|- role: active | retained
`- readonly_path: HostNativeAbsolutePath
```

The closed `OperationSessionV1` union is:

```text
ActionSessionV1
|- kind: action
|- action_id: ActionIdentity
|- access: observe | mutate
|- bindings: BindingAuthorityV1[]
`- outputs: ActionOutputAuthorityV1[]

CaptureSessionV1
|- kind: snapshot_capture
|- access: observe | mutate
|- bindings: BindingAuthorityV1[]
`- candidate: SnapshotCandidateAuthorityV1

RestoreSessionV1
|- kind: snapshot_restore
|- snapshot_id: OpaqueMachineIdV1
|- bindings: BindingAuthorityV1[]
`- snapshot_content: SnapshotContentRootAuthorityV1

MigrationSessionV1
|- kind: migration
|- source_revision: RevisionIdentityV1
|- source_bindings: BindingAuthorityV1[]
|- target_bindings: BindingAuthorityV1[]
`- target_outputs: MigrationOutputAuthorityV1[]

CleanupSessionV1
|- kind: cleanup
`- bindings: BindingAuthorityV1[]
```

Action and Capture bindings MUST all be active and expose the pinned current
active binding view. Retained bindings are excluded. Restore bindings MUST all
be active and expose the staged active Snapshot view that will become
authoritative on success. The Hook-visible view does not redefine Restore
commit: Pactrun restores the complete Snapshot-managed binding state defined by
PR-REQ-0102, PR-REQ-0146, PR-REQ-0151, and PR-REQ-0199, including retained
bindings, absence, and protection. Restore replaces rather than merges current
bindings. M0-B does not verify that later commit.

Migration source bindings MAY contain active and retained roles from the full
pinned source context. Target bindings are the full staged target context and
MUST be active. Cleanup bindings MAY contain the current active and retained
context. Revision Core `requires_source[]`, `requires_target[]`, and Cleanup
`requires[]` remain admission or completion contracts, not Session visibility
ACLs. Active and retained remain roles over one `ManagedInputBindings` registry.

Binding arrays are semantic sets unique and sorted by `(role, input_id)`.

**Verification: PR-TEST-0026, PR-TEST-0094, PR-TEST-0257, PR-TEST-0258, PR-TEST-0261.**

### PR-REQ-0212 - Managed output authorities and completion

Action output authority is:

```text
ActionOutputAuthorityV1
|- handle: AuthorityHandleV1
|- output_id: ManagedOutputIdentity
`- staged_path: HostNativeAbsolutePath
```

Declarations establish the maximum authorized output vocabulary. Session start
preallocates one opaque regular-file slot for each authorized V1 output. An
Action success or failure MAY submit any subset of those handles. An undeclared,
unallocated, or duplicate handle is invalid. A failed Action remains failed
even when submitted files are retained as diagnostic Artifacts. Acceptance does
not guarantee final RunArtifact persistence before Pactrun's later managed
commit succeeds.

Migration output authority is:

```text
MigrationOutputAuthorityV1
|- handle: AuthorityHandleV1
|- input_id: InputIdentity
`- staged_path: HostNativeAbsolutePath
```

Each slot is tied to one declared `produces_target[]` Input and contains opaque
target binding bytes. Successful Migration completion MUST submit every target
output handle exactly once. Failure MUST submit none and MUST NOT publish staged
target bindings. Restore and Cleanup publish no managed output.

**Verification: PR-TEST-0027, PR-TEST-0094, PR-TEST-0151, PR-TEST-0153,
PR-TEST-0169, PR-TEST-0323.**

### PR-REQ-0213 - Snapshot candidate and content authorities

Capture and Restore use distinct authorities:

```text
SnapshotCandidateAuthorityV1
|- handle: AuthorityHandleV1
`- root_path: HostNativeAbsolutePath

SnapshotContentRootAuthorityV1
|- handle: AuthorityHandleV1
|- readonly_root_path: HostNativeAbsolutePath
`- logical_descriptors: SnapshotContentDescriptorV1[]

SnapshotContentDescriptorV1
|- role: ServiceContentRole
|- path: SnapshotContentPath
|- blob_digest: Sha256Digest
`- materialized_path: PortableSessionRelativePathV1

CaptureServiceContentSubmissionV1
|- role: ServiceContentRole
|- path: SnapshotContentPath
`- candidate_path: PortableSessionRelativePathV1
```

`PortableSessionRelativePathV1` is a normative alias of the Frozen
RevisionCoreFormat runtime-path lexical profile in PR-REQ-0185. That
requirement is the single lexical authority. Informatively, the current profile
uses `/`-separated identifier segments, bounds the UTF-8 byte length, and
rejects empty, dot, dot-dot, backslash, absolute, drive-prefixed, and reserved
device-name spellings. Any disagreement with PR-REQ-0185 is a specification
conflict and Candidate freeze blocker; an implementation MUST NOT choose one
spelling silently.

Snapshot logical `path`, `candidate_path`, and `materialized_path` have distinct
roles. The latter two are Session locators below their respective roots. They
are not Snapshot or Revision identity, do not affect `SnapshotIntegrityDigest`,
and may differ across hosts or executions. Lexical validation precedes
materialization. A lexical check alone does not prove filesystem containment or
safe symlink behavior.

Restore receives only the selected immutable Snapshot's read-only service
content. Its logical `role`, `path`, and `blob_digest` remain authoritative
under Frozen SnapshotIntegrityFormat; the Hook cannot browse another
Snapshot. Capture writes a candidate, and Pactrun computes and validates
authoritative content digests rather than trusting Hook claims.

`CaptureCompletionV1.service_content` is the Frozen Snapshot semantic set from
PR-REQ-0200 and PR-REQ-0201. Its uniqueness key is `(role, path)` and wire array
order has no semantic meaning. `candidate_path` is not part of the key; changing
it does not permit a duplicate logical descriptor. Different logical pairs MAY
refer to the same candidate file. A successful Capture MUST submit a valid set,
which MAY be empty because Frozen Snapshot semantics permit an empty service
content closure. Empty service content does not remove separately managed
Snapshot binding state.

**Verification: PR-TEST-0029, PR-TEST-0031, PR-TEST-0242, PR-TEST-0243, PR-TEST-0245, PR-TEST-0250, PR-TEST-0256, PR-TEST-0257, PR-TEST-0258, PR-TEST-0261, PR-TEST-0265, PR-TEST-0266, PR-TEST-0268, PR-TEST-0269.**

S5 protocol coverage exercises the runtime decoder against the unchanged Frozen
Capture transcripts and a real private pipe/socket peer that writes explicitly
submitted candidate bytes. This is not yet managed Capture materialization,
safe filesystem acquisition, digest computation, or Snapshot publication.

## Diagnostics, recovery, and completion

### PR-REQ-0214 - Structured Hook-authored text

A Hook MAY emit:

```text
DiagnosticV1
|- type: diagnostic
|- severity: info | warning | error
|- code: HookCodeV1
`- message: string
```

Completion results and Hook-originated `protocol_error` messages MAY likewise
carry a `HookCodeV1` and human-readable message. `HookCodeV1` uses the semantic
identifier grammar, is owned by the Hook or Package, and MUST NOT be interpreted
as a Pactrun error-taxonomy code. Diagnostics are separate from managed output.

Hook-authored diagnostics, codes, completion messages, protocol-error text, and
other Hook-authored protocol text MUST NOT intentionally disclose Secret or
sensitive values. Schema validation cannot generally recognize hashes,
encodings, substrings, transformations, or application-specific derivations.
M0-B fixtures verify only structure and lexical constraints and do not claim
complete leakage detection. Runtime collection and presentation follow the
[execution diagnostics policy](../behavior/execution-diagnostics.md). That policy
does not promise universal redaction or make Pactrun a secret vault or OS sandbox.

**Verification: PR-TEST-0024.**

### PR-REQ-0283 - Runtime Hook text retention boundary

New production Runs MUST retain structural Hook state and, by default, bounded
accepted diagnostic, completion and protocol-error explanations for debugging.
`--no-retain-hook-text` MUST disable text persistence without disabling display.
Omission, truncation and unknown crash tails MUST remain explicit. Rejected frames
MUST NOT become accepted evidence. Hook text is not Pactrun outcome authority.
This approved policy supersedes non-retention on 2026-09-19; it does not collect
Legacy events retroactively. The existing V4 optional completion
representation and exact historical reading of absent versus present-empty
values remain unchanged.

Pactrun-owned failures MUST use fixed safe text and a taxonomy reference and
MUST NOT retain raw frames, Secret values, sensitive parameters, protocol
endpoints, or execution paths. This boundary does not require scanning or
rewriting opaque Artifact bytes and does not claim general detection of
sensitive derived values.

**Verification: PR-TEST-0084, PR-TEST-0113, PR-TEST-0520, PR-TEST-0522.**

### PR-REQ-0215 - Recovery-risk request state machine

The initial risk state is `clear`. A Hook requests a transition with:

```text
{
  type: request,
  request_id: PositiveSafeInteger,
  request: { kind: enter_recovery_risk | resolve_recovery_risk }
}
```

Pactrun acknowledges with:

```text
{
  type: request_ack,
  request_id: PositiveSafeInteger,
  risk_state: clear | open
}
```

`enter_recovery_risk` is valid only from clear. Pactrun MUST durably publish the
open marker and operation-specific recovery directive before acknowledging
`open`; the Hook MUST NOT cross the recovery-relevant service boundary before
that acknowledgment. `resolve_recovery_risk` is valid only from open. Pactrun
MUST durably clear the marker before acknowledging `clear`. It means the Pack
considers service-owned state coherent for ordinary management, not rollback or
Run success. There is one `clear | open` state, not a nested or graded risk
taxonomy.

Fixtures verify message order and modeled state only. Durable publication and
crash boundaries for Action execution are an M3 responsibility under
PR-REQ-0055 and PR-REQ-0056 and require M3 persistence and runtime integration
tests; M6 generalizes them to later managed-execution types.

**Verification: PR-TEST-0028, PR-TEST-0093, PR-TEST-0156, PR-TEST-0159, PR-TEST-0173, PR-TEST-0243, PR-TEST-0248, PR-TEST-0253, PR-TEST-0260.**

### PR-REQ-0216 - Operation completion and terminal states

The Hook completes with this closed union:

```text
ActionCompletionV1
|- type: complete
|- operation: action
|- status: success | failure
|- code?: HookCodeV1
|- message?: string
`- produced_outputs: AuthorityHandleV1[]

MigrationCompletionV1
|- type: complete
|- operation: migration
|- status: success | failure
|- code?: HookCodeV1
|- message?: string
`- produced_target_outputs: AuthorityHandleV1[]

CaptureCompletionV1
|- type: complete
|- operation: snapshot_capture
|- status: success | failure
|- code?: HookCodeV1
|- message?: string
`- service_content: CaptureServiceContentSubmissionV1[]

RestoreCompletionV1 | CleanupCompletionV1
|- type: complete
|- operation: snapshot_restore | cleanup
|- status: success | failure
|- code?: HookCodeV1
`- message?: string
```

A Hook MUST NOT complete while one of its requests is outstanding. Success with
open recovery risk is a protocol violation. A valid failure with open risk is
accepted as a failed operation and causes the runtime recovery consequence
defined by PR-REQ-0057. Pactrun acknowledges a valid completion only with:

```text
{ type: completion_accepted }
```

The Hook SHOULD remain until it receives that message. `completion_accepted`
means only that the protocol submission was accepted. It does not imply Run,
Artifact, Snapshot, Migration, Restore, or Instance commit, nor does it turn a
failed completion into success. The Session is terminal after acceptance.

Pactrun cancellation uses:

```text
{ type: cancel, control_id: PositiveSafeInteger, reason: requested | timeout }
{ type: cancel_ack, control_id: PositiveSafeInteger }
```

An acknowledgment is not terminal. EOF, timeout, process loss, or protocol
failure before accepted completion prevents protocol success. Their Run and
Instance consequences remain Pactrun-owned and depend on the durable risk state;
Pactrun MUST NOT infer replay or compensation.

**Verification: PR-TEST-0027, PR-TEST-0028, PR-TEST-0030, PR-TEST-0093, PR-TEST-0097, PR-TEST-0152, PR-TEST-0153, PR-TEST-0169, PR-TEST-0170, PR-TEST-0171, PR-TEST-0176, PR-TEST-0177, PR-TEST-0242, PR-TEST-0243, PR-TEST-0245, PR-TEST-0248, PR-TEST-0260, PR-TEST-0399.**

### PR-REQ-0217 - Local protocol errors and ownership

Pactrun V1 protocol failures use closed local codes:

```text
invalid_preamble
frame_too_large
invalid_utf8
invalid_json
duplicate_property
invalid_unicode_scalar
unsupported_protocol_version
unknown_field
invalid_message
unexpected_message
request_in_flight
request_id_mismatch
invalid_recovery_transition
invalid_session_relative_path
duplicate_semantic_key
invalid_authority
invalid_completion
completion_with_open_risk
```

These codes belong to HookProtocol validation and state-machine handling.
They do not establish the future cross-layer error architecture owned by M0-C.
A Pactrun-originated `protocol_error` MUST use one of these codes; a
Hook-originated `protocol_error` uses a Hook-owned code. Sender and state make
the two closed message forms unambiguous.

**Verification: PR-TEST-0021, PR-TEST-0023, PR-TEST-0028, PR-TEST-0030,
PR-TEST-0093, PR-TEST-0170.**

### PR-REQ-0218 - Authority is not isolation and verification is layered

HookProtocol authority restricts Pactrun-mediated operations. It does not
claim that a trusted native Hook is confined by an OS sandbox. The protocol
stream remains separate from the terminal channels described by `io`, and
messages cannot invoke another Action, acquire a guard, commit managed state,
or enlarge Session authority.

M0-B verification covers wire framing, strict message validation, typed
authority representation, operation-specific context shape, semantic sets,
authorized completion submissions, request and recovery-state order, and the
completion handshake. It does not cover real filesystem access or cleanup,
Snapshot materialization, actual binding existence, production context
construction, process isolation, runtime redaction, durable risk ACKs, or any
Artifact, Snapshot, Migration, Restore, Run, or Instance commit. The production
materialization, context and commit evidence below comes from later runtime tests,
not the M0-B verifier. Their owning requirements retain the operation-specific
contracts. Process isolation remains conditional under PR-REQ-0171 and is not
claimed by this coverage.

The Rust verifier owns raw JSON, duplicate-property and Unicode rejection,
closed schemas, authority and path validation, semantic-set normalization, and
the modeled state machine. The independent Node 24 oracle consumes only valid
fixtures and independently produces preamble bytes, frame bytes, and normalized
valid-fixture state. It does not prove duplicate rejection and MUST NOT consume
Rust-produced expected bytes as calculation input.

**Verification: PR-TEST-0025, PR-TEST-0026, PR-TEST-0029, PR-TEST-0031, PR-TEST-0032, PR-TEST-0249, PR-TEST-0258, PR-TEST-0312, PR-TEST-0328, PR-TEST-0404.**


### PR-REQ-0321 - V2 transport and persistent authority

Use the V1 reliable full-duplex dedicated channel, discovery ABI, strict JSON
rules, 16 MiB frame limit, cancellation/diagnostic grammar and request IDs.
Each direction has exactly this preamble followed by length-prefixed frames:

```text
ASCII("pactrun.hook-protocol\0") U16BE(version_byte_length) ASCII("1.0-alpha.1")
U32BE(json_payload_length) json_payload_utf8 ...
```

Both session_start and session_ready confirm the exact string `1.0-alpha.1`.
There is no negotiation, version inference or fallback. Service-free Hooks use
the same baseline with an empty service_authorities set; authority-bearing
Hooks receive only the exact grants qualified at admission.

```text
Complete HookSessionSpec additionally requires:
  protocol_version: "1.0-alpha.1"
  service_authorities: ServiceAuthorityV2[]
  target_commit?: { handle: AuthorityHandleV1 }

ServiceAuthorityV2 =
  { handle, reference: ServiceReferenceV2 with kind=storage,
    mode: read | write, path: HostNativeAbsolutePath }
  | { handle, reference: ServiceReferenceV2 with kind=resource,
      mode: read | write, path: HostNativeAbsolutePath,
      resource_kind: file | directory }

session_ready = { type: session_ready, protocol_version: "1.0-alpha.1", session_id }
```

All otherwise referenced V1 Session, Input/output, Snapshot candidate/content,
parameter and operation structures retain their exact fields and meanings.
`service_authorities` is required, even empty, and sorted by the Core reference
key. Handles are fresh Session-local opaque 32-lowercase-hex IDs, unique across
all authority classes including target_commit. No allocation ID or durable
database handle is sent. The complete authority set must exactly match the
admitted declaration; requests cannot expand it dynamically.

A storage path denotes the same persistent allocation across Runs. A file
resource path denotes that one file; a directory path grants its subtree. These
are live paths, not Workspace copies, Input bindings or Snapshot contents.
Write includes read and permits creation/removal inside the granted scope; it
does not authorize changing Pactrun association/lifetime records. Missing
ancestors outside the granted scope cannot be created by a resource-only grant.
Grant a containing directory/storage explicitly when that work is needed.
Paths are scalar-Unicode native absolute paths under the existing wire type;
unrepresentable native names fail before Session start, never lossy conversion.

Authority remains distinct from presence prerequisites under PR-REQ-0244: a
read grant can name an absent resource without asserting Present. Authors needing
Present declare that prerequisite. This is not the human `locate --intent read`
operation, whose separate contract requires an existing matching object.

Pactrun validates the owned root and existing path prefixes without following
symlinks/reparse points. The live service can still race or replace objects after
observation. Handles describe Pactrun-mediated contract authority, not an OS
file-descriptor sandbox, immutable file identity, read-only mount or writer lock.
No user exposure setting enlarges Hook authority and no Hook grant authorizes a
user path disclosure. Protected allocations survive process/Run termination.

**Verification: PR-TEST-0360, PR-TEST-0362, PR-TEST-0363, PR-TEST-0364, PR-TEST-0365, PR-TEST-0368, PR-TEST-0369, PR-TEST-0370, PR-TEST-0385, PR-TEST-0386, PR-TEST-0426.**

Codec, authority and ordinary-runtime evidence: explicit V2 preamble/handshake, strict common and
target-proposal message decoding, and rejection of V1 fallback. The outgoing
Session builder matches materialized grants to independently supplied admitted
declarations and checks handles across all existing authority classes, new
service grants and target_commit. Codec tests alone are not completed
Migration target-publication evidence. PR-TEST-0365
exercises the dedicated V2 channel against a separate process, without claiming
durable admission, live service authority or target publication.
PR-TEST-0369 covers an installed Core baseline Action explicitly selecting V2 through
the real supervisor, durable risk requests and ordinary completion. It does not
establish ServiceStorage-backed or transforming Migration execution.
PR-TEST-0370 covers real ServiceStorage-backed V2 Actions through admission pins,
native path qualification, supervision and terminal publication, including
Instance isolation, absent prerequisites, live bytes outside Inputs/Snapshots and
success-with-Open refusal. The same test executes V2 Capture/Restore over an
explicitly transformed recovery representation, preserving a guard on Capture
override and clearing it only at successful Restore publication. Migration,
comprehensive crash/compatibility coverage and installer activation have their
own evidence below and in the milestone implementation record.

### PR-REQ-0322 - Target proposal and terminal ordering

For a Migration edge containing a resource transform, Session start includes
target_commit. All other Sessions omit it. On such an edge:

- EnterRecoveryRisk retains its V1 meaning and durable Open-before-ack ordering.
- ResolveRecoveryRisk is rejected while that transform Session is active. A
  target-only coherent service state cannot independently clear risk.
- Ordinary `complete` with status success is rejected, with either Clear or
  Open risk. Ordinary failure remains legal and submits no target outputs.
- A target proposal is allowed only after acknowledged Open, with no outstanding
  request, and before any terminal message. No new public risk-state rank exists.

The only additional Hook message and Pactrun receipt are:

```json
{"type":"target_ready","commit_handle":"00000000000000000000000000000001","produced_target_outputs":[]}
```

```json
{"type":"target_ready_received","commit_handle":"00000000000000000000000000000001"}
```

The sample handle stands for this Session's actual target_commit handle; it is
not a reusable fixture capability. `target_ready` has exactly these three keys,
no status, operation, diagnostics, arbitrary resource paths or risk_state. Its
output array obeys MigrationCompletionV1's all-exactly-once target handle rule.
The receipt has exactly two keys. A duplicate proposal, wrong-class/foreign
handle, missing/duplicate output handle or proposal in another Session fails
the protocol. Handles must be unique and sorted by ASCII bytes on this new
proposal wire; unsorted arrays are rejected. JSON object member order remains
insignificant and V1 completion acceptance rules remain unchanged.

Before sending target_ready, the trusted Hook must reach target coherence,
finish/close all submitted output files and finish its transformation writes.
The Package must ensure target coherence can persist without further Hook work;
an independently running service may continue target-compatible writes. Pactrun
does not quiesce or restart it. The Hook must remain available for the receipt, then exit
normally with status zero without further service mutation or protocol messages.
The receipt means syntactic/authority acceptance of a PROPOSAL only: not success,
durable output acquisition, risk resolution, target publication or Run completion.
It ends Hook messaging, but leaves the Run Running and durable risk Open.

Pactrun retains the proposal only in its owner-held continuation. It does not
persist a replayable PreparedCommit or a target-ready success flag. After observed
process-tree termination with zero status, it safely acquires the submitted
Managed Input outputs, completes execution scratch cleanup, and attempts the
Pactrun-owned target transaction in PR-REQ-0326. Service paths are never scratch
cleanup targets. Loss of the owner at any of these points is recovered solely
from durable Open risk and the last committed boundary, not from that proposal.

The target transaction clears this Run's risk together with target associations,
Revision, Inputs, state version and recovery boundary. The last edge also
publishes Run success in that transaction. No `request_ack(clear)` is sent for
target_ready; it was not ResolveRecoveryRisk. The operator observes committed
success through ordinary Run/Instance inspection. Prior manual-recovery guards
are not implicitly cleared by this Migration.

Ordinary V2 Sessions without target_commit retain V1 completion/risk ordering,
including durable Clear before resolution ack and rejection of ordinary success
with Open risk. Compatibility-only edges do not become transform edges merely
because their Hook has authority; the Pack is responsible for declaring actual
cross-representation transformations and their risk boundary.

### Failure and cancellation table

| Event | Result |
| --- | --- |
| Request/proposal validation failure | protocol failure; no target publication; apply actual durable risk consequence |
| Loss before Pactrun fully writes target_ready_received, or a reported receipt-write/transport failure | no target publication; Open risk implies manual recovery; never infer replay permission |
| Nonzero exit, supervisor termination failure, cancellation, timeout, or extra Hook message after proposal | no target publication; terminal non-success with Open risk |
| Output acquisition/required-output failure | no target publication, no Hook replay; Open risk consequence |
| Retryable storage failure while owner retained | retry the same acquired target publication only; no second Hook or new proposal |
| Permanent publication refusal or owner loss before commit | source/last committed boundary remains; manual recovery; no old service coherence claim |
| Loss after committed target boundary | committed boundary wins; do not replay; later reconciliation uses the next edge or terminal evidence |

Cancellation is owner-arbitrated against target commit. Before proposal receipt,
the existing cancel/cancel_ack exchange applies. After receipt no new Hook
messages are required; the supervisor can terminate the process. A cancellation
that wins before publication prevents publication; one after a committed final
success cannot change that durable outcome. A completed local write is not proof
of peer receipt; the trusted Hook must read the receipt before normal zero exit.
An uncertain receipt must not be guessed successful by the Hook or replayed.

V2 error proposal: owner `hook_protocol`, protocol-category codes
`invalid_frame`, `unexpected_message`, `invalid_authority`, and
`invalid_target_proposal`. Messages remain fixed safe text without raw frames,
paths or service contents. S0 did not register them; the integrated M6.5 implementation
appends these approved entries under the existing ErrorTaxonomyV1 process,
without modifying Frozen V1 codes or claiming completed V2 runtime integration.

**Verification: PR-TEST-0360, PR-TEST-0361, PR-TEST-0363, PR-TEST-0365, PR-TEST-0377, PR-TEST-0378, PR-TEST-0380, PR-TEST-0381, PR-TEST-0382, PR-TEST-0386.**

Protocol and target-runtime evidence: acknowledged Open before proposal, exact target
handle coverage, no ordinary transform success or independent risk resolution,
and proposal receipt distinct from completion with risk remaining Open. Real
transforming Hooks exercise receipt delivery, zero-exit/tree termination,
owner-held publication and failures after target writes. Process-crash tests
cover receipt and target-commit boundaries; no persisted proposal flag exists.
Comprehensive multi-edge, output, platform and compatibility closeout remains
part of the milestone and is not implied by format Freeze.
