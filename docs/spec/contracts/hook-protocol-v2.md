---
title: Hook Protocol V2
---

# Hook Protocol V2

**Status: Frozen normative Package contract specification.**

These independently versioned requirements do not change V1 message meanings.
The target proposal below is deliberately NOT a successful Hook completion with
Open risk. The [format review](../../development/design-notes/m6-5-format-activation-review.md)
records the approved interpretation and conformance gate; milestone completion
requires separate final runtime verification.

<!-- spec-navigation:start -->
## Reading map (informative)

Read [Hook V1](./hook-protocol-v1.md), [Core V2](./revision-core-format-v2.md),
[target publication](../execution/m6-5-service-storage-execution.md), and
[S0 review gates](../../development/design-notes/m6-5-servicestorage-baseline.md).
PR-REQ-0056/0057/0246 remain the owning recovery constraints.
<!-- spec-navigation:end -->

### PR-REQ-0321 - V2 transport and persistent authority

Use the V1 reliable full-duplex dedicated channel, discovery ABI, strict JSON
rules, 16 MiB frame limit, cancellation/diagnostic grammar and request IDs.
Each direction has exactly this preamble followed by length-prefixed frames:

```text
ASCII("pactrun.hook-protocol\0") U32BE(2)
U32BE(json_payload_length) json_payload_utf8 ...
```

Both session_start and session_ready select protocol_version exactly 2. No
negotiation or fallback occurs. V1 and V2 transports/decoders must not guess a
peer's version. A V2 Hook can operate under Core V1 when it needs no new declared
authority; Core V2 can select a V1 Hook with no V2-only authority/prerequisites.

```text
HookSessionSpecV2 = HookSessionSpecV1 with:
  protocol_version: 2
  service_authorities: ServiceAuthorityV2[]
  target_commit?: { handle: AuthorityHandleV1 }

ServiceAuthorityV2 =
  { handle, reference: ServiceReferenceV2 with kind=storage,
    mode: read | write, path: HostNativeAbsolutePath }
  | { handle, reference: ServiceReferenceV2 with kind=resource,
      mode: read | write, path: HostNativeAbsolutePath,
      resource_kind: file | directory }

session_ready = { type: session_ready, protocol_version: 2, session_id }
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

**Verification: PR-TEST-0360, PR-TEST-0362, PR-TEST-0363, PR-TEST-0364, PR-TEST-0365, PR-TEST-0368, PR-TEST-0369, PR-TEST-0370, PR-TEST-0385, PR-TEST-0386.**

Codec, authority and ordinary-runtime evidence: explicit V2 preamble/handshake, strict common and
target-proposal message decoding, and rejection of V1 fallback. The outgoing
Session builder matches materialized grants to independently supplied admitted
declarations and checks handles across all existing authority classes, new
service grants and target_commit. Codec tests alone are not completed
Migration target-publication evidence. PR-TEST-0365
exercises the dedicated V2 channel against a separate process, without claiming
durable admission, live service authority or target publication.
PR-TEST-0369 covers an installed Core V1 Action explicitly selecting V2 through
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

V2 error proposal: owner `hook_protocol_v2`, protocol-category codes
`invalid_frame`, `unexpected_message`, `invalid_authority`, and
`invalid_target_proposal`. Messages remain fixed safe text without raw frames,
paths or service contents. S0 did not register them; the M6.5 feature branch now
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
