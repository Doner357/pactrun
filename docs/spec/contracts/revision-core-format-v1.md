---
title: Revision Core Format V1
---

# Revision Core Format V1

**Status: Frozen normative Package contract specification.**

<!-- spec-navigation:start -->
## Reading map (informative)

Implement the exact Frozen normalized Core encoding and identity calculation. Do not use raw Core JSON as the public Pack authoring interface.

Start with the [specification map](../index.md)
and [shared vocabulary](../glossary.md) if a term is unfamiliar.
Check [implementation status and remaining decisions](../../development/next-milestone.md)
before treating an approved contract as available runtime behavior.
The original status, rules, exceptions, and verification declarations below retain their meaning.
<!-- spec-navigation:end -->

:::note Informative future-version scope note

The accepted `ServiceStorage` and Managed Service Resource architecture is not
part of the closed `RevisionCoreV1` schema. Formal identity-bearing declarations
require a future Revision Core format. This note adds no V1 field or semantic
meaning and changes no V1 normalization, canonical bytes, framing, digest, or
golden vector.

:::

This page defines the complete, Frozen `RevisionCoreFormatV1` identity
contract. The Candidate used these exact schema, canonical bytes, framing, and
digests without publishing them as stable identities. Freezing changed status
and verification metadata only.

## Format lifecycle

### PR-REQ-0182 - Candidate and Frozen identity

Candidate and Frozen use the same `format_version = 1` semantic schema,
normalization, canonical bytes, framing, and digest profile. Freezing is a
status change only. Candidate digests MAY appear as clearly marked conformance
data, but MUST NOT be published or persisted as stable Pactrun Revision
identities. After the format is Frozen, any identity-affecting change requires
a new Revision Core format.

**Verification: PR-TEST-0001, PR-TEST-0011, PR-TEST-0012.**

### PR-REQ-0183 - One Revision Core V1 contract

`RevisionCoreFormatV1` owns the closed Revision Core schema, the
`RuntimeContentClosureIdentityV1` representation, semantic normalization,
collection ordering, the RFC 8785 profile, framing and domain separation, and
the SHA-256 profile. `RevisionCoreV1.format_version` and the framing value
`U32BE(1)` are markers for this one contract and MUST NOT be independently
mixed. `RuntimeContentClosureIdentityV1` has no separate in-band version field.

`HookV1.protocol_version` is different: it is an identity-bearing reference to
the independently versioned Hook Protocol domain. A Hook protocol value of `1`
does not become another Revision Core format marker.

**Verification: PR-TEST-0002, PR-TEST-0011, PR-TEST-0012.**

## Closed semantic schema

### PR-REQ-0184 - RevisionCoreV1 schema

The normalized Revision Core document MUST use this closed schema. All object
keys and enum tokens shown here are lowercase ASCII `snake_case`. A field not
listed for its object is an `unknown_field` error. Required arrays are present
even when empty. Optional capabilities are omitted rather than encoded as
`null`.

```text
RevisionCoreV1
|- format_version: 1
|- inputs: InputDeclarationV1[]
|- actions: ActionV1[]
|- snapshot?: SnapshotCapabilityV1
|- migrations: MigrationV1[]
`- cleanup?: CleanupV1

InputDeclarationV1
|- id: InputIdentity
|- required: boolean
`- protection: normal | secret

ActionV1
|- id: ActionIdentity
|- access: observe | mutate
|- parameters: ParameterV1[]
|- hook: HookV1
`- outputs: ManagedOutputV1[]

ManagedOutputV1
`- id: ManagedOutputIdentity

ParameterV1
|- id: ParameterIdentity
|- type: integer | float | boolean | string
|- sensitive: boolean
`- default?: value matching type

IOContractV1
`- terminal: none | output | interactive

SnapshotCapabilityV1
|- capture?: CaptureV1
`- restore?: RestoreV1

