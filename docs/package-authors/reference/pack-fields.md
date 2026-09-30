---
title: Pack fields and values
---

# Pack fields and values

Use this reference while editing `pactrun.yaml`. The paths below are YAML paths:
`[]` means one sequence entry, and `?` in a shape means an optional property.
Fields not listed by the selected shape are rejected. Omission uses a documented
default; it does not mean that arbitrary `null` values are accepted.

Start with the [complete runnable Pack](../fundamentals/authoring-model.md).
For the exact grammar, portable metadata shapes, and path rules, use the
[complete source reference](./source-format.md). Advanced normalized declaration
rules are in [declaration types and identity](./revision-format.md).

## Root and runtime files

| Field | Type / accepted value | Required or default | Meaning |
| --- | --- | --- | --- |
| `source_format` | String `"1.0-alpha.1"` | Required | Selects the supported source format; numeric development versions are refused. |
| `package_id` | 32 lowercase hexadecimal characters | Required | Stable lineage ID. Generate once with `pactrun pack generate-id`; keep it across Revisions of this Pack. |
| `revision` | Object | Required; `{}` is valid | Operational declarations below. Do not add `revision.format_version`; installation supplies it. |
| `runtime_content` | Object | Required; `{}` is valid | Files Pactrun acquires into the installed Revision. |
| `runtime_content.files` | Sequence | Empty when omitted | Runtime file declarations. |
| `runtime_content.files[].id` | Semantic ID | Required | ID used by Hook `script` or `executable` references. |
| `runtime_content.files[].source` | Source-relative path | Required | File beneath the Pack directory, read during installation. |
| `runtime_content.files[].path` | Portable logical path | Required | Path within owned installed content; independent of the original host source. |
| `runtime_content.files[].executable` | Boolean `true`, `false` | `false` | Required to be `true` for a direct executable. Script launch does not use this flag to choose an interpreter. |
| `portable_metadata` | Object | Empty when omitted | Labels, presentation, and provenance; never operational configuration. |

Runtime files are copied and identified at installation. Later edits to the
source directory do not alter an installed Revision. Exported Packs must include
their complete declared runtime content. Do not supply derived `blob_digest`
or file `kind` in a source file declaration.

## Identifiers and paths

Semantic IDs such as `config`, `inspect`, or `app.database` use
`^[a-z][a-z0-9]*(?:[._-][a-z0-9]+)*$`, with 1–128 ASCII bytes. Keep an ID's
meaning stable within its Package lineage. This is different from generated
hexadecimal Package, Run, or Snapshot IDs.

Runtime logical paths use `/`-separated semantic-ID segments, at most 1024 bytes.
No empty, `.` or `..` segment, absolute path, drive prefix, backslash, or Windows
reserved device name is allowed. Source acquisition also rejects symlinks and
filesystem aliases. Service locators have a different grammar; see
[service fields](./service-fields.md).

## Revision collections

| Field under `revision` | Omission | Contents |
| --- | --- | --- |
| `inputs` | Empty sequence | Persistent managed bindings. |
| `actions` | Empty sequence | User-invoked operations. |
| `migrations` | Empty sequence | Inbound transitions from exact Revisions in this Package. |
| `service_storages` | Empty sequence | Owned storage allocations for service data. |
| `service_resources` | Empty sequence | Declared files/directories within those allocations. |
| `snapshot` | Capability absent | Optional `capture` and `restore` declarations. |
| `cleanup` | Capability absent | Pack-specific work before Instance deletion. |

Sequences may be empty. Supply maps/sequences with their declared types; `null`
is not shorthand for an omitted capability or an empty collection.

## Inputs: `revision.inputs[]`

| Field | Values | Required or default | Meaning |
| --- | --- | --- | --- |
| `id` | Semantic ID | Required | Stable binding name. |
| `required` | `true`, `false` | `false` | `true` requires a binding for ordinary Actions and Capture. Creation may still leave the Instance incomplete. |
| `protection` | `normal`, `secret` | `normal` | `secret` applies protected handling/disclosure. It does not encrypt the host or sandbox a Hook. |

Input values are opaque bytes, not typed parameter values. Once bound, an active
required Input cannot be ordinarily deleted. Secret protection is sticky:
changing a declaration to `normal` does not declassify existing Secret bytes.
Migration and Cleanup declare their own binding requirements.

```yaml
# Place this property directly inside revision.
inputs:
  - id: config
    required: true
    protection: normal
```

## Actions: `revision.actions[]`

| Field | Values | Required or default | Meaning |
| --- | --- | --- | --- |
| `id` | Semantic ID | Required | Name used by `pactrun invoke`. |
| `access` | `observe`, `mutate` | Required | `observe` uses a pinned read context without a reader lock; `mutate` serializes/conflicts with mutations on the same Instance. Neither freezes external service bytes. |
| `parameters` | Sequence | Required; `[]` is valid | Invocation-only typed values. |
| `outputs` | Sequence of `{id: <semantic-id>}` | Required; `[]` is valid | Declared managed-output slots. The Hook must explicitly register produced outputs. |
| `hook` | Hook object below | Required | How the operation runs and which service authorities it receives. |

