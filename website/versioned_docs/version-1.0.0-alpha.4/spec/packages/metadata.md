---
title: Labels, Presentation, and Provenance
---

# Labels, Presentation, and Provenance

Metadata describes an exact Revision without changing its identity. Labels and provenance are typed sets; presentation, aliases, notes, and trust use explicit current-value mutation rules.

### PR-REQ-0019 - Human label ambiguity

Human labels and display versions MUST be associations rather than Revision
identity. Multiple labels MAY refer to one Revision, and one label MAY refer to
multiple exact Revisions. Authoritative label equality and lookup MUST preserve
and compare the exact valid Unicode scalar-value sequence without normalization,
case folding, trimming, or locale-sensitive comparison.

Resolution MUST first deduplicate bindings by exact `RevisionIdentity`. Zero
distinct targets is unresolved, one is resolved, and more than one is
ambiguous. Ambiguity MUST fail with `resolution.ambiguous_reference` and MUST
NOT implicitly choose the latest, the most recently installed target, the
binding with the most source claims, or an older or newer association.
Deterministic results MUST use the typed ordering defined by PR-REQ-0250 rather
than insertion or query-plan order.

**Verification: PR-TEST-0532.**

### PR-REQ-0020 - Metadata observations

Reference labels, presentation, provenance, publisher attribution, and local
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
part of the portable bundle. Reference-label bindings, presentation metadata,
source-URI claims, publisher-attribution claims, and attribution claims are
semantically portable-capable because their Domain meaning does not depend on
one Pactrun installation. These fields MUST remain non-identity metadata.

Local install timestamps, source filesystem paths, local aliases, local notes,
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

Revision import MUST NOT be rejected solely because a human-readable label
becomes ambiguous. If the Revision or bundle is otherwise valid, import MUST
preserve both label associations and MAY emit a warning. Later human reference
resolution MUST report ambiguity and MUST NOT use last-write-wins or implicit
latest selection. The [Pack distribution contract](./distribution.md) defines
the bundle representation and import application policy.

**Verification: PR-TEST-0538, PR-TEST-0539, PR-TEST-0540.**

### PR-REQ-0248 - Descriptive metadata boundary {#pr-req-0248---m1-d-metadata-boundary}

Non-identity metadata persistence MUST be limited to the closed typed
descriptive values in PR-REQ-0249 through PR-REQ-0253: reference-label
bindings, current presentation, typed provenance claims, local aliases, one
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
Its closed types are `ReferenceLabelBinding`, current presentation metadata,
the three provenance claim types in PR-REQ-0252, local Revision aliases, one
optional local current note, and one optional local current trust assessment.
Metadata MUST NOT attach metadata to a Package, Instance, generic entity path, or an
as-yet-undefined local installation record.

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

### PR-REQ-0250 - Reference-label binding, lookup, and ordering

A `ReferenceLabelBinding` MUST be the complete tuple of a non-empty exact
`ReferenceLabel`, one exact `RevisionIdentity`, and one typed source. The source
union and stable rank are `Unattributed < SourceUri < Publisher <
PublisherSourceUri`. A publisher contains a non-empty exact publisher name and
an optional non-empty exact namespace; source-bearing variants contain an exact
`SourceUri`. The complete tuple determines equality. Adding an identical tuple
or removing an absent identical tuple MUST be idempotent; removing one tuple
MUST NOT remove another source or target binding. A label has no independent
managed object or lifetime apart from its bindings.

Authoritative lookup by label MUST collect distinct exact Revision targets.
Multiple source tuples for the same target MUST NOT make that target ambiguous;
multiple distinct Revision targets MUST. Lookup results and enumeration MUST
order by label UTF-8 bytes, `PackageId` canonical bytes,
`RevisionContentDigest` canonical bytes, source-variant rank, and then the
source fields in their declared tuple order. Optional source fields use the
ordering in PR-REQ-0249.

**Verification: PR-TEST-0058, PR-TEST-0062, PR-TEST-0066.**

### PR-REQ-0251 - Closed current presentation model

Current presentation metadata MUST be keyed by one exact Revision, one
`PresentationTargetV1`, and one `PresentationField`. The target union and stable
rank are, in order: Revision root, Input, Action, Action Parameter, Managed
Output, Snapshot Capture, Snapshot Capture Parameter, Snapshot Restore,
Snapshot Restore Parameter, Migration edge, and Cleanup. Nested targets MUST
use their typed parent and child semantic identities. A Migration edge MUST use
the exact source Revision selector within the target Revision context.

The presentation fields and stable rank are `display_name < summary <
description < help`. Every one of the four fields is valid for every target in
the closed target union. Values MUST be non-empty exact strings under
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

### PR-REQ-0253 - Local aliases, notes, and trust

A `LocalAlias` MUST be a non-empty exact string in a Revision-only namespace
within one management environment. One alias may target at most one exact
Revision, and one Revision may have multiple aliases. Setting the same target
is idempotent; a different target is a conflict unless an explicit semantic CAS
authorizes the replacement. Alias equality and conflict use the exact typed
value, not normalized or fuzzy search output.

Each exact Revision MAY have one non-empty local current note and one local
current trust assessment. Note absence is distinct from an empty string. Trust
is exactly `Trusted | Distrusted`, with absence meaning `NoDecision`.
`Distrusted` is descriptive local assessment only and MUST NOT independently
deny installation or execution. Note and trust replacement and clearing use
PR-REQ-0255. Pactrun MUST NOT create alias, note, or trust history, opaque IDs,
purpose-scoped trust, policy enforcement, or a metadata version token.

Local install time and source filesystem path are local-only. They MUST NOT
be persisted as current Revision metadata: this metadata model does not define
a `LocalInstall` identity, cardinality, or lifecycle.

**Verification: PR-TEST-0058, PR-TEST-0064, PR-TEST-0066, PR-TEST-0530, PR-TEST-0535.**
