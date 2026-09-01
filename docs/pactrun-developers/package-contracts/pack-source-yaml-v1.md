---
title: Pack Source YAML V1
---

# Pack Source YAML V1

**Status: Candidate normative Package authoring contract; versioned,
non-Frozen, and not a public compatibility promise.**

`PackSourceYamlV1` is the closed minimal M2 authoring frontend. It projects
authoring source into the existing Frozen `RevisionCoreV1` semantic model; it
does not modify that format, its canonical bytes, digest, or vectors. The
language-neutral Recipe Authoring Contract remains separate M8 work.

### PR-REQ-0258 - PackSourceYamlV1 schema, numbers, and Package lineage

The source root MUST contain one manifest named exactly `pactrun.yaml`. The
manifest MUST be one UTF-8 YAML 1.2 JSON-compatible document with this closed
top-level model. Here, JSON-compatible describes only the accepted structural
and syntactic subset; it does not enable the YAML or JSON implicit scalar
resolver. The schema-directed decoding rules below are authoritative.

```text
source_format: 1
package_id: <32 lowercase hexadecimal characters>
revision:
  inputs
  actions
  snapshot?
  migrations
  cleanup?
runtime_content:
  files:
    - id
      source
      path
      executable
portable_metadata?:
  reference_labels
  presentation
  provenance
```

`package_id` MUST be present and parse as the exact `PackageId` declared for
this Package lineage. Installation MUST NOT derive, guess, or generate it.
Generating a new lineage identifier is a distinct operation that uses the
operating-system cryptographic random source and prints the canonical 128-bit
identifier without changing source or repository state.

`revision` MAY express every capability in the closed `RevisionCoreV1` model:
Inputs, Actions, Snapshot Capture and Restore, inbound Migration edges, and
Cleanup. Their semantic identifiers and values project to the corresponding
Frozen typed fields. The subtree uses the Frozen semantic property and variant
spellings, but MUST NOT contain `format_version`; projection inserts exact
Revision Core format version `1`. The `runtime_content.files` source record
replaces the Frozen derived `blob_digest` with `source`, and omits `kind`
because M2 supports only `regular_file`; `id`, logical `path`, and `executable`
retain their Frozen meanings. A source record MUST NOT supply `blob_digest` or
another object kind. `inputs`, `actions`, `migrations`, and
`runtime_content.files` default to empty collections. An Input's `required`
and `protection` fields default to `false` and `normal`; a runtime file's
`executable` field defaults to `false`. Access, Hook launch, parameter types,
defaults, transitions, requirements, and every other operation-affecting field
MUST be explicit unless this contract states a default. Unknown fields,
duplicate decoded keys, anchors, aliases, merge keys, multiple documents, and
unsupported `source_format` values MUST be rejected. Pactrun MUST also reject
every explicit YAML tag, including `!!str`, `!!int`, `!!bool`, `!!null`, other
standard tags, custom tags, tag handles, tag directives, and any explicitly
tagged node.

The authoring decoder MUST preserve each decoded scalar, its raw UTF-8 source
spelling, and its scalar style until the closed schema identifies the target
Domain type. It MUST NOT run a YAML implicit scalar resolver first.

- A string-like target accepts a plain, single-quoted, or double-quoted scalar.
  YAML quoting and escapes are decoded, and the resulting Unicode scalar
  sequence is the exact value. Pactrun MUST NOT normalize Unicode, fold case,
  trim, or reinterpret a plain `true`, `null`, or JSON-number-looking token in
  such a position.
- `package_id` is a schema-targeted `PackageId`, not a YAML number. An exact
  32-character lowercase hexadecimal value remains a `PackageId` even when all
  of its characters are decimal digits.
- A string parameter default is decoded as an exact string after the parameter
  type is known. Numeric-looking source text in that position MUST NOT enter
  numeric projection.
- A Boolean target accepts only an unquoted lowercase `true` or `false`.
  Quoted `"true"` and `"false"` are not Boolean values.
- Enum and union-discriminator targets accept only their exact closed textual
  tokens and MUST NOT obtain variant meaning from a YAML resolver.
- Generic `null`, `~`, and an empty scalar do not mean absence. Optionality is
  expressed only by omitting the property unless a particular closed schema
  position explicitly defines another rule; `PackSourceYamlV1` defines no
  generic null-as-absence rule.

Every YAML scalar projected to a numeric Domain field MUST have an unquoted raw
UTF-8 token matching exactly:

```regex
-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?
```

The authoring decoder MUST retain that raw token until the target semantic type
is known. It MUST NOT first convert through a YAML library `i64`, `u64`, `f64`,
or generic numeric value. A quoted scalar is not numeric. Leading `+`, a
nonzero leading zero, `.5`, `1.`, hexadecimal, octal, binary, numeric
separators, sexagesimal, NaN, Infinity, and any other YAML-only numeric spelling
MUST be rejected. The all-explicit-tag rejection above applies equally to
numeric positions and cannot be used to enlarge this grammar.