Any Action that writes service resources needs `mutate` and explicit write
authority. An Action name or description grants no authority. Snapshot, Migration,
and Cleanup are separate capabilities; do not simulate them as ordinary Actions.

## Parameters: `parameters[]`

These fields apply to Action, Capture, and Restore parameters.

| Field | Values | Required or default | Meaning |
| --- | --- | --- | --- |
| `id` | Semantic ID | Required | Name supplied by an operator or read by a Hook. |
| `type` | `integer`, `float`, `boolean`, `string` | Required | Determines parsing and the default's type. |
| `sensitive` | `true`, `false` | Required | Whether values/defaults require protected presentation; never print sensitive values in diagnostics. |
| `default` | Value matching `type` | Optional | Present: operator may omit the parameter. Absent: operator must supply it. |

| Type | Authoring value and limits |
| --- | --- |
| `integer` | Exact integral value from −9,007,199,254,740,991 through +9,007,199,254,740,991. YAML numeric defaults such as `1`, `1.0`, and `1e0` normalize to the same integer. |
| `float` | Finite binary64 value; overflow is rejected, underflow to zero is allowed, negative zero normalizes to zero. |
| `boolean` | Unquoted lowercase `true` or `false`. A quoted Boolean is a string and is rejected for this type. |
| `string` | Exact Unicode text, preserving spaces and normalization differences. Quote ambiguous-looking values for readability. |

There are no enum/range/regex parameter-declaration fields. Validate any
application-specific constraints in your Hook. Operator CLI text has its own
lexical rules: Boolean text is exactly `true`/`false`; integer text is whole
decimal notation. Do not assume every YAML-default spelling works as CLI text.

For operator-facing instructions, integer text must match the whole expression
`-?(0|[1-9][0-9]*)` within the same safe-integer range. Float text must match
`-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?` and convert to finite binary64.
There is no leading plus, invalid leading zero, surrounding whitespace, `NaN`,
or `Infinity`. Integer `1e0` is refused at invocation even though it is a valid
integer YAML default. Negative zero becomes zero; float underflow may become zero.
String invocation values preserve exact text, including the empty string.

```yaml
# Replace the chosen Action's parameters property with this sequence.
parameters:
  - id: message
    type: string
    sensitive: false
    default: hello
```

Read a bound value using `pactrun hook parameter message`; it returns a JSON
scalar. Parse JSON rather than evaluating the result as shell code.

## Hooks: `hook`

| Field | Values | Required or default | Meaning |
| --- | --- | --- | --- |
| `protocol_version` | FormatVersion string; this runtime supports `"1.0-alpha.1"` | Required | Identity-bearing protocol selection. Numeric `1` or `2` is not accepted. |
| `launch` | One closed alternative below | Required | Direct executable, interpreter, or built-in Shell Loader. |
| `args` | Ordered string sequence | Required; `[]` is valid | Literal argument tail; no shell interpolation by Pactrun. |
| `io` | `{terminal: <value>}` | Required | Pack-facing terminal contract. |
| `service_access` | Sequence | Empty when omitted | Explicit read/write grants; see [service fields](./service-fields.md). |
| `service_requires` | Sequence | Empty when omitted | Presence prerequisites, which do not grant authority. |

Protocol labels use `Major.Minor`, optionally followed by `-alpha.N`, `-beta.N`,
or `-rc.N`. Components are canonical unsigned 64-bit decimal integers without leading
zeroes; prerelease `N` starts at one. Labels are ASCII and at most 128 bytes.
Syntactic validity and runtime support are separate: a service-free declaration
can preserve an unsupported label while its Action fails compilation. Service
authorities and transforms also require supported protocol capabilities during
declaration validation. Use `"1.0-alpha.1"` for runnable Hooks in this candidate;
installation alone does not prove that every operation can run.

| `io.terminal` | Meaning |
| --- | --- |
| `none` | No user terminal channel. |
| `output` | Output streams without interactive input. |
| `interactive` | Bidirectional terminal. |

The Hook protocol channel is always separate. There is no default terminal mode.
Interactive Actions cannot also acquire a parameter from stdin, even for a plan.

| `launch.kind` | Other required fields | Behavior |
| --- | --- | --- |
| `direct` | `executable` | Runtime Content ID of a file marked executable; it implements the Hook protocol. |
| `interpreter` | `command`, `interpreter_args`, `script` | Host program plus argument sequence and owned script Content ID; the script implements Hook integration. |
| `shell_loader` | `shell`, `command`, `script` | Pactrun manages the Session; your script calls helpers. No `interpreter_args` field in this alternative. |

