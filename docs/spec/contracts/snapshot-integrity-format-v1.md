---
title: Snapshot Integrity Format V1
---

# Snapshot Integrity Format V1

**Status: Frozen normative Package contract specification.**

<!-- spec-navigation:start -->
## Reading map (informative)

Preserve the Frozen V1 identity and verification rules required by supported existing Snapshots. V2 does not make this compatibility contract disappear.

Start with the [specification map](../index.md)
and [shared vocabulary](../glossary.md) if a term is unfamiliar.
Check [implementation status and remaining decisions](../../development/next-milestone.md)
before treating an approved contract as available runtime behavior.
The original status, rules, exceptions, and verification declarations below retain their meaning.
<!-- spec-navigation:end -->

:::note Informative ServiceStorage scope clarification

The V1 `service_content` closure represents immutable recovery content selected
and submitted through Snapshot Capture. It is not an automatic inventory or
mirror of future `ServiceStorage` or Managed Service Resources. This
clarification changes no V1 schema, normalization, canonical bytes, framing,
digest, or golden vector.

:::

This page defines the complete, Frozen `SnapshotIntegrityFormatV1` contract.
The Candidate used these exact semantic schema, canonical bytes, framing, and
digests as conformance data without publishing them as stable Pactrun Snapshot
integrity identities. Freezing changed status and verification metadata only.

## Format lifecycle

### PR-REQ-0195 - Candidate and Frozen integrity contract

Candidate and Frozen use the same `format_version = 1` semantic schema,
normalization, canonical bytes, framing, and digest profile. Freezing is a
status change only. After the format is Frozen, any integrity-affecting change
requires a new Snapshot integrity format.

`SnapshotIntegrityFormatV1` is independent of Revision Core format, Hook
Protocol, Snapshot bundle, and persistence-schema versions. The manifest
`format_version` and framing `U32BE(1)` are markers of this one Snapshot
integrity contract and MUST NOT be independently mixed.

**Verification: PR-TEST-0013, PR-TEST-0018, PR-TEST-0019.**

## Authoritative object and digest input

### PR-REQ-0196 - Digest-bearing Snapshot structure

A committed Snapshot authoritative body logically contains:

```text
SnapshotAuthoritativeBody
|- integrity_format: SnapshotIntegrityFormatV1
|- integrity_digest: SnapshotIntegrityDigest
|- manifest: SnapshotIntegrityManifestV1
`- content_blobs: digest-addressed opaque payload closure
```

This structure is a semantic ownership boundary, not a persistence or bundle
encoding. The integrity digest is calculated over the manifest framing defined
below. It does not hash its own spelling. The selected integrity format tells a
verifier how to interpret and frame the manifest; the in-band manifest and
frame markers must agree with that selected format.

`content_blobs` contains the exact payload bytes referenced by bound managed
bindings and service-content descriptors. It is not another JCS component:
each payload is authenticated by the SHA-256 digest embedded in the manifest,
and only referenced payloads belong to the authoritative closure. Storage
layout, deduplication, archive encoding, and compression remain outside this
format.

`SnapshotId` identifies the durable object. `SnapshotIntegrityDigest` protects
the manifest and the content digests it names. Equal integrity digests do not
merge different Snapshot IDs, and a repeated Snapshot ID with a different
integrity digest is an identity collision.

**Verification: PR-TEST-0013, PR-TEST-0016, PR-TEST-0018.**

## Closed semantic schema

### PR-REQ-0197 - SnapshotIntegrityManifestV1 schema

The normalized manifest MUST use this closed schema. Object keys and enum
tokens are lowercase ASCII `snake_case`. Required arrays are present even when
empty. A field not listed for its object is `unknown_field`.

```text
SnapshotIntegrityManifestV1
|- format_version: 1
|- snapshot_id: OpaqueResourceIdV1
|- producer: RevisionIdentityV1
|- origin_instance_id: OpaqueResourceIdV1
|- captured_at: TimestampV1
|- managed_bindings: ManagedBindingStateV1[]
`- service_content: ServiceContentV1[]

RevisionIdentityV1
|- package_id: OpaqueResourceIdV1
`- revision_content_digest: Sha256Digest