Projection then applies the existing Frozen typed rules directly. Integer
defaults use the exact decimal mathematical value, MUST be integral, and MUST
be within `-(2^53-1)` through `+(2^53-1)`; `1`, `1.0`, and `1e0` therefore
normalize to the same integer. Float defaults convert the raw decimal once to
finite IEEE-754 binary64 using round-to-nearest, ties-to-even; overflow is
invalid, underflow to zero is allowed, and negative zero normalizes to positive
zero. Fixed integral fields, including Hook protocol versions, use exact
mathematical-integral validation without a binary64 intermediate.
`source_format` is narrower: only the raw token `1` is valid, not `1.0` or
`1e0`.

Portable metadata MUST use only the source-facing closed shapes below. The
`portable_metadata` map and each of its three sequence properties are optional;
omission means an empty collection. A present property MUST contain a sequence.
Every map is closed, every unmarked property is required, and `?` marks the only
optional properties.

```text
reference_labels:
  - label: ReferenceLabel
    source: ReferenceLabelSourceYamlV1

ReferenceLabelSourceYamlV1 =
  | { kind: unattributed }
  | { kind: source_uri,
      source_uri: SourceUri }
  | { kind: publisher,
      publisher_name: NonEmptyExactString,
      publisher_namespace?: NonEmptyExactString }
  | { kind: publisher_source_uri,
      publisher_name: NonEmptyExactString,
      publisher_namespace?: NonEmptyExactString,
      source_uri: SourceUri }
```

`source` MUST be present. In particular, `{ kind: unattributed }` is the only
unattributed spelling; omission MUST NOT synthesize attribution semantics.

```text
presentation:
  - target: PresentationTargetYamlV1
    field: display_name | summary | description | help
    value: NonEmptyExactString

PresentationTargetYamlV1 =
  | { kind: revision }
  | { kind: input, input_id: InputIdentity }
  | { kind: action, action_id: ActionIdentity }
  | { kind: action_parameter,
      action_id: ActionIdentity,
      parameter_id: ParameterIdentity }
  | { kind: managed_output,
      action_id: ActionIdentity,
      output_id: ManagedOutputIdentity }
  | { kind: snapshot_capture }
  | { kind: snapshot_capture_parameter,
      parameter_id: ParameterIdentity }
  | { kind: snapshot_restore }
  | { kind: snapshot_restore_parameter,
      parameter_id: ParameterIdentity }
  | { kind: migration,
      source_revision_digest: RevisionContentDigest }
  | { kind: cleanup }
```

All four presentation fields are valid for every target variant, as required
by PR-REQ-0251. A target MUST name a typed member that exists in the normalized
Revision. For a Migration target, `source_revision_digest` is the Frozen source
selector in the Package lineage being installed; the Package component is
implicit from the installation context.

```text
provenance:
  - { kind: source_uri,
      source_uri: SourceUri }
  - { kind: publisher_attribution,
      publisher_name: NonEmptyExactString,
      publisher_namespace?: NonEmptyExactString,
      source_uri?: SourceUri }
  - { kind: attribution,
      attribution_text: NonEmptyExactString,
      source_uri?: SourceUri }
```

Every portable metadata item targets the Revision produced by this installation.
The source MUST NOT spell a metadata target `package_id`, target Revision digest,
or complete `RevisionIdentity`. After intrinsic target validation, projection
MUST derive the installed `RevisionIdentity`, attach it to each typed M1-D
metadata operation, and order the operations with the existing M1-D typed
comparators. Portable metadata remains outside Revision identity.

Source normalization MUST reject duplicate semantic keys rather than silently
coalesce them. A reference-label semantic key is `(label, source tuple)` for
the implicit installed Revision; the same label with a different source tuple
is valid. A presentation semantic key is `(target, field)`; repeating it is
invalid whether its value is equal or different. A provenance key is its
complete typed claim tuple; an identical claim is invalid while distinct claims
may coexist. Local alias, note, and trust values MUST NOT occur in source.
YAML syntax and bytes are not Revision identity.

**Verification: Pending automated coverage.**

### PR-REQ-0259 - SourceRelativePathV1 and safe source acquisition

`SourceRelativePathV1` MUST contain 1 through 1024 UTF-8 bytes and use `/` as
its only separator. Every segment MUST be non-empty, MUST be neither `.` nor
`..`, MUST NOT contain `\`, `:`, `<`, `>`, `"`, `|`, `?`, `*`, U+0000 through
U+001F, or U+007F through U+009F, and MUST NOT end in U+002E dot or U+0020
space. Absolute, drive-relative, UNC, Win32 device-namespace, and NT object-
manager spellings are invalid. The complete colon prohibition makes NTFS
alternate-data streams and stream-type syntax unrepresentable.

