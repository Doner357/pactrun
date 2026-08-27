---
title: Identity and State
---

# Identity and State

**Status: Normative architecture.**

This page owns internal identity, canonicalization, metadata, Instance state,
and managed binding invariants. User-visible behavior is summarized in the
[product behavior](../product-behavior/packages-revisions-and-instances.md).

## Revision identity

### PR-REQ-0011 - Stable Package lineage

`PackageId` MUST be opaque, stable, and independent of content. It MUST survive
Revision changes, presentation or publisher changes, export, and import. A fork
that becomes a new lineage MUST be explicitly re-identified.

**Verification: Pending automated coverage.**

### PR-REQ-0012 - Operational content digest

`RevisionContentDigest` MUST identify Pactrun-managed operational semantics and
immutable owned runtime content. It MUST exclude `PackageId`, display content,
publisher claims, labels, source location, install time, local aliases, notes,
and local trust decisions.

**Verification: Pending automated coverage.**

### PR-REQ-0013 - Exact Revision identity

An exact `RevisionIdentity` MUST be the pair of `PackageId` and
`RevisionContentDigest`. Equal content digests under different Package IDs MUST
remain distinct Revision identities.

**Verification: Pending automated coverage.**

### PR-REQ-0014 - RevisionCore projection

`RevisionCore` MUST be a versioned, identity-bearing semantic projection of a
validated `NormalizedPackDefinition`. It MUST NOT be defined by serializing a
Rust type, database row, authoring AST, or the entire normalized authoring
model.

The projection MUST include every field that can affect invocation semantics,
parameter behavior, compiler or executor behavior, operation access, Hook
Session authority, Inputs, Snapshot, Migration, Cleanup, failures, recovery, or
service-facing runtime behavior. Presentation, attribution, provenance, and
local-management data MUST remain outside it.

**Verification: Pending automated coverage.**

### PR-REQ-0015 - Runtime content closure identity

Runtime content identity MUST encode logical runtime location or role, object
kind, immutable content identity, and runtime-semantic metadata. Hashing an
unordered set of blob digests is insufficient. Source-only paths, timestamps,
inodes, and ownership metadata MUST NOT affect identity unless Pactrun
explicitly defines them as runtime semantics.

**Verification: Pending automated coverage.**

### PR-REQ-0016 - Semantic normalization

Pactrun MUST apply semantic defaults, validate, and normalize equivalent forms
before projecting and encoding `RevisionCore`. Unordered domain collections
MUST be sorted by a stable semantic key; ordered collections MUST preserve their
order; duplicate semantic keys MUST be rejected.

**Verification: Pending automated coverage.**

### PR-REQ-0017 - RevisionCoreFormatV1 hash contract

`RevisionCoreFormatV1` MUST use a closed semantic schema, explicit domain
separation and framing, RFC 8785 JSON Canonicalization Scheme bytes, and a fixed
SHA-256 profile represented by a self-describing digest such as
`sha256:<value>`. JCS MUST NOT replace Pactrun semantic normalization.

The exact Frozen contract is defined by
[Revision Core Format V1](../package-contracts/revision-core-format-v1.md).

**Verification: Pending automated coverage.**

### PR-REQ-0018 - Stable published identity

Internal schema, type, serializer, library, and CLI changes MUST NOT alter the
identity of an already published RevisionCore format. Unknown or unsupported
format versions MUST be rejected instead of interpreted on a best-effort basis.

**Verification: Pending automated coverage.**

## References and non-identity metadata

### PR-REQ-0019 - Human label ambiguity

Human labels and display versions MUST be associations rather than Revision
identity. Multiple labels MAY refer to one Revision, and one label MAY refer to
multiple exact Revisions. An ambiguous reference MUST fail resolution and MUST
NOT implicitly choose the latest or overwrite an older association.

**Verification: Pending automated coverage.**

### PR-REQ-0020 - Metadata observations

Reference labels, presentation, provenance, publisher attribution, and local
management metadata MUST remain separate from the immutable Revision body.
Conflicting observations MAY coexist and MUST NOT create a new Revision,
concatenate implicitly, or use last-write-wins semantics.

Presentation observations MUST refer to semantic keys already present in
`RevisionCore`; an observation for an unknown semantic key MUST be rejected.
The initial product scope MUST NOT silently select one observation as a trusted
preferred or canonical description.

**Verification: Pending automated coverage.**

### PR-REQ-0021 - Portable and local metadata

Revision export and import MUST preserve every portable non-identity
association or observation that the relevant Export Bundle Format defines as
part of the portable bundle. This MAY include human or reference labels,
portable presentation observations, and portable provenance or publisher
attribution claims. These fields MUST remain non-identity metadata.

Local install timestamps, source filesystem paths, local aliases, local notes,
and local trust decisions MUST be excluded by default. Publisher claims and
digests MUST NOT be presented as publisher authentication.

**Verification: Pending automated coverage.**

### PR-REQ-0022 - Repeat Revision import