TimestampV1
|- unix_seconds: SafeInteger
`- nanoseconds: integer 0..=999999999

ManagedBindingStateV1
|- {
|    input_id: InputIdentity,
|    role: active | retained,
|    state: absent,
|    protection: normal | secret
|  }
`- {
     input_id: InputIdentity,
     role: active | retained,
     state: bound,
     protection: normal | secret,
     blob_digest: Sha256Digest
   }

ServiceContentV1
|- role: ServiceContentRole
|- path: SnapshotContentPath
`- blob_digest: Sha256Digest
```

The manifest contains no origin Instance name, creator Run, historical binding
log, trust decision, label, note, compression setting, archive member offset,
source filesystem path, timestamp other than the authoritative capture time,
or extension map.

**Verification: PR-TEST-0013.**

### PR-REQ-0198 - Resource identity and timestamp spelling

`OpaqueResourceIdV1` is the Snapshot-format canonical spelling for
`SnapshotId`, `PackageId`, and `InstanceId`. It is exactly 32 lowercase ASCII
hexadecimal characters representing 16 opaque octets. This does not prescribe
an internal database representation, UUID version, or user-visible name.
Uppercase, prefixes, braces, and hyphenated forms are rejected with
`invalid_resource_id` rather than normalized.

`TimestampV1` represents one UTC instant as:

```text
unix_seconds + nanoseconds / 1,000,000,000
```

`unix_seconds` is an exact mathematical integer in
`-(2^53-1)..=+(2^53-1)`. `nanoseconds` is an exact mathematical integer in
`0..=999999999`. For instants before the Unix epoch, `unix_seconds` is the
floor and `nanoseconds` remains nonnegative. This gives each representable
instant one normalized spelling and does not encode a local timezone, leap
second spelling, or adjustable precision.

Equivalent JSON integer tokens such as `1`, `1.0`, and `1e0` normalize to the
same timestamp component. Implementations MUST validate their exact decimal
mathematical value before conversion through binary64. Invalid or out-of-range
timestamp numbers are `invalid_number`.

**Verification: PR-TEST-0014.**

## Managed binding state

### PR-REQ-0199 - Complete binding representation and validation boundary

The manifest has at most one `ManagedBindingStateV1` for each `input_id`.
`active` and `retained` are roles over the one managed binding registry,
relative to the exact producer Revision; they do not establish separate
stores. A bound entry names the SHA-256 digest of the exact opaque payload
bytes, including the digest of a zero-length payload. Protection is always
explicit and sticky for retained Secrets.

An absent entry records a declared active Input for which no binding exists.
A retained entry therefore MUST be `bound`; `retained + absent` is
`invalid_binding_state`. Absence is a state descriptor, not an invented empty
payload.

Intrinsic format validation checks the closed union, identifier and digest
spelling, unique `input_id`, and the prohibition on absent retained entries.
When exact producer Revision semantics are supplied, relational validation
additionally checks all of the following:

- every producer Input declaration has exactly one `active` descriptor;
- an `active` descriptor names a producer Input declaration;
- a `retained` descriptor does not name a producer Input declaration; and
- an active descriptor's protection equals the producer declaration.

Without exact producer semantics, these predicates are `not_evaluated`, not
valid or invalid. Producer Revision availability is not required to verify the
manifest digest, import a Snapshot, or preserve a Snapshot that outlives its
producer installation. Exact-compatible Restore supplies the producer
semantics before the staged binding state becomes executable.

Format vectors demonstrate the representation and explicitly supplied-context
relational predicates. They do not prove that a Capture recorded every binding
from the admitted registry view. That runtime completeness and view-consistency
promise remains owned by existing Capture integration requirements.

Secret blob digests are integrity-bearing and sensitive. Ordinary inspection,
diagnostics, and Run records MUST NOT disclose a Secret value or its
value-derived digest. The conformance fixtures contain only public test bytes
and do not relax that presentation boundary.

**Verification: PR-TEST-0015.**

## Service Snapshot content

### PR-REQ-0200 - Logical service-content closure

