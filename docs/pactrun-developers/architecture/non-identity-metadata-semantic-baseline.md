---
title: Non-Identity Metadata Semantic Baseline
---

# Non-Identity Metadata Semantic Baseline

**Status: Canonical semantic synthesis and navigation entry point.**

This page summarizes the Pre-M1-D closure for Revision-scoped non-identity
metadata. It is not a competing normative source and contains no independent
requirement. Normative authority remains with the linked requirement-bearing
specification pages. If a summary here differs from a linked requirement, the
linked requirement controls.

The closure now has an integrated crate-private implementation. It does not
change Revision identity, any Frozen format or wire contract, a public API, or
CLI behavior; the implemented persistence schema remains non-Frozen and
non-public.

## Closed semantic model

| Topic | Closed synthesis | Normative authority |
| --- | --- | --- |
| Metadata boundary | M1-D is limited to typed, Revision-scoped descriptive metadata and cannot persist ServiceStorage or other operational state. | [PR-REQ-0248](./identity-and-state.md#pr-req-0248---m1-d-metadata-boundary), [PR-REQ-0249](./identity-and-state.md#pr-req-0249---typed-metadata-scope-and-authoritative-strings) |
| Authoritative strings | Exact valid Unicode scalar sequences retain their UTF-8 bytes without normalization, case folding, trimming, or locale comparison. Source URIs match the ASCII RFC 3986 `URI` production: a scheme is required, relative references and non-ASCII input are rejected, fragments are allowed, and caller-provided bytes are preserved without URI normalization or parser reserialization. | [PR-REQ-0249](./identity-and-state.md#pr-req-0249---typed-metadata-scope-and-authoritative-strings) |
| Reference labels | A binding is an exact label, Revision identity, and closed typed source tuple. Lookup deduplicates sources by target Revision before deciding one result or ambiguity. | [PR-REQ-0019](./identity-and-state.md#pr-req-0019---human-label-ambiguity), [PR-REQ-0250](./identity-and-state.md#pr-req-0250---reference-label-binding-lookup-and-ordering) |
| Presentation | Presentation is current metadata keyed by exact Revision, closed typed target, and one of four fields. Every field is valid for every closed V1 target. Clear means absence, not an empty value or tombstone. | [PR-REQ-0020](./identity-and-state.md#pr-req-0020---metadata-observations), [PR-REQ-0251](./identity-and-state.md#pr-req-0251---closed-current-presentation-model) |
| Provenance | Provenance is a set of exact typed claims: source URI, publisher attribution, or attribution. Equal complete tuples deduplicate; different tuples coexist. | [PR-REQ-0022](./identity-and-state.md#pr-req-0022---repeat-revision-import), [PR-REQ-0252](./identity-and-state.md#pr-req-0252---typed-provenance-claims) |
| Local metadata | A local alias binds at most one Revision. Each Revision has at most one current non-empty note and one current `Trusted` or `Distrusted` assessment; row/state absence means no value. | [PR-REQ-0253](./identity-and-state.md#pr-req-0253---local-aliases-notes-and-trust) |
| Portability | Presentation, reference-label bindings, and the three provenance claim types are portable-capable. Alias, note, and trust are local-only. Classification does not define bundle carriage or import merge. | [PR-REQ-0021](./identity-and-state.md#pr-req-0021---portable-and-local-metadata), [PR-REQ-0254](./resources-and-versioning.md#pr-req-0254---non-identity-metadata-portability-boundary) |
| Mutation and concurrency | One typed batch targets one existing exact Revision. Set-like updates are idempotent; current values use semantic CAS. CAS compares only current state and intentionally cannot detect ABA or mutation history. | [PR-REQ-0255](./persistence-schema-v2.md#pr-req-0255---typed-metadata-mutation-and-repository-contract) |
| Persistence | PersistenceSchemaV2 is the current implemented internal schema and fixes canonical relational representation, exact keys and constraints, current-state absence, deletion cascades, and a crate-private typed repository contract. | [PR-REQ-0256](./persistence-schema-v2.md#pr-req-0256---exact-persistenceschemav2) |
| Migration | Pristine databases create V2 directly. Exact V1 migrates transactionally to exact V2; foreign, partial, drifted, and newer schemas are rejected. Revision identity and canonical bytes remain unchanged. | [PR-REQ-0078](./resources-and-versioning.md#pr-req-0078---persistence-migrations), [PR-REQ-0257](./persistence-schema-v2.md#pr-req-0257---v1-to-v2-migration-and-validation) |

## Closed presentation relation

`PresentationTargetV1` contains Revision root, Input, Action, Action Parameter,
Managed Output, Snapshot Capture, Snapshot Capture Parameter, Snapshot Restore,
Snapshot Restore Parameter, Migration edge, and Cleanup. The closed field set
is `display_name`, `summary`, `description`, and `help`. The relation is the
complete target-by-field cross-product: all four fields are valid for every
closed target. Target existence is nevertheless validated against the exact
strict-decoded `RevisionCoreV1` named by the metadata.

This is only a navigation summary of
[PR-REQ-0251](./identity-and-state.md#pr-req-0251---closed-current-presentation-model).

## Deterministic comparison

Every repository operation that promises deterministic lookup or enumeration
uses the complete typed comparator below. Ordering is repeatability, not
preference, recency, trust precedence, or an ambiguity tie-breaker.

| Value | Comparator synthesis | Normative authority |
| --- | --- | --- |
| Revision identity | Canonical 16-byte `PackageId`, then canonical 32-byte `RevisionContentDigest`. | [PR-REQ-0250](./identity-and-state.md#pr-req-0250---reference-label-binding-lookup-and-ordering) |
| Exact text | Unsigned lexicographic comparison of preserved UTF-8 bytes. | [PR-REQ-0249](./identity-and-state.md#pr-req-0249---typed-metadata-scope-and-authoritative-strings) |
| Optional value | `Absent` before `Present`; present payload uses its typed comparator. | [PR-REQ-0249](./identity-and-state.md#pr-req-0249---typed-metadata-scope-and-authoritative-strings) |
| Closed enum or union | Explicit stable variant rank, then declared typed components. | [PR-REQ-0249](./identity-and-state.md#pr-req-0249---typed-metadata-scope-and-authoritative-strings) |
| Full enumeration | Revision identity, then the fixed kind order: reference label, presentation, source-URI claim, publisher-attribution claim, attribution claim, local alias, local note, local trust; then the complete type-specific comparator. | [PR-REQ-0255](./persistence-schema-v2.md#pr-req-0255---typed-metadata-mutation-and-repository-contract) |

SQL must spell every correctness-relevant `ORDER BY` component explicitly and
remain parity-equivalent with Domain ordering. Rowid, insertion order,
timestamps, query-plan order, SQLite NULL ordering, and locale collation are
not semantic inputs.

## Implemented internal persistence boundary

[Persistence Schema V2](./persistence-schema-v2.md) is the current implemented
normative internal persistence contract. Its exact DDL uses
non-null semantic key components, explicit presence ranks and canonical empty
payload sentinels for optional tuple values, fixed integer variant ranks,
primary keys over complete semantic tuples, and row absence for absent current
note, trust, alias, or wholly absent presentation.

The integrated M1-D repository implements this schema rather than inferring a
generic metadata table, JSON/EAV store, nullable alternate representation, or
opaque serialized Domain object.

## Deferred design

- Export Bundle serialization, carriage, import replacement, conflict, and
  merge policy;
- Package- or Instance-scoped metadata and the `LocalInstall` model;
- localization, normalized or fuzzy discovery, and additional provenance or
  asserted-time claims;
- trust policy and history, note history, audit generations, metadata version
  tokens, and history-sensitive CAS or ABA protection;
- stable new error codes, cross-domain Revision-plus-metadata publication,
  public API, CLI, and runtime integration;
- every ServiceStorage persistence, authority, continuity, retention, discard,
  Cleanup, finalization, and Abandon non-destruction representation.

## M1-D implementation closure

The integrated implementation follows the closed typed requirements and schema
above. Automated coverage includes fresh V2 and V1-to-V2 behavior, crash and
concurrent migration boundaries, canonical tuple and absence rejection, the
full presentation target-by-field cross-product, deterministic SQL/Domain
comparator parity, idempotency, semantic CAS including its intentional ABA
limitation, alias conflict, cascade deletion, reload equivalence, and proof that
Frozen canonical bytes and digests do not change.