Importing the same `PackageId` and digest MUST be identity-idempotent. Equivalent
visible metadata associations MUST NOT accumulate duplicates; new distinct
claims MAY be attached. Revision import MUST NOT be rejected solely because a
human-readable label becomes ambiguous. If the Revision or bundle is otherwise
valid, import MUST succeed, preserve both label associations, and MAY emit a
warning. Later human reference resolution MUST report ambiguity and MUST NOT use
last-write-wins or implicit latest selection.

**Verification: Pending automated coverage.**

## Instance identity and state publication

### PR-REQ-0023 - Stable Instance identity

`InstanceId` MUST be Pactrun-generated, opaque, globally strong, stable, and
non-reusable. Migration, Input mutation, and Snapshot Restore MUST NOT change
it. Recreating a deleted Instance name MUST produce a new `InstanceId`.

**Verification: Pending automated coverage.**

### PR-REQ-0024 - Instance names and provenance

`InstanceName` MUST be a human reference that is unique among live Instances in
one management environment. Names MAY be reused after deletion, but durable
provenance MUST use `InstanceId` as its authoritative historical referent.

**Verification: Pending automated coverage.**

### PR-REQ-0025 - Opaque InstanceStateVersion

Every Instance MUST have one opaque, durable, non-revivable
`InstanceStateVersion` representing a Pactrun-owned authoritative state
publication event. It MUST NOT be an external-service hash. A rollback or
recovery that recreates earlier field values MUST publish a new token to prevent
ABA behavior.

**Verification: Pending automated coverage.**

### PR-REQ-0026 - State-version publication

Any committed Instance-state change that can affect future compilation,
admission, execution-visible managed context, lifecycle, or recovery MUST
atomically publish one new `InstanceStateVersion`. This includes active Revision
changes, managed binding mutations, Restore commits, Migration edge commits,
ManualRecoveryRequired transitions, and `ResolveManualRecovery`.

**Verification: Pending automated coverage.**

### PR-REQ-0027 - Non-versioned observations

Hook-only external side effects, Snapshot object creation, Run and Artifact
creation, and presentation-only metadata MUST NOT independently change an
Instance's state version.

**Verification: Pending automated coverage.**

## Managed Input state

### PR-REQ-0028 - Stable Input identity and opaque bytes

Within one Package lineage, the same `InputIdentity` MUST always represent the
same long-lived semantic Input and MUST NOT be reused for another meaning. Input
payloads MUST be preserved byte-for-byte, including empty payloads; Pactrun MUST
NOT infer format or semantics from content or filenames.

**Verification: Pending automated coverage.**

### PR-REQ-0029 - One managed binding registry

An Instance MUST have at most one binding for each `InputIdentity`. `Active` and
`retained` MUST be roles derived relative to the active Revision, not separate
stores or historical value logs. A binding declared by the active Revision is
active; an existing undeclared binding is retained.

**Verification: Pending automated coverage.**

### PR-REQ-0030 - Retained binding lifetime

A retained binding MUST persist until it becomes active again, is explicitly
discarded by Migration, is explicitly deleted by the operator, or the Instance
is removed or abandoned. Historical Revision identity MAY be provenance but
MUST NOT be part of binding identity.

**Verification: Pending automated coverage.**

### PR-REQ-0031 - RequiredInputsSatisfied

`RequiredInputsSatisfied` MUST be derived from required declarations in the
active Revision and current managed bindings. Empty bindings count as present.
Missing required bindings MUST NOT invalidate the Instance or create a separate
lifecycle state.

**Verification: Pending automated coverage.**

### PR-REQ-0032 - Initial binding acquisition and Instance publication

Instance creation MAY commit an incomplete Instance. When a request explicitly
supplies an initial binding, it MUST identify the Input and acquisition source
explicitly and deterministically. The Instance and every explicitly supplied
binding MUST be one atomic management commit. Any acquisition or validation
failure for such a binding MUST fail the whole create request; Pactrun MUST NOT
silently omit it and create an Instance more incomplete than requested.

**Verification: Pending automated coverage.**

### PR-REQ-0033 - Binding mutation invariants

An active required binding MAY initially be absent, but once bound under that
active Revision it MUST NOT be ordinarily unset or deleted. Active optional
bindings MAY be set, replaced, or removed. Replacements MUST be atomic. Retained
bindings MAY be inspected, explicitly exported, or deleted, but MUST NOT be
directly set or replaced by the operator.

**Verification: Pending automated coverage.**

### PR-REQ-0034 - Secret protection

Secret protection MUST be sticky. Normal data MAY be promoted to Secret
automatically, but Secret data MUST NOT be implicitly downgraded. Declassification
requires an explicit Migration transition and explicit operator authorization.
Retained Secrets MUST retain protection, and ordinary inspection, diagnostics,
Run records, and user-visible metadata MUST NOT disclose their values or
value-derived digests.

**Verification: Pending automated coverage.**
