---
title: Revision Format and Identity Encoding
---

# Revision Format and Identity Encoding

## Supported baseline

The only supported canonical marker is the string `format_version: "1.0-alpha.1"`.
The complete root requires inputs, actions, migrations, service_storages and
service_resources; snapshot and cleanup remain optional. Runtime closure is a
separate canonical component governed by this same domain, not a ninth format.
All service declarations and explicit shell-loader alternatives below belong to
this format. Numeric format markers are not supported.

The digest frame is exactly domain label, Core label, U64BE Core byte length,
Core JCS, runtime-closure label, U64BE runtime byte length and runtime JCS.
There is no redundant integer version field. Hook versions are independent
identity-bearing FormatVersion strings. Producer product versions, metadata,
source paths and ZIP encodings do not enter Revision identity.

Format changes and existing-object compatibility follow the
[compatibility contract](../storage/compatibility.md).

Identity needs canonical bytes, not just repeatable output from one serializer.
[RFC 8785 (JCS)](https://www.rfc-editor.org/rfc/rfc8785) supplies the JSON
canonicalization used after Pactrun's semantic normalization. The Protocol Buffers
explanation of [deterministic versus canonical serialization](https://protobuf.dev/programming-guides/serialization-not-canonical/)
shows why a serializer's stable output alone is not a durable identity contract.
Pactrun additionally fixes the normalized collections, digest framing and golden
vectors described below.

## Format lifecycle

### PR-REQ-0182 - Published identity stability {#pr-req-0182---candidate-and-frozen-identity}

The baseline defines one exact set of canonical bytes, normalization, framing
and digest rules. A change to any identity-bearing rule requires a distinct
format label once this baseline is established. Product promotion is metadata,
not permission to regenerate or relabel existing Revision objects. Conformance
vectors are fixtures, not installed Revisions; authoritative publication still
requires full content validation and the owning persistence transaction.

**Verification: PR-TEST-0001, PR-TEST-0011, PR-TEST-0012.**

### PR-REQ-0183 - Revision format ownership {#pr-req-0183---one-revision-core-v1-contract}

`RevisionCoreFormat` owns the closed Revision Core schema, the
`RuntimeContentClosureIdentityV1` representation, semantic normalization,
collection ordering, the RFC 8785 profile, framing and domain separation, and
the SHA-256 profile. `format_version` is the exact in-band string; the frame
has no second integer version marker. `RuntimeContentClosureIdentityV1` has no separate in-band version field.

`HookV1.protocol_version` is different: it is an identity-bearing reference to
the independently versioned Hook Protocol domain. A Hook protocol identifier does not become another Revision Core format marker.

**Verification: PR-TEST-0002, PR-TEST-0011, PR-TEST-0012.**

## Closed semantic schema

### PR-REQ-0184 - Revision Core schema {#pr-req-0184---revisioncorev1-schema}

The root fields are listed below. PR-REQ-0317 defines service declarations and
the complete Hook and Migration records used by this same schema. All object
keys and enum tokens shown here are lowercase ASCII `snake_case`. A field not
listed for its object is an `unknown_field` error. Required arrays are present
even when empty. Optional capabilities are omitted rather than encoded as
`null`.

```text
RevisionCoreV1
|- format_version: "1.0-alpha.1"
|- service_storages: ServiceStorageV2[]
|- service_resources: ServiceResourceV2[]
|- inputs: InputDeclarationV1[]
|- actions: ActionV1[]
|- snapshot?: SnapshotCapabilityV1
|- migrations: MigrationV2[]
`- cleanup?: CleanupV1

InputDeclarationV1
|- id: InputIdentity
|- required: boolean
`- protection: normal | secret

ActionV1
|- id: ActionIdentity
|- access: observe | mutate
|- parameters: ParameterV1[]
|- hook: HookDeclarationV2
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
`- hook: HookDeclarationV2

RestoreV1
|- parameters: ParameterV1[]
`- hook: HookDeclarationV2

MigrationV1
|- source_revision_digest: Sha256Digest
|- transitions: MigrationTransitionV1[]
|- requires_source: InputBindingRefV1[]
|- requires_target: InputIdentity[]
|- produces_target: InputIdentity[]
`- hook?: HookDeclarationV2

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
`- hook: HookDeclarationV2
```

The presence of a parameter `default` makes that parameter optional for an
invocation; omission makes it required. Parameters have no enum, range, regex, raw argv,
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

Fixed integral schema values use the same exact mathematical integral check.
Format and Hook protocol versions in the current baseline are FormatVersion
strings, not numbers; the supported marker is `"1.0-alpha.1"`.

**Verification: PR-TEST-0006, PR-TEST-0007.**

## Hooks and lifecycle capabilities

### PR-REQ-0189 - Identity-bearing Hook launch

Every Hook uses this closed identity-bearing representation:

```text
HookV1
|- protocol_version: FormatVersion string
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

RevisionCoreFormat does not make exact source availability an installation
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

This format defines no author-configurable `execution_requirements` vocabulary. Capture
adds `RequiredInputsSatisfied`, valid parameters, and available Hook runtime to
the global managed-execution prerequisites. Restore adds exact producer
Revision compatibility, valid staged Snapshot bindings, valid parameters, and
available Hook runtime. These capability prerequisites are additive to, not
replacements for, global Run creation, trust, exact-reference, stale-state,
policy, environment, pin, guard, Plan invalidation, and recovery invariants.

The fixed prerequisites are semantics of the Capture and Restore schema tokens.
A future author-configurable requirement vocabulary is a Revision Core format
evolution. This format does not define Snapshot authoritative bodies or
`SnapshotIntegrityFormat`.

**Verification: PR-TEST-0003, PR-TEST-0225, PR-TEST-0227, PR-TEST-0231, PR-TEST-0235, PR-TEST-0241, PR-TEST-0246, PR-TEST-0258, PR-TEST-0270.**

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

Revision Core golden vectors do not establish this runtime behavior. The
Compiler, Admission, and Executor integration tests listed below cover it
separately from canonical-byte verification.

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
The encoding is defined by this specification, RFC 8785, and Pactrun golden
vectors. `serde_jcs = "=0.2.0"` is an implementation dependency, not an
authority. A disagreement requires correcting or replacing the implementation;
the specification and expected digest MUST NOT be changed merely to fit a
library.

The digest input is exactly:

```text
ASCII("pactrun.revision-content-digest\0")
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

## Codec verification {#golden-vector-coverage-boundary}

The Rust verifier owns raw JSON, duplicate rejection, closed-schema and semantic
validation, normalization, canonicalization, framing, and digest. The Node 24
oracle owns independent canonicalization, framing, SHA-256, and valid-vector
parity. It is not independent proof of raw duplicate rejection.


### PR-REQ-0317 - Service declarations and authority schema {#pr-req-0317---core-v2-closed-schema}

The following service fields are required parts of the complete baseline.
Referenced shared declaration types retain their closed fields and validation. Unknown
fields, duplicate decoded keys and generic null values are rejected. Normalized
arrays below are required even when empty; optional capabilities are omitted.

```text
Complete RevisionCore = common declaration fields plus:
  format_version: "1.0-alpha.1"
  service_storages: ServiceStorageV2[]
  service_resources: ServiceResourceV2[]
  every HookV1 position replaced by HookDeclarationV2
  every MigrationV1 replaced by MigrationV2

ServiceStorageV2 = { id: ServiceStorageIdentity }
ServiceResourceV2 = {
  id: ServiceResourceIdentity,
  storage_id: ServiceStorageIdentity,
  locator: ServiceLocatorV2,
  kind: file | directory,
  read_exposure: hidden | readable,
  user_mutation: UserMutationV2
}
UserMutationV2 =
  { kind: unavailable }
  | { kind: direct }
  | { kind: operation, action_id: ActionIdentity }

HookDeclarationV2 = HookV1 with:
  service_access: ServiceAccessV2[]
  service_requires: ServicePrerequisiteV2[]
ServiceAccessV2 = { reference: ServiceReferenceV2, mode: read | write }
ServiceReferenceV2 = {
  view: current | source | target,
  role: active | retained,
  kind: storage | resource,
  id: ServiceStorageIdentity-or-ServiceResourceIdentity
}
ServicePrerequisiteV2 = {
  reference: ServiceReferenceV2 with kind exactly resource,
  presence: present | absent
}

MigrationV2 = MigrationV1 with:
  hook?: HookDeclarationV2
  storage_transitions: StorageTransitionV2[]
  resource_transitions: ResourceTransitionV2[]
StorageSourceV2 = { role: active | retained, storage_id: ServiceStorageIdentity }
ResourceSourceV2 = { role: active | retained, resource_id: ServiceResourceIdentity }
StorageTransitionV2 =
  { kind: create, target_storage_id: ServiceStorageIdentity }
  | { kind: reuse, source: StorageSourceV2, target_storage_id: ServiceStorageIdentity }
  | { kind: reattach, source: StorageSourceV2, target_storage_id: ServiceStorageIdentity }
ResourceTransitionV2 =
  { kind: create, target_resource_id: ServiceResourceIdentity, presence: any | present | absent }
  | { kind: reuse, source: ResourceSourceV2, target_resource_id: ServiceResourceIdentity }
  | { kind: reattach, source: ResourceSourceV2, target_resource_id: ServiceResourceIdentity }
  | { kind: transform, sources: ResourceSourceV2[], targets: ServiceResourceIdentity[] }
```

Storage and Resource identities are separate types using the semantic-identifier
grammar and length. They are Package-lineage scoped and non-reusable in meaning;
equal spellings across these two types do not equate identities. Every resource
names a declared storage. A file means a regular file, not a device, socket or
symbolic link. A directory includes its descendant service contents for authority
purposes, not for identity calculation or automatic scanning.

Live Storage contents have no Managed Input payload-size limit or Pactrun quota
in this contract; ordinary host filesystem capacity applies. Existing Input,
Snapshot capture/restore and artifact capability limits remain independent.

`ServiceLocatorV2` is 1..1024 ASCII bytes of slash-separated 1..128-byte segments
matching `[A-Za-z0-9._-]+`. Reject empty, `.` and `..` segments, trailing periods,
Windows device-name stems (the case-insensitive reserved-name rule), and any
absolute/drive/backslash/colon syntax. Leading periods and uppercase letters
are allowed and preserved. Spaces and non-ASCII locator characters are outside
this initial portable contract; native storage-root paths remain separate.
Do not lowercase or Unicode-normalize canonical data. Within one storage,
active declarations must not have ASCII-case-fold-equal locators. A declared
file cannot be a locator ancestor of another resource; a directory may be.
Nested directory/file resources deliberately have overlapping authority scopes.

`user_mutation.kind=operation` names an existing Mutate Action whose write
authority covers this current resource. It does not auto-invoke that Action.
Every Action/Capture Hook with write service authority must have Mutate access;
Migration/Restore are already Mutate. This classification does not substitute
for the runtime Clear/Open risk handshake or lock out an external service.

Ordinary Action/Capture/Restore references use `current/active`. For Restore,
current service authority is the target Instance's existing live storage, while
Input authority is still the staged Snapshot binding view. Migration references
use `source/active`, `source/retained`, or `target/active`; `current` is forbidden
there. Cleanup uses `current/active` and `current/retained` under the
[Instance retirement contract](../lifecycle/retirement.md).
Source/active authority resolves against
the exact source Core; source/retained authority resolves the explicitly named
retained association and its last declaration. A grant need not consume a
mapping source: an explicitly granted source Storage can contain a resource
being transformed into a new target Storage. Target/current references resolve
to target/current Core declarations. Exact retained availability is a runtime
predicate, not an install-time claim.

Write includes read. Duplicate references with different modes are rejected,
not silently unioned. A whole-storage grant that subsumes another grant of the
same view/role is redundant and rejected; a directory resource grant may
deliberately include descendants. Authority is Pactrun-mediated, not OS
confinement. A prerequisite never grants authority. Contradictory/duplicate
presence prerequisites for one reference are rejected. A definitive Unknown
observation satisfies neither present nor absent.

The supported Hook protocol is the string `1.0-alpha.1`. A Hook with nonempty
service_access or service_requires, or on a transform edge, must select that
supported protocol. Other syntactically valid Hook format identifiers remain
representable for non-transform Hooks with empty service arrays, but fail
runtime qualification before Run acceptance. The Core version never selects
the Hook protocol version.

A minimal normalized Core example and its empty runtime-content sibling are:

```json
{"actions":[],"format_version":"1.0-alpha.1","inputs":[],"migrations":[],"service_resources":[],"service_storages":[{"id":"state"}]}
```

```json
{"files":[]}
```

These illustrate shapes, not a Frozen vector or published digest. Adding
allocation_id, native_path or service bytes to this Core is an unknown-field
error. Repeating state is a duplicate identity; using a numeric format_version is unsupported, not a downgrade path.

**Verification: PR-TEST-0331, PR-TEST-0333, PR-TEST-0334, PR-TEST-0337, PR-TEST-0367, PR-TEST-0384, PR-TEST-0386.**

### PR-REQ-0318 - Service declaration normalization and identity {#pr-req-0318---v2-normalization-and-identity}

Apply the strict JSON, exact numeric, Unicode-scalar, finite binary64 and
RFC 8785 rules in PR-REQ-0185 through PR-REQ-0193. Normalize each set by its
defined keys and preserve ordered args. Storage/resource declarations sort by id.
Service references sort by `(view,role,kind,id)` using ASCII token bytes;
service_access and service_requires use that reference key. Reject duplicate
keys before sorting. Transform sources sort by `(role,resource_id)` and targets
by identity; both lists must be nonempty and duplicate-free.

Storage/resource transition arrays sort by their sole target, or the first
sorted transform target. Target uniqueness below makes that a total unique
key; one transform group owns all its targets. Different explicit mappings
remain distinct identities even when a Pack believes them equivalent.

Use RuntimeContentClosureIdentityV1 as the sibling component. Encode both
normalized components and hash them using the
[JCS, framing and digest rules](#pr-req-0193---jcs-framing-digest-and-conformance)
in PR-REQ-0193. Render `sha256:` plus 64 lowercase hexadecimal characters.
Lengths count bytes. The exact string format_version is bound by Core JCS;
there is no redundant integer frame version. Native paths, allocation IDs, resource observations,
service bytes, exposure history and persistence bookkeeping never enter identity.
There is one supported baseline decoder and no decode-and-reencode development upgrade.
PackageId plus the resulting digest remains RevisionIdentity; no new public ID
envelope is required. Independent parity vectors must consume checked-in inputs, not Rust-produced
expected bytes.

**Verification: PR-TEST-0331, PR-TEST-0332, PR-TEST-0333, PR-TEST-0338, PR-TEST-0339, PR-TEST-0379, PR-TEST-0383.**

### PR-REQ-0319 - Target-owned resource mapping

Each target storage and target resource has exactly one transition writer in
each inbound edge, even when the live resource is absent. Omission is not
compatibility. Each source identity may occur in only one transition of its
type; group one-to-many/many-to-one changes in a single transform. Unmapped
source-only declarations default to retention, not physical deletion. No service `discard`
transition is defined here; Input discard remains a separate operation.

A source named by reuse, reattach or transform is consumed by that mapping.
At target commit its old association is removed and its target associations
are published atomically. If its identity is also a target, the target replaces
that association. This applies independently to Storage and Resource mappings.
Do not retain a second live alias solely because a mapped identity was renamed.
Unmapped sources retain their associations and exact last contracts. Before a
failed/uncommitted edge, all original associations remain unchanged. Association
consumption never authorizes deleting service bytes or allocation custody.

One typed source identity must have one consistent active/retained role across
all of that edge's mappings, grants and prerequisites. Reject contradictory roles
even inside one transform group; storage and resource identity types are checked
independently, and target-view references are not source-role assertions.

`create` requires no existing association for that identity, including retained
ones. Storage create reserves a new directory. Resource create establishes a
new locator contract, not a live object. Its mandatory presence predicate is
checked before grant/publication: present/absent require that exact definitive
observation; any asserts no existence condition and never converts Unknown to
presence or absence. This explicitly permits naming already service-owned files
inside a reused allocation without moving them or adopting an external directory.
It asserts no content validity or coherence; the Pack owns that interpretation.
Only parents necessary for the Storage root are created by Pactrun; service
directory trees belong to service/Hook behavior. A target create predicate and
Hook prerequisite that contradict at the same observation boundary are invalid.

At Admission, a target locator equal under portable case comparison to another
published active/retained resource in the same allocation is a location claim
conflict unless that resource is an explicit source of the target's writer.
Thus create cannot disguise a rename as a new association over an already named
object; it may name previously unassociated contents. This rejects ambiguous
claims without inferring identity or compatibility from a locator. Directory
containment alone is not equality and retains the explicit subtree semantics.

`reuse` requires an active source; `reattach` requires a retained source. Both
are explicit Pack assertions of representation compatibility. Storage reuse or
reattach transfers the same allocation to the target role. Distinct active
Storage declarations may not share an allocation. Resource reuse/reattach
requires equal kind, exact case-sensitive locator and the same resolved physical
allocation. Identity spelling changes are permitted only through that explicit
source-to-target mapping. No live existence or coherence is inferred.

`transform` requires a Hook with target-publication authority under the
[Hook Protocol](../interfaces/hook-protocol.md).
It covers locator/kind/allocation/representation changes, splits and merges;
Pactrun does not implement file moves, copying, parsing, merging or deletion.
The Hook coordinates transformations of nested/overlapping service scopes.
Every source needs declared read authority and every target write authority;
source mutation additionally needs explicit write authority. Unmapped source-only
association records remain retained even if a later service observation is
Absent. A target identity already present in the
Instance must occur as a source of its own writer group: no silent overwrite
of an unrelated active/retained association. Identity changes never authorize
reuse of an old spelling for an unrelated semantic purpose.

For example, explicit config -> settings reuse over config.json leaves only
settings associated after commit. Subsequent settings -> settings reuse has no
ghost config alias to conflict with. Dropping config with no mapping instead
retains config and requires a later explicit reattach. Historical Revision/Run
identity is separate from a live association; no alias-lineage registry is added.

Intrinsic validation checks syntax, target coverage, unique writers, local
references and Hook requirements. Exact-source relational validation checks
roles and declared compatibility, with unavailable source semantics remaining
NotEvaluated under existing install policy. Admission additionally resolves
retained contracts/allocations and presence observations. It cannot infer
compatibility by reading service bytes. When a source Revision has no service
declarations, a transition must explicitly create its initial storages and
resources.

**Verification: PR-TEST-0334, PR-TEST-0335, PR-TEST-0371, PR-TEST-0372, PR-TEST-0373, PR-TEST-0374, PR-TEST-0382, PR-TEST-0383, PR-TEST-0386, PR-TEST-0388.**

### PR-REQ-0348 - Explicit shell-loader identity and launch

The complete baseline permits exactly this additional alternative at every
Hook launch position:

```text
{ kind: shell_loader,
  shell: sh | bash | powershell_7 | windows_powershell_5_1,
  command: HostExecutableName,
  script: ContentId }
```

All fields are required; unknown fields are rejected. The script MUST reference
an owned regular runtime file; its executable bit does not control launch.
Existing Hook args, I/O, protocol selection and service authorities retain their
meanings. All declaration fields bear identity. Canonicalization uses the same baseline rules and the single frame in PR-REQ-0318. The current Pactrun executable path is a host fact,
not a Core field or identity input. Unsupported shell kinds and invalid closed alternatives MUST be rejected.

POSIX supports sh and bash; Windows supports powershell_7 and
windows_powershell_5_1. Other host/shell pairs MUST fail admission, not fall back.
Host executable resolution and admission revalidation retain the exact selected
path rules. Loader execution MUST use the running Pactrun executable without
requiring an installed PATH alias or copying Pactrun into the Pack.

**Verification: PR-TEST-0486, PR-TEST-0487, PR-TEST-0492, PR-TEST-0493, PR-TEST-0510.**

## YAML projection

Pack Source YAML baseline explicitly uses the string `source_format: "1.0-alpha.1"` and otherwise
retains YAML baseline's closed schema, defaults, scalar handling and source acquisition.
Its revision projection inserts `format_version: "1.0-alpha.1"`. There is no inferred upgrade
of a development-era source or installed Revision, and no interpreter argument
override on the shell_loader alternative. Baseline evolution and formal promotion
follow [product versioning](../storage/compatibility.md).