`command` is a host executable filename such as `sh`, `bash`, or `pwsh.exe`, not
a path or command string. It is 1–128 ASCII bytes and matches
`^[A-Za-z0-9](?:[A-Za-z0-9._-]{0,126}[A-Za-z0-9])?$`; reserved device names and
path syntax are rejected. Windows extensions must be explicit.

Shell values are `sh` and `bash` on POSIX, or `powershell_7` and
`windows_powershell_5_1` on Windows. The interpreter must be installed. Unsupported
host/shell pairs fail; Pactrun does not silently choose another shell or override
execution policy. See the [Shell helper reference](./shell-loader.md) for exit,
failure, output registration, risk, and cancellation rules. Direct/interpreter
authors use the [message reference](./hook-protocol.md).

## Snapshot and Cleanup declarations

| Location | Fields | Meaning |
| --- | --- | --- |
| `revision.snapshot.capture` | `parameters`, `access`, `hook` | Optional Capture capability; `access` is `observe` or `mutate`. |
| `revision.snapshot.restore` | `parameters`, `hook` | Optional Restore capability; always Mutate. Do not add an `access` field. |
| `revision.cleanup` | `requires`, `hook` | Optional Cleanup capability; no invocation parameters. |
| `cleanup.requires[]` | `{role: active or retained, input_id: <id>}` | Exact binding requirements for Cleanup. |

All fields listed inside a present capability are required; empty sequences are
valid where the capability needs no parameters or bindings. Capture/Restore may
be declared independently. Follow the [Snapshot tutorial](../managed-capabilities/snapshots-and-managed-data.md)
for candidate registration and fresh-store recovery, and
[Cleanup guidance](../managed-capabilities/hooks-recovery-and-cleanup.md) for failure handling.
Check [Snapshot authoring limits](./snapshot-limits.md) before choosing payload
sizes or descriptor counts.

## Portable metadata

`portable_metadata.reference_labels`, `.presentation`, and `.provenance` are
optional sequences, empty when omitted. Labels and descriptions do not change
Revision identity, authenticate publishers, or grant permissions.

Presentation entries have `target`, `field`, and `value`. `field` is one of
`display_name`, `summary`, `description`, or `help`; `value` is a nonempty exact
string. Targets name an existing Revision capability, never an arbitrary object.

```yaml
portable_metadata:
  presentation:
    - target: {kind: revision}
      field: display_name
      value: Example service
```

The [complete metadata shapes](./source-format.md#pr-req-0258---packsourceyamlv1-schema-numbers-and-package-lineage)
list every target, attribution alternative, and required field. Local aliases,
notes, trust decisions, and installed filesystem paths are not portable source fields.

Metadata text such as labels, publisher names/namespaces, attribution text, and
presentation values is nonempty Unicode text preserved exactly: no trimming,
case folding, or Unicode normalization. These are not semantic IDs, so spaces
and non-ASCII characters are allowed. `SourceUri` instead requires an absolute
ASCII URI with a scheme, such as `https://example.test/source`; fragments are
allowed, relative references are rejected, and spelling is not normalized.
A URI here is a provenance claim, not a download instruction.

| Metadata collection / discriminator | Required fields | Optional fields / meaning |
| --- | --- | --- |
| `reference_labels[]` | `label`, `source` | `source` must be one alternative below; omission does not mean unattributed. |
| `source.kind: unattributed` | No additional fields | No attribution claim. |
| `source.kind: source_uri` | `source_uri` | Exact claimed URI. |
| `source.kind: publisher` | `publisher_name` | Optional `publisher_namespace`. |
| `source.kind: publisher_source_uri` | `publisher_name`, `source_uri` | Optional `publisher_namespace`. |
| `provenance[].kind: source_uri` | `source_uri` | URI claim attached to the installed Revision. |
| `provenance[].kind: publisher_attribution` | `publisher_name` | Optional `publisher_namespace`, `source_uri`. |
| `provenance[].kind: attribution` | `attribution_text` | Optional `source_uri`. |

Presentation targets use `kind: revision`, `input` with `input_id`, `action` with
`action_id`, `action_parameter` with `action_id` and `parameter_id`, `managed_output`
with `action_id` and `output_id`, `snapshot_capture`, `snapshot_capture_parameter`
with `parameter_id`, `snapshot_restore`, `snapshot_restore_parameter` with
`parameter_id`, `migration` with `source_revision_digest`, or `cleanup`.
No target names an arbitrary Package, Instance, Storage, or resource. All four
presentation fields are available for each supported target kind. A label that
resolves to multiple distinct Revisions is ambiguous; it does not silently choose
the newest one.

## Common refusals

- Unknown fields or a field from the wrong alternative: check the selected shape.
- Missing values: compare required fields with documented omission defaults.
- Duplicate keys/IDs, anchors, aliases, YAML tags, or multiple YAML documents: remove them.
- Changed content did not update an existing Instance: install a new Revision,
  then use a declared Migration; reinstalling does not rebind the Instance.
- A valid declaration still cannot run: inspect host programs, Inputs, parameters,
  service presence/authority, and recovery state before retrying.