CaptureV1
|- parameters: ParameterV1[]
|- access: observe | mutate
`- hook: HookV1

RestoreV1
|- parameters: ParameterV1[]
`- hook: HookV1

MigrationV1
|- source_revision_digest: Sha256Digest
|- transitions: MigrationTransitionV1[]
|- requires_source: InputBindingRefV1[]
|- requires_target: InputIdentity[]
|- produces_target: InputIdentity[]
`- hook?: HookV1

InputBindingRefV1
|- role: active | retained
`- input_id: InputIdentity

MigrationTransitionV1
|- { kind: carry, source: InputBindingRefV1, target_input_id: InputIdentity }
|- { kind: declassify, source: InputBindingRefV1, target_input_id: InputIdentity }
|- { kind: keep, source: InputBindingRefV1 }
`- { kind: discard, source: InputBindingRefV1 }

CleanupV1
|- requires: InputBindingRefV1[]
`- hook: HookV1
```

The presence of a parameter `default` makes that parameter optional for an
invocation; omission makes it required. V1 has no enum, range, regex, raw argv,
or extension-map parameter forms. `IOContractV1` describes only the
Pack-facing terminal contract: `none` grants no user terminal channel,
`output` grants output streams without interactive input, and `interactive`
grants a bidirectional terminal. The Hook Protocol channel always remains
separate.

**Verification: PR-TEST-0001, PR-TEST-0003, PR-TEST-0009.**

### PR-REQ-0185 - Portable identifiers and runtime paths

Every semantic identifier uses the ASCII grammar
`^[a-z][a-z0-9]*(?:[._-][a-z0-9]+)*$` and is 1 through 128 UTF-8 bytes.
Runtime paths use `/`-separated segments with the same grammar and are at most
1024 UTF-8 bytes. Empty, `.`, and `..` segments, backslashes, absolute paths,
drive prefixes, and Windows reserved device names are rejected. Device-name
matching is ASCII case-insensitive and examines the segment stem before its
first period.

`HostExecutableName` is a separate portable ASCII filename, not a semantic
identifier or path. It is 1 through 128 bytes and uses:

```regex
^[A-Za-z0-9](?:[A-Za-z0-9._-]{0,126}[A-Za-z0-9])?$
```

A period is allowed inside a name, including `pwsh.exe` and `python3.13`.
The complete names `.` and `..`, path separators, drive or URI syntax,
whitespace, leading or trailing periods, and Windows reserved device names are
forbidden. On Windows an extension such as `.exe` must be explicit; `PATHEXT`
or file-association inference is not part of the contract.

**Verification: PR-TEST-0008.**

## Semantic normalization

### PR-REQ-0186 - Collection normalization

Semantic sets MUST reject duplicate semantic keys and sort before JCS. Inputs,
Actions, parameters, managed outputs, and runtime files sort by `id`.
Migrations sort by `source_revision_digest`. Source references sort by
`(role, input_id)`. Target-reference arrays sort by `input_id`. Transitions
sort by `(source.role, source.input_id, kind, target_input_id-or-empty)` and
MUST contain no more than one disposition for a source binding. Runtime file
paths must also be unique.

Hook `args` and interpreter `interpreter_args` are ordered collections and MUST
preserve order. Unicode string comparison does not perform normalization.

**Verification: PR-TEST-0004, PR-TEST-0009.**

### PR-REQ-0187 - Unicode scalar validity

Every semantic string MUST preserve its exact Unicode scalar-value sequence.
NFC, NFD, NFKC, NFKD, case, and other Unicode normalization or repair are
forbidden. A JSON string containing malformed UTF-8, a lone high surrogate, a
lone low surrogate, or an invalid surrogate sequence MUST be rejected with
`invalid_unicode_scalar`. A valid surrogate pair represents its one non-BMP
scalar. Canonically equivalent NFC and NFD spellings remain distinct and may
produce different Revision digests.

**Verification: PR-TEST-0005.**

### PR-REQ-0188 - Raw JSON number contract

Raw JSON number syntax MUST be retained until schema validation identifies the
semantic number type. Implementations MUST NOT convert an integer default
through binary64 before integral and range validation.

An integer default is the exact mathematical value of its decimal JSON token.
It must be integral and in `-(2^53-1)` through `+(2^53-1)`. Thus `1`, `1.0`,
and `1e0` normalize to the same integer; `9007199254740991` is accepted, while
`9007199254740992` and `9007199254740993` are rejected with `invalid_number`.

A float default converts once from its decimal token to finite IEEE-754
binary64 using round-to-nearest, ties-to-even. Overflow, including `1e400`, is
`invalid_number`. Underflow to zero is allowed. Negative zero from `-0` or
`-0.0` semantically normalizes to positive zero before JCS.

Fixed integral schema values, including format and protocol versions, use the
same exact mathematical integral check. A Hook protocol version must be in
`1..=2^53-1`; it is not fixed to HookProtocolV1.

**Verification: PR-TEST-0006, PR-TEST-0007.**

## Hooks and lifecycle capabilities

### PR-REQ-0189 - Identity-bearing Hook launch

Every Hook uses this closed identity-bearing representation:

```text
HookV1
|- protocol_version: PositiveVersion
|- launch:
|  |- { kind: direct, executable: ContentId }
|  `- {
|       kind: interpreter,
|       command: HostExecutableName,
|       interpreter_args: string[],
|       script: ContentId
|     }
|- args: string[]
`- io: IOContractV1
```

A direct executable must reference an owned regular file with
`executable = true`. An interpreter script must reference an owned regular
file; its executable bit does not control interpreter launch. The selected
Content IDs, host command, interpreter arguments, Hook arguments, protocol
version, and I/O contract are identity-bearing.

The launcher process `argv[0]` is not Pack-facing, is not Revision identity,
and MUST NOT be relied upon by a Hook. The direct argument tail is `args`. The
interpreter argument tail is `interpreter_args`, followed by the materialized
script path, followed by `args`. Golden vectors verify representation and
ordered fields, not actual operating-system argument delivery.

**Verification: PR-TEST-0001, PR-TEST-0002, PR-TEST-0008, PR-TEST-0009.**

### PR-REQ-0190 - Typed source and target references

Migration `requires_source`, Cleanup `requires`, and every Migration transition
source use `InputBindingRefV1`. `requires_target` and `produces_target` remain
distinct target Input identities. Carry and Declassify have a typed source and
one target; Keep and Discard have only a typed source. Existing Migration
transition meanings do not change.

Intrinsic target validation checks closed shapes, identity syntax, unique and
non-conflicting roles, target declarations, single-writer rules, and
requires-target/produces-target disjointness. Whether a source reference is
active or retained is a relational predicate over the exact source Revision:
active means declared there and retained means not declared there. Without the
exact source semantics this predicate is `not_evaluated`, not valid or invalid,
and canonicalization and digest calculation remain available.

RevisionCoreFormatV1 does not make exact source availability an installation
prerequisite. That installation policy requires a separate decision. Exact
source semantics and successful relational validation are nevertheless
required before an edge enters an executable Plan. Actual binding existence and
its current role are Admission checks against the one `ManagedInputBindings`
registry; they are outside format-vector coverage.

**Verification: PR-TEST-0004.**

### PR-REQ-0191 - Snapshot capability projection

Capture and Restore are separate optional fields. Each owns its parameter
schema and Hook. Capture additionally owns `access: observe | mutate`. Restore
is always Mutate and MUST NOT encode a redundant access field.

V1 defines no author-configurable `execution_requirements` vocabulary. Capture
adds `RequiredInputsSatisfied`, valid parameters, and available Hook runtime to
the global managed-execution prerequisites. Restore adds exact producer
Revision compatibility, valid staged Snapshot bindings, valid parameters, and
available Hook runtime. These capability prerequisites are additive to, not
replacements for, global Run creation, trust, exact-reference, stale-state,
policy, environment, pin, guard, Plan invalidation, and recovery invariants.

The fixed prerequisites are semantics of the Capture and Restore schema tokens.
A future author-configurable requirement vocabulary is a Revision Core format
evolution. This format does not define Snapshot authoritative bodies or
`SnapshotIntegrityFormatV1`.

**Verification: PR-TEST-0003, PR-TEST-0225, PR-TEST-0227, PR-TEST-0231, PR-TEST-0235, PR-TEST-0241, PR-TEST-0246, PR-TEST-0258, PR-TEST-0270.**

Runtime coverage is partial: these M4 tests exercise authored Capture access,
required-input admission, and exact runtime/binding-fact validation. They do not
establish complete Snapshot Hook execution. S4 adds operation-specific typed
parameter/default/redaction handling, supported protocol/runtime qualification,
and staged Restore transition checks. Hook sessions and successful results
are still tested in the later lifecycle slices.
The Frozen projection and fixed prerequisite semantics above are unchanged.

### PR-REQ-0194 - Runtime launcher integration

For an interpreter Hook, the Compiler MUST perform the ordered host-launcher
selection defined by PR-REQ-0274 and bind the ordered search configuration,
`HostExecutableName`, and selected exact absolute candidate path into the
Execution Plan. Search directories and the selected candidate path are
execution-environment facts, not Revision identity.

Admission MUST repeat the same ordered selection and host eligibility check and
confirm that it still selects the exact candidate path bound into the Plan. It
MUST NOT silently select another search directory. This revalidation does not
compare a symlink or reparse target object identity, inode, file ID, canonical
target path, metadata snapshot, content digest, or any other replacement
witness. On Windows, Admission MUST apply the same non-directory target rule
without parsing PE or another image type and without using `GetBinaryType` or a
similar executable-image probe.

Executor MUST launch the exact admitted path without searching again. On
Windows it MUST pass that exact path as the `CreateProcessW` application path;
actual image launchability is decided by `CreateProcessW`. An explicit
`lpApplicationName` launch is not a shell-redirection request; the Executor may
apply an explicit fail-closed image guard to a batch-suffixed candidate without
changing the exact-path rule. If process launch fails, Pactrun MUST report
launch or execution failure and MUST NOT fall back to a later launcher-search
directory. Foreground interactive cancellation is covered by the platform
process-group/console contract and remains owner-arbitrated rather than
becoming a Hook-owned escape path.

Direct launch selects the materialized owned executable. Runtime implementations
must deliver the specified argument tail while treating `argv[0]` as
platform-controlled and outside the Pack-facing contract.

Any future requirement to detect in-place candidate replacement, symlink or
reparse retargeting, or a changed object behind the same candidate path requires
a separate launcher object-identity and replacement-detection semantic closure.
This requirement does not implicitly define one.

**Verification: PR-TEST-0125.**

This requirement defines runtime behavior but is intentionally not claimed by
Revision Core golden vectors. It requires future Compiler, Admission, and
Executor integration tests.

**Verification: PR-TEST-0081, PR-TEST-0090, PR-TEST-0095, PR-TEST-0101,
PR-TEST-0102, PR-TEST-0134, PR-TEST-0135, PR-TEST-0136, PR-TEST-0137,
PR-TEST-0138, PR-TEST-0139, PR-TEST-0161, PR-TEST-0164.**

### PR-REQ-0274 - Host launcher candidate eligibility

Interpreter launcher pathname resolution MUST follow the host operating
system's normal filesystem pathname semantics for every component. A symlink or
reparse point is not by itself a rejection reason. Candidate eligibility is
evaluated against the target reached by normal pathname resolution, including
when an intermediate component is a symlink or reparse point.

The Compiler MUST append the exact `HostExecutableName` to each
Pactrun-provided absolute search directory in order and select the first
eligible candidate. The selected Plan path MUST remain that exact absolute
candidate path. Pactrun MUST NOT canonicalize it to a symlink or reparse target
path or derive filesystem object identity, canonical-target identity, inode,
file ID, metadata, content-digest, or replacement-detection semantics from the
selection.

On POSIX, the resolved target MUST be a regular file and MUST have execute
access under the same execution credentials and host access-control semantics
used for the Hook process. The Compiler MUST NOT prevalidate ELF, Mach-O,
shebang, or another executable image format.

On Windows, the resolved target MUST be a non-directory file. The Compiler MUST
NOT parse PE or another image type and MUST NOT use `GetBinaryType` or a similar
API to create an additional executable-image contract. The
`HostExecutableName` and candidate filename MUST match exactly; Pactrun MUST
NOT append `.exe`, use `PATH` or `PATHEXT`, invoke file associations, or perform
extension inference.

**Verification: PR-TEST-0081, PR-TEST-0138, PR-TEST-0139, PR-TEST-0140,
PR-TEST-0141, PR-TEST-0161, PR-TEST-0162, PR-TEST-0164.**

## Runtime content, canonical bytes, and digest

### PR-REQ-0192 - RuntimeContentClosureIdentityV1

The second canonical component uses this closed schema:

```text
RuntimeContentClosureIdentityV1
`- files: RuntimeFileV1[]

RuntimeFileV1
|- id: ContentId
|- path: RuntimePath
|- kind: regular_file
|- blob_digest: Sha256Digest
`- executable: boolean
```

Only regular files are supported. File IDs and paths are unique. The
`executable` boolean is runtime-semantic identity; timestamps, source paths,
inodes, ownership, and full POSIX modes are excluded. This component has no
independently evolvable `format_version` field.

**Verification: PR-TEST-0001, PR-TEST-0008, PR-TEST-0011.**

### PR-REQ-0193 - JCS, framing, digest, and conformance

After Pactrun semantic normalization, both components use RFC 8785 JCS bytes.
The normative authority is this specification, RFC 8785, and Pactrun golden
vectors. `serde_jcs = "=0.2.0"` is an implementation dependency, not an
authority. A disagreement requires correcting or replacing the implementation;
the specification and expected digest MUST NOT be changed merely to fit a
library.

The digest input is exactly:

```text
ASCII("pactrun.revision-content-digest\0")
U32BE(1)
ASCII("revision-core\0")
U64BE(len(core_jcs))
core_jcs
ASCII("runtime-content-closure\0")
U64BE(len(content_jcs))
content_jcs
```

The result is SHA-256 rendered as `sha256:` plus 64 lowercase hexadecimal
characters. Rust and the independent Node 24 oracle MUST separately produce
identical component bytes, frame bytes, and digest for every valid parity
vector. Node consumes checked-in normalized vector inputs, never Rust-produced
bytes or digests, and does not claim duplicate-property validation.

Raw objects with duplicate properties are `duplicate_property`. Validation
vectors SHOULD isolate one violation. Unless a separate requirement defines a
necessary order, a document with multiple independent errors establishes no
stable first-error precedence.

**Verification: PR-TEST-0010, PR-TEST-0011, PR-TEST-0012.**

## Golden-vector coverage boundary

Golden vectors cover closed representation, raw JSON validation, intrinsic
semantic validation, explicitly supplied-source relational validation,
normalization, JCS, framing, and digest. They do not cover host executable
search, actual executability, Compiler Plan binding, Admission revalidation,
OS process creation, `argv[0]`, argument delivery, runtime retained-binding
existence, or Snapshot managed-execution admission.

The Rust verifier owns raw JSON, duplicate rejection, closed-schema and semantic
validation, normalization, canonicalization, framing, and digest. The Node 24
oracle owns independent canonicalization, framing, SHA-256, and valid-vector
parity. It is not independent proof of raw duplicate rejection.
