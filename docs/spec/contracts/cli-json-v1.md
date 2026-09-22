---
title: CLI JSON V1
---

# CLI JSON V1

**Status: Approved normative versioned CLI presentation contract.**

<!-- spec-navigation:start -->
## Reading map (informative)

This page owns Pactrun CLI JSON V1, not exported payloads or Hook streams.
The [schema](./cli-json-v1.schema.json) specifies command-specific result shapes.
Read [error identity](./error-taxonomy-v1.md) and
[command behavior](../behavior/command-and-output-reference.md) for their owners.
The [implementation record](../../development/cli-presentation-status.md) separates
runtime verification and Git integration from approval of this contract.
<!-- spec-navigation:end -->

This contract implements the version boundary in
[PR-REQ-0083](../foundations/resources-and-versioning.md#pr-req-0083---structured-cli-version)
and the ownership boundary in
[PR-REQ-0120](../behavior/command-and-output-reference.md#pr-req-0120---structured-output-boundary).
The [approved baseline](../../development/design-notes/cli-presentation-baseline.md)
owns delivery scope, not additional product semantics.

### PR-REQ-0358 - Explicit CLI presentation selection

The CLI MUST accept a leading `--format human` or `--format json`. Omission MUST
select human presentation. The selector MUST NOT be inferred from terminal
availability, pipes, environment variables or redirects. Duplicate selectors,
unknown values and selectors after the command MUST be usage errors. A selector
MUST NOT consume or reinterpret a command operand containing similar text.

Format recognition MUST precede ordinary parsing and cancellation-handler setup.
When a valid leading JSON selection is recognizable, subsequent usage and startup
errors MUST use JSON. An invalid first selection uses human diagnostics because
no valid presentation was selected. Existing exit codes retain their meanings.

**Verification: PR-TEST-0553, PR-TEST-0557, PR-TEST-0558, PR-TEST-0561, PR-TEST-0566.**

### PR-REQ-0359 - Versioned typed CLI response

An ordinary JSON response MUST be a single UTF-8 JSON object followed by a newline
on stdout, with `format` equal to `pactrun.cli.v1`, `command` (canonical command
name, or null before dispatch including parsing/startup failures), `status`
(`success` or `failure`), `result` (a
command-specific typed object or null) and `error` (a typed error or null).
Success MUST have a null error. Failure MUST have a non-null error and MAY retain
known partial result facts. Neither a failure status nor missing output proves
absence of side effects. No automatic retry classification is implied.

The error object contains `kind` (`usage`, `operation`, `output` or `startup`),
`message` and `reference`. Reference is the existing Pactrun error owner/code
pair when available, otherwise null. Messages are explanatory, not identities.
`kind` is a coarse presentation failure class, not a replacement error taxonomy.
When no stable reference exists, consumers must not classify a cause or infer
retryability by matching message text; they can inspect authoritative state or
request operator intervention.
Implementations MUST preserve available typed identity before prose conversion
and MUST NOT infer identities from strings or invent a Hook-owned Pactrun error.

Result models MUST be explicit projections, not parsed prose, private-struct
serialization or Debug output. Human and JSON rendering MUST use the same facts
and disclosure policy. Empty collections are arrays; null denotes an absent
optional value. Domain distinctions such as not checked, not applicable and
unknown MUST have explicit typed states where the owning result distinguishes
them; absence MUST NOT be turned into a negative assertion.

Object member order and insignificant whitespace are not contractual. Consumers
MUST tolerate additional object members. Removing or changing the type or meaning
of existing members, requiredness or existing enum values requires an explicit
interface version change. The CLI version is independent of product, persistence,
Pack, Core, Snapshot and Hook versions.

**Verification: PR-TEST-0554, PR-TEST-0556, PR-TEST-0557, PR-TEST-0558, PR-TEST-0559, PR-TEST-0560, PR-TEST-0562, PR-TEST-0563, PR-TEST-0564, PR-TEST-0565, PR-TEST-0566, PR-TEST-0567, PR-TEST-0568, PR-TEST-0569.**

### PR-REQ-0360 - Payload and terminal preservation

Presentation MUST NOT transform raw Input/Artifact bytes, transport archives or
Hook terminal streams. A raw Input export to stdout MUST NOT gain a JSON success
envelope; its Pactrun-owned errors use stderr in the selected format. Other
ordinary responses MUST NOT mix human result text or logs into stdout JSON.

JSON actual execution requiring a Hook output or interactive terminal contract
MUST fail before execution side effects and before starting the Hook. Planning
without execution MUST remain supported. This is a presentation restriction,
not a new admission/recovery rule. Human execution followed by JSON Run inspection
remains available. No new result-file destination or event stream is defined.

Output failure MUST NOT be represented as rollback. Once writing a response has
begun, an implementation MUST NOT append a replacement envelope to damaged output.
Broken pipes and forced process termination cannot guarantee a complete response.
Formatting MUST NOT introduce additional sensitive disclosure or authorization.

**Verification: PR-TEST-0555, PR-TEST-0558, PR-TEST-0559, PR-TEST-0560, PR-TEST-0561, PR-TEST-0562, PR-TEST-0563, PR-TEST-0564, PR-TEST-0565, PR-TEST-0566, PR-TEST-0567, PR-TEST-0568, PR-TEST-0569.**

## V1 representation rules

The checked-in [JSON Schema](./cli-json-v1.schema.json) is the normative shape
inventory for 62 canonical command names. Its generation test compares explicit
CLI projections to the checked-in contract; tests write a candidate only under
target/, never silently update this specification. The following meanings apply:

- Identity/digest strings and state tokens retain their owning canonical spellings.
  Revision objects contain `package_id` and `content_digest`; text reference
  arguments and Revision pagination cursors retain the `exact:` reference prefix.
- State versions, potentially large counters, byte lengths, epoch seconds and
  milliseconds use decimal strings, avoiding loss in consumers with binary64
  numbers. Bounded counts, format versions and nanoseconds use JSON integers.
- Optional members are emitted explicitly as null. Array fields are arrays,
  including when empty. `required: null` on a retained Input is not an active
  optional declaration. Null guards do not assert unconditional execution safety.
- Native filesystem paths use a tagged object: `utf8` with `value`,
  `windows_wide` with exact `units` (u16), or `unix_bytes` with exact `bytes` (u8).
  The latter two are used when Unicode conversion would lose information. The
  contract accepts all three across platforms; it never substitutes lossy text.
- Catalog `next` and Migration `next_after` are exact continuation tokens, or
  null at the end. Human `--no-trunc` does not change JSON identities or values.
- A Plan has `preview: not_admitted` (retirement: `admission: not_attempted`).
  Runtime checks and predicted completeness are not observations of live service
  bytes and do not reserve resources or establish Admission.
  Snapshot Plan `eligibility` distinguishes Capture's satisfied required Inputs
  from Restore's compiler checks passed without Admission.
- Run `state.phase` distinguishes running from finished. Finished `outcome`,
  boundary, recovery risk and failure references retain the execution contract.
  Legacy free-form failure/completion text stays withheld. Diagnostic `source`
  is `hook`, never Pactrun. A null diagnostic collection means no feature record;
  a non-closed collection has unknown tail completeness. Sequence gaps and the
  `observed` count represent omissions, not proof of non-emission. Retention
  disabled means no event text is disclosed through this projection.
- Service observations explicitly distinguish present, absent and unknown.
  Unknown is a failure with the known observation retained in `result`.
  Detached locations are null unless the existing explicit reveal flag is used;
  multiple handoff locations do not establish a coherent or running service.
- `created_instance` records successful creation, not a guarantee of current
  state after Restore. `restore_run_id` records the known Run identity;
  null `restore` means its inspection is unavailable, not that it never ran.
  A failure result containing only `run_id` preserves the known invocation
  association; without an inspection it does not assert that acceptance, a
  terminal outcome or a durable record has been established.
- Publication failures preserve the known destination and `destination_published`
  when that fact is available. True means this attempt published the final name
  before failing; it does not claim durability or rollback. False means this
  attempt did not publish it, not that the path is absent (a no-clobber collision
  can leave a pre-existing file). Consumers must not automatically repeat exports.
- Metadata is descriptive. Trust assessments and publisher/provenance claims
  do not add authentication or execution authority. Parameter projections carry
  sensitivity/default-presence facts, never sensitive default/argument values.

Successful raw Input stdout export is the one payload-only exception and has no
document to validate. Diagnostic/progress text on stderr is not a second JSON
interface or an event stream. A completed JSON document never contains that text.

## Examples (informative)

```json
{"format":"pactrun.cli.v1","command":"version","status":"success","result":{"product_version":"0.1.0"},"error":null}
```

```json
{"format":"pactrun.cli.v1","command":null,"status":"failure","result":null,"error":{"kind":"usage","message":"unknown command","reference":null}}
```

A missing `format`, an unknown interface version, a numeric `product_version`,
or `status: failure` with `error: null` is invalid. Additional object members are
accepted; key order is irrelevant. Consumers should use state/identity fields,
not parse messages, human output or Hook-authored text.
