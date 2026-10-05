---
title: Identity and Metadata During Import
---

# Identity and Metadata During Import

### PR-REQ-0084 - Revision import identity

Revision export and import MUST preserve `PackageId`,
`RevisionContentDigest`, `RevisionCore`, and owned runtime content. The same
Package ID and digest MUST be idempotent; a different digest under the same
Package ID MUST form another Revision; the same digest under another Package ID
MUST remain a distinct Revision identity. Export and import MUST also preserve
the bundle-defined portable non-identity metadata required by
[PR-REQ-0021](./metadata.md#pr-req-0021---portable-and-local-metadata).

The [Pack distribution format](./distribution.md) determines
which portable metadata is carried and how import applies conflicts and merges.
Being portable-capable alone does not require a field to be exported.

**Verification: PR-TEST-0538, PR-TEST-0539, PR-TEST-0540, PR-TEST-0544, PR-TEST-0594.**

### PR-REQ-0254 - Non-identity metadata portability boundary

Current presentation metadata, `ReferenceLabelBinding`, `SourceUriClaim`,
`PublisherAttributionClaim`, and `AttributionClaim` MUST be semantically
portable-capable: their Domain meaning is valid across Pactrun installations
and MUST NOT contain a local database identity, install event, row identity,
host path, or local trust conclusion.

Local aliases, local current notes, local current trust assessments, local
install timestamps, and source filesystem paths MUST be local-only.
Portable-capable MUST NOT be equated with identity-bearing or automatically
exported. Local persistence MUST NOT be treated as evidence that a kind is
local-only. Export serialization, selection, carriage, import conflict and
merge policy are owned by the [Pack distribution contract](./distribution.md)
and MUST NOT alter Revision identity.

**Verification: PR-TEST-0058.**
