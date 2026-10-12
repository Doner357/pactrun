---
title: Operation-scoped Machine Output
---

# Operation-scoped Machine Output

## Applicability

CLI machine interface `1.0-alpha.4` is the default JSON/JSONL contract.
The [legacy machine interface](./machine-output.md), `1.0-alpha.3`, remains
explicitly selectable. This is a separate output contract, not a reinterpretation
of existing `1.0-alpha.3` results.

The [response schema](./cli-machine.schema.json) and
[event schema](./cli-events.schema.json) describe this version. The legacy
contract retains separate schemas; selecting it does not reinterpret its fields.

### PR-REQ-0388 - Operation requirements and machine compatibility

JSON and JSONL MUST select `1.0-alpha.4` by default and MUST
also accept leading `--machine-version 1.0-alpha.3` to preserve old automation
for old-format objects. Explicit `--machine-version 1.0-alpha.4` is equivalent
to the default. The selector MUST be usable only with explicit `--format json`
or `--format jsonl`; human presentation, duplicate selectors and unsupported
versions MUST be rejected before command side effects. Version selection MUST
NOT select a different runtime contract, reinterpret a Revision or retry an
operation. A valid selected version governs failures as well as success.
Before a supported version can be selected, parse errors use the implemented
default machine envelope when machine presentation is unambiguously selected.
The format and machine-version selectors may occur in either order, but both
belong before the command. JSONL event envelopes and their final nested response
MUST use the same selected version, including error and incomplete-delivery paths.

Envelope and event framing, terminal delivery, partial results, object identity,
error ownership and Secret disclosure remain governed by `1.0-alpha.3`, except
that the selected `format_version` and the projections below change. Stored
Run facts MUST NOT be rewritten to produce a new presentation.

In `1.0-alpha.4`, Instance, Plan, initialization and Migration-boundary projections
MUST NOT expose `required_inputs_satisfied`, `predicted_required_inputs_satisfied`
or a global `missing_input_ids` as if the new Core had global readiness. New-Core
Input declarations and active binding roles MUST NOT carry a global `required`
boolean. Operation declarations instead expose their exact required/optional
lists and applicable source/target context.

An operation requirement assessment MUST have these fields:

```text
InputRequirementAssessment
|- operation: OperationRequirementSelector
|- state: satisfied | missing | not_evaluated
`- missing: MissingInputReference[]

MissingInputReference
|- context: current | snapshot | source | target
|- role: active | retained
`- input_id: InputIdentity
```

The closed selector union is:

| kind | Additional fields |
| --- | --- |
| `action` | `action_id` |
| `snapshot_capture` | None |
| `snapshot_restore` | `snapshot_id` (nullable before selection) |
| `migration` | `source_revision`, `target_revision` |
| `cleanup` | None |

Identifiers MUST use existing machine identity encodings. Instance inspection
MUST expose `input_requirements`, an array covering declared Actions, Capture,
Restore and Cleanup. Restore without a selected Snapshot is `not_evaluated`.
Migration assessments belong to a selected path/edge's Plan, not an invented
Instance-wide enumeration. Plan assessments MUST bind their existing exact
Instance/Revision observations; multi-edge Migration retains edge order.
Required facts supplied by not-yet-acquired operator values, earlier Hook outputs,
or a future edge context are `not_evaluated`, not an optimistic presence claim.
An empty Migration requirement set imposes no Input prerequisite.

`missing` MUST be empty for `satisfied` and `not_evaluated`, and nonempty for
`missing`. References MUST be unique and sorted by `(context, role, input_id)`.
Instance assessments MUST sort by `(kind, action_id)` with absent action IDs
sorting as the empty string. Missing optional Inputs MUST NOT appear as unmet
requirements. When required facts are unavailable, `not_evaluated` MUST NOT
be replaced by a false negative or an optimistic satisfied result.

Input satisfaction MUST NOT imply full execution eligibility. Other admission
refusals and diagnostics remain separate. Human output follows the same
distinction without making its layout part of this machine contract.

For old-Core objects projected in `1.0-alpha.4`, the Instance projection MUST
add `legacy_required_inputs: {satisfied: boolean, missing_input_ids: InputIdentity[]}`;
for new-Core objects this field MUST be absent. Old Action/Capture assessments
derive from their old global requirements; old Restore, Migration and Cleanup
continue using their original operation contracts. Old Input declarations may
retain `required` because it remains a real fact about that old format.

When explicitly selecting `1.0-alpha.3`, supported old-format result fields,
requiredness, values and meaning MUST remain unchanged. Commands whose result
requires new-Core semantics MUST be refused before mutation or Hook launch,
not projected with a fabricated global boolean or silently upgraded envelope.
Read-only mixed collections MUST refuse rather than omit incompatible objects.
Commands independent of the new semantics remain usable in either version.
Predictable output-contract incompatibility is a preflight refusal, not an
output failure discovered after successful execution. Concurrent invalidation
continues to follow existing admission and output-failure rules.

#### Ordinary artifacts and common capability views

Operation-scoped declarations expose their required/optional Input grants and
their parameter/output declarations. Plan projections expose parameter identities
and effective redaction, not invocation values. Ordinary output identities remain
distinct from Migration target Input outputs. Old-format declaration projections
retain their genuine legacy requirement fields rather than inventing read grants.

Run inspection adds `published_artifacts`, including artifacts committed by an
earlier Migration edge or by Cleanup before final retirement. Finished Run artifact
entries and Artifact command results include an `edge` object only for Migration:
`{source_revision_digest, target_revision_digest}`. Both digests identify one
exact edge in the retained Run path. Singleton-operation artifacts omit `edge`.

`run artifact export` and `run artifact delete` accept
`--edge <source-digest>/<target-digest>`. Unique prefixes are resolved jointly
against the retained Run's edges, never against current installations or display
ordinals. Absence or ambiguity refuses the operation. A same-name report on
another edge MUST NOT be exported or deleted as a fallback. Scoped results require
machine `1.0-alpha.4`; incompatible legacy requests fail before file publication
or artifact deletion. Existing unscoped byte export retains its explicit
sensitive-export authorization and destination-publication rules.

**Verification: PR-TEST-0726, PR-TEST-0727, PR-TEST-0728, PR-TEST-0729.**