`service_content` is a semantic set of opaque content blobs produced for
service recovery. `role` uses the semantic identifier profile defined below;
`path` is a logical portable Snapshot path. Each `(role, path)` pair is unique.
The same blob MAY be referenced by more than one logical pair.

A descriptor binds logical placement to exact bytes through `blob_digest`.
It does not hash source archive layout, compression, filesystem timestamps,
ownership, inode, local materialization root, or storage location. V1 does not
interpret the blob payload or infer a content type from its path.

A complete Snapshot verifier MUST verify that every bound managed payload and
service-content payload is available and hashes to its declared digest.
Manifest-only calculation may compute the integrity digest without fetching
payloads, but MUST NOT report full Snapshot content verification.

**Verification: PR-TEST-0016, PR-TEST-0258.**

## Semantic normalization and input validity

### PR-REQ-0201 - V1 lexical and collection profile

`InputIdentity` and `ServiceContentRole` use the Revision Core V1 semantic
identifier grammar:

```regex
^[a-z][a-z0-9]*(?:[._-][a-z0-9]+)*$
```

They are 1 through 128 UTF-8 bytes. `SnapshotContentPath` uses the Revision
Core V1 runtime-path profile: `/`-separated identifier segments, at most 1024
UTF-8 bytes, no empty, `.` or `..` segment, backslash, absolute path, drive
prefix, or ASCII-case-insensitive Windows reserved device name. No Unicode
normalization is performed.

Managed bindings sort by `input_id`. Service content sorts by `(role, path)`.
Duplicate semantic keys are `duplicate_semantic_key`. Object member order and
input set order do not affect normalized bytes.

Every raw JSON string MUST represent a valid Unicode scalar-value sequence.
Malformed UTF-8, lone surrogates, and invalid surrogate sequences are
`invalid_unicode_scalar`; implementations MUST NOT repair them. All V1 fields
then apply their stricter ASCII lexical profiles.

Raw JSON property names MUST be duplicate-aware. Duplicate properties are
`duplicate_property`. Negative vectors SHOULD isolate one violation. Unless a
future requirement explicitly defines precedence, inputs with multiple
independent violations establish no stable first-error ordering.

**Verification: PR-TEST-0017.**

## Canonical bytes and digest

### PR-REQ-0202 - JCS, framing, and SHA-256

After semantic normalization, the manifest uses RFC 8785 JCS bytes. The
normative authority is this specification, RFC 8785, and Pactrun golden
vectors. A serialization library is an implementation dependency and MUST be
corrected or replaced if it disagrees with the Frozen specification or
vectors.

The digest input is exactly:

```text
ASCII("pactrun.snapshot-integrity-digest\0")
U32BE(1)
ASCII("snapshot-integrity-manifest\0")
U64BE(len(manifest_jcs))
manifest_jcs
```

Lengths count octets. The digest is SHA-256 rendered as `sha256:` followed by
64 lowercase hexadecimal characters.

**Verification: PR-TEST-0018, PR-TEST-0019.**

### PR-REQ-0203 - Verifier and oracle responsibilities

The Rust verifier owns raw JSON validation, duplicate rejection, closed-schema
validation, intrinsic validation, explicitly supplied producer-context
relational validation, payload-digest verification, semantic normalization,
JCS, framing, SHA-256, and traceability.

The independent Node 24 oracle consumes checked-in normalized valid vectors and
public blob fixtures. It independently calculates payload SHA-256 values,
canonical manifest bytes, framing, and the Snapshot integrity digest. It MUST
NOT use Rust-produced expected bytes or digests as calculation input. Node's
built-in `JSON.parse()` is not independent proof of raw duplicate-property
rejection and the Node oracle does not claim that responsibility.

Golden vectors cover format representation, normalization, payload digest
closure, intrinsic and explicitly supplied-context relational validation,
canonical bytes, framing, and digest. They do not cover Capture binding-view
consistency, managed-registry pinning, SnapshotCandidate commit, persistence,
export/import authorization, Restore Admission, Secret redaction in product
output, or Hook execution.

**Verification: PR-TEST-0017, PR-TEST-0019.**
