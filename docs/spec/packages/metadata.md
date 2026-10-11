---
title: Descriptions and Provenance
---

# Descriptions and Provenance

Authors describe capabilities and explain their use. Users name Packages and Revisions locally. Descriptions and provenance do not change immutable Revision identity.

### PR-REQ-0019 - Local reference ambiguity {#pr-req-0019---human-label-ambiguity}

Human names are local references, not Package or Revision identity. Names and ID
prefixes follow [PR-REQ-0371](./local-names.md#pr-req-0371---local-names-and-two-part-identity-resolution).
Resolution deduplicates complete Revision identities. Zero matches is unresolved,
one is resolved, and multiple matches fail with `resolution.ambiguous_reference`.
No format may choose the latest installation, prefer a name over an ID, normalize
spelling, or treat publisher attribution as ownership of a local name.

**Verification: PR-TEST-0532.**

### PR-REQ-0020 - Metadata observations

Presentation, provenance, publisher attribution, and local
management metadata MUST remain separate from the immutable Revision body.
Presentation MUST be keyed current metadata as defined by PR-REQ-0251. A
different value for the same typed target and presentation field replaces the
current value only through the explicit typed mutation contract. Pactrun MUST NOT
retain conflicting historical presentation values or use an implicit
last-write-wins write.

Provenance and publisher attribution MUST be value-keyed typed claims as
defined by PR-REQ-0252. Identical semantic tuples are one idempotent claim;
different tuples MAY coexist and MUST NOT create a new Revision, concatenate
implicitly, or cause Pactrun to select a trusted preferred claim.

Presentation targets MUST resolve to the closed typed set already established
by the installed `RevisionCoreV1`; an unknown or inapplicable target MUST be
rejected. Presentation MUST NOT create an Action, Input, Parameter, output, or
lifecycle capability that is absent from the Revision.

This metadata category is descriptive and non-operational. It MUST NOT be used
to persist a ServiceStorage declaration, a ServiceStorage-backed Managed Service
Resource identity or declaration, live-resource presence, continuity,
retention, authority, an operation prerequisite, or a storage-lifetime
obligation.

**Verification: PR-TEST-0062, PR-TEST-0063.**

### PR-REQ-0021 - Portable and local metadata

Revision export and import MUST preserve every portable non-identity
association or observation that the relevant Export Bundle Format defines as
part of the portable bundle. Presentation metadata,
source-URI claims, publisher-attribution claims, and attribution claims are
semantically portable-capable because their Domain meaning does not depend on
one Pactrun installation. These fields MUST remain non-identity metadata.

Local install timestamps, source filesystem paths, local names, local notes,
and local trust decisions are local-only and MUST be excluded by default.
Portable-capable does not require an Export Bundle Format to carry a
metadata kind and does not define serialization, carriage, import conflict, or
merge policy. Publisher claims and digests MUST NOT be presented as publisher
authentication.

**Verification: PR-TEST-0539.**

### PR-REQ-0022 - Repeat Revision import

Importing the same `PackageId` and digest MUST be identity-idempotent. Equivalent
visible metadata associations MUST NOT accumulate duplicates. Equivalence MUST
use the complete typed semantic tuple for the metadata kind, including every
typed source component; it MUST NOT use a storage row identifier, timestamp, or
serialization artifact. New distinct set-like claims MAY be attached, while
current metadata may change only through its explicit replacement semantics.

Import MUST NOT infer a local name from author descriptions or publisher claims.
Explicitly requested local names follow the atomic conflict rules in PR-REQ-0372.
Repeat installation without requested names preserves existing user names and
installation time. The [Pack distribution contract](./distribution.md) defines
the bundle representation and import application policy.

**Verification: PR-TEST-0538, PR-TEST-0539, PR-TEST-0540.**

### PR-REQ-0248 - Descriptive metadata boundary {#pr-req-0248---m1-d-metadata-boundary}

Non-identity metadata persistence MUST be limited to the closed typed
descriptive values in PR-REQ-0249 through PR-REQ-0253: current presentation,
typed provenance claims, one
local current note, and one local current trust assessment. It MUST NOT accept
generic metadata, arbitrary keys, JSON/EAV payloads, or opaque serialized
objects, and MUST NOT become a backdoor persistence mechanism for
ServiceStorage or ServiceStorage-backed Managed Service Resource operational
semantics.

Descriptive metadata MUST NOT persist storage or resource declarations or identities, live
presence, service-owned contents, locators or associations, continuity,
compatibility, retention or discard, persistent Hook authority, operation
prerequisites, Cleanup-completion or storage-finalization obligations,
AbandonManagement non-destruction obligations, or the broader service-owned
resource taxonomy. The persistence schema and repository contract MUST enforce
this boundary rather than merely hiding excluded operational data behind a
metadata name.

**Verification: PR-TEST-0059, PR-TEST-0067.**

### PR-REQ-0249 - Typed metadata scope and authoritative strings

Metadata MUST be Revision-scoped, descriptive, and non-identity-bearing.
Its closed types are current presentation metadata,
the three provenance claim types in PR-REQ-0252, one
optional local current note, and one optional local current trust assessment.
Descriptive metadata MUST NOT attach to a generic entity path or stand in for
managed state. Package/Revision names and installation time belong to the separate
local catalog defined by PR-REQ-0371/0372.

Except where a closed type says otherwise, a textual metadata value MUST be a
non-empty valid Unicode scalar-value sequence. Pactrun MUST preserve its exact
UTF-8 byte sequence and MUST NOT normalize Unicode, fold case, trim, repair, or
apply locale-sensitive comparison. Equality and authoritative ordering MUST use
the typed exact value. Exact textual ordering is unsigned lexicographic ordering
of the preserved UTF-8 bytes. The metadata model has no semantic length cap.
A transport limit MUST reject an oversized request rather than truncate or
change the value.

`SourceUri` MUST match the ASCII `URI` production in RFC 3986. It MUST contain a
scheme, MUST reject a relative reference and non-ASCII input, and MAY contain a
fragment. URI validity does not authorize case normalization, percent-encoding
normalization, base resolution, parser-result reserialization, parser semantic
equality, or any other canonicalization; its authoritative equality and
ordering remain those of the caller-provided exact URI string. A
normalized or fuzzy search facility MAY provide non-authoritative discovery but
MUST NOT change binding identity, equality, conflict, or ambiguity.

Every optional typed component MUST order `Absent` before `Present(value)`;
present values use their underlying typed comparator. Every closed union or enum
used for deterministic ordering MUST publish an explicit stable rank and MUST
NOT depend on a Rust discriminant, SQL rowid, insertion time, query-plan order,
SQLite NULL ordering, or locale collation.

**Verification: PR-TEST-0058, PR-TEST-0066.**

### PR-REQ-0250 - Metadata ordering {#pr-req-0250---reference-label-binding-lookup-and-ordering}

Current descriptive metadata MUST enumerate deterministically by exact Revision
identity, kind, and each kind's typed key. Presentation precedes provenance,
followed by local notes and local trust. Presentation uses the target and field
ranks in PR-REQ-0251; provenance uses PR-REQ-0252. Optional values order Absent
before Present and strings use their exact UTF-8 byte order.

Search normalization and display layout MUST NOT change equality, replacement,
claim identity, or the resolution of local names. Descriptions and publisher
claims do not create lookup bindings. Local name lookup follows PR-REQ-0371.

**Verification: PR-TEST-0058, PR-TEST-0062, PR-TEST-0066.**

### PR-REQ-0251 - Closed current presentation model

Current presentation metadata MUST be keyed by one exact Revision, one
`PresentationTargetV1`, and one `PresentationField`. The target union and stable
rank are, in order: Revision root, Input, Action, Action Parameter, Managed
Output, Snapshot Capture, Snapshot Capture Parameter, Snapshot Restore,
Snapshot Restore Parameter, Migration edge, and Cleanup. Nested targets MUST
use their typed parent and child semantic identities. A Migration edge MUST use
the exact source Revision selector within the target Revision context.

Revision explanations MUST use `summary`, `description`, or `help`; authors
cannot assign a Revision display name. Other declared targets retain display
names as explanations beside their immutable callable identifiers.

The presentation fields and stable rank are `display_name < summary <
description < help`. The Revision target excludes `display_name`; every other
target in the closed union supports all four fields. Values MUST be non-empty exact strings under
PR-REQ-0249; clearing a field represents absence. No Hook internal, Migration
transition row, runtime `ContentId`, JSON path, authoring-AST path, arbitrary
field, or locale is a presentation target or key.

A target MUST exist in the strict-decoded installed `RevisionCoreV1`. Unknown,
malformed, or absent targets MUST be rejected. Reapplying the current value is
idempotent. A different value or clear operation MUST use the semantic compare-and-swap (CAS) rule
in PR-REQ-0255 and MUST NOT retain an older presentation value. Deterministic
enumeration MUST order by Revision identity, target rank, target components in
their declared order, and presentation-field rank.

**Verification: PR-TEST-0058, PR-TEST-0063, PR-TEST-0066.**

### PR-REQ-0252 - Typed provenance claims

Provenance MUST contain exactly these value-keyed claim types and stable
rank: `SourceUriClaim < PublisherAttributionClaim < AttributionClaim`.

- `SourceUriClaim` contains one exact `SourceUri`.
- `PublisherAttributionClaim` contains a non-empty exact publisher name, an
  optional non-empty exact namespace, and an optional exact source URI.
- `AttributionClaim` contains non-empty exact attribution text and an optional
  exact source URI.

The complete typed tuple determines equality and idempotency. Identical tuples
MUST NOT accumulate; different tuples MAY coexist. Deterministic enumeration
MUST order by Revision identity, claim rank, and the fields above in declaration
order, with optional fields ordered by PR-REQ-0249. Provenance MUST NOT add an opaque
claim ID, generic claim kind, arbitrary key/value payload, automatic timestamp,
mutation order, preferred claim, authenticity conclusion, or audit history.

**Verification: PR-TEST-0058, PR-TEST-0062, PR-TEST-0066.**

### PR-REQ-0253 - Local names, notes, and trust {#pr-req-0253---local-aliases-notes-and-trust}

Local names follow PR-REQ-0371/0372: one optional user name per Package and one
per Revision. They are not author metadata or a second immutable identity.

Each exact Revision MAY have one non-empty local current note and one local
current trust assessment. Note absence is distinct from an empty string. Trust
is exactly `Trusted | Distrusted`, with absence meaning `NoDecision`.
`Distrusted` is descriptive local assessment only and MUST NOT independently
deny installation or execution. Note and trust replacement and clearing use
PR-REQ-0255. Pactrun MUST NOT create name, note, or trust history, opaque IDs,
purpose-scoped trust, policy enforcement, or a metadata version token.

Installation time is local-only and is recorded under PR-REQ-0372, not portable
descriptive metadata. A source filesystem path is not a Revision identity.

**Verification: PR-TEST-0058, PR-TEST-0064, PR-TEST-0066, PR-TEST-0530, PR-TEST-0535.**