For each segment, Pactrun MUST compare the ASCII-case-insensitive stem before
the first dot and reject `CON`, `PRN`, `AUX`, `NUL`, `CLOCK$`, `CONIN$`,
`CONOUT$`, `COM1` through `COM9`, `LPT1` through `LPT9`, and the corresponding
`COM` or `LPT` forms using Unicode superscript `¹`, `²`, or `³`. Pactrun MUST
NOT normalize Unicode, fold case generally, or apply Win32 trailing-character
normalization.

The request-selected source root is the acquisition trust anchor. Pactrun MUST
open it once as a non-link directory object and resolve every source component
relative to that opened object. The source root itself MAY be the mount trust
anchor selected by the caller. Every platform MUST reject traversal outside
that root, link following, mount crossing below that root, a non-directory
intermediate object, and a non-regular final object. Hashing and staging MUST
use the same opened final object and MUST NOT re-resolve its path.

On Windows, Pactrun MUST use handle-relative traversal without following links.
A symlink, junction, mount point, or other reparse object at any component is
invalid. After opening, Pactrun MUST obtain each authoritative long entry name
and compare it with the requested exact Unicode spelling. Resolution through a
case-insensitive match, 8.3 short name, or another pathname alias is invalid.

On Linux, the source root MUST be a non-symlink directory on a supported local
ext4, XFS, Btrfs, or ZFS filesystem. Every component MUST be opened relative to
the current directory file descriptor with semantics equivalent to
`RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS |
RESOLVE_NO_XDEV`. Every intermediate object MUST be a directory on the same
mount. Symlinks, magic links, bind mounts, and every other mount crossing are
invalid. The requested UTF-8 bytes MUST match the exact directory-entry name;
case-insensitive, Unicode-normalizing, or other non-exact lookup is invalid.
Pactrun MUST confirm the final object is a regular file from the opened file
descriptor. If the kernel or filesystem cannot provide all these semantics,
installation MUST fail before content staging rather than use weaker pathname
traversal.

The fixed manifest is acquired by the same platform-specific source-root-
relative, exact-name, no-follow, no-mount-crossing, regular-file, and same-open-
object rules. Only exact `pactrun.yaml` is valid; `pactrun.yml`, case variants,
short-name aliases, normalization aliases, and other pathname aliases are not
alternatives.

Each source locator MUST be acquired and staged once. Multiple logical runtime
files MAY reference the same staged bytes, but remain distinct descriptors
under the Frozen runtime-content identity rules. Pactrun does not promise that
a mutable source root reproduces the same Revision; identity is determined by
the normalized core and the bytes actually acquired into the runtime-content
closure.

**Verification: Pending automated coverage.**

### PR-REQ-0260 - RevisionCandidate projection and metadata boundary

M2 installation MUST use this typed pipeline:

```text
PackSourceYamlV1
-> strict parse
-> NormalizedPackDefinition
-> safe local-content staging
-> intrinsic validation
-> RevisionCoreV1 and RuntimeContentClosureIdentityV1 projection
-> attach the derived RevisionIdentity to the typed portable metadata plan
-> combine a separately supplied crate-private local metadata batch
-> durable runtime-blob publication
-> one SQLite installation publication
```

The `NormalizedPackDefinition` MUST materialize semantic defaults and stable
ordering before projection. The authoring AST, source bytes, source paths,
filesystem timestamps, storage rows, and runtime-content acquisition handles
MUST NOT enter Revision identity. Projection MUST produce the complete Frozen
semantic model and its sibling runtime-content closure rather than treating raw
Core JSON as authoring input.

The source-projected metadata plan contains reference-label, current
presentation, and provenance metadata only. It MUST validate presentation
membership against the normalized Revision before projection; after
`RevisionCoreV1` and runtime-content projection derive the exact
`RevisionIdentity`, that identity becomes the implicit target of every source-
projected metadata operation. The resulting operations retain the M1-D
idempotency, current-value CAS, and conflict rollback semantics in PR-REQ-0261.

A crate-private installation orchestrator MAY additionally receive explicit
local metadata through the existing M1-D typed batch semantics. That batch is
separate from `NormalizedPackDefinition`; it is not a source field or an M2
human-CLI option. This contract MUST NOT create a generic metadata map, a
`LocalInstall` observation, or a ServiceStorage representation.

Intrinsic candidate validation MUST remain distinct from the repository-
context Migration relation described by PR-REQ-0262. An exact source Revision
need not already be installed for the target candidate to be intrinsically
valid or installable.

**Verification: Pending automated coverage.**

## Candidate evolution and exclusions

Unsupported source versions fail explicitly. Because this contract is a
Candidate, future review may replace it before a public compatibility promise;
implementations MUST nevertheless follow the exact current version rather than
guessing unknown syntax. M2 implements authoring and installation only. Recipe
programs, `InstallContext`, network or generated authoring, stable public APIs,
and runtime execution of Action, Snapshot, Migration, or Cleanup capabilities
remain outside this slice.
