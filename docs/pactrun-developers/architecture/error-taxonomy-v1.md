---
title: Error Taxonomy V1
---

# Error Taxonomy V1

**Status: Frozen normative architecture specification.**

This page defines the stable Pactrun-owned error identity and append-only
catalog contract. Candidate entries are conformance data, not evidence that a
production Resolver, Compiler, Admission path, Executor, persistence adapter,
or recovery implementation exists. Freezing changes status and verification
metadata only.

### PR-REQ-0219 - Stable error reference

A Pactrun-owned error identity is exactly:

```text
PactrunErrorRefV1
|- owner: ErrorOwnerV1
`- code: ErrorCodeV1
```

`owner` and `code` each contain 1 through 128 ASCII bytes and match:

```regex
^[a-z][a-z0-9]*(?:_[a-z0-9]+)*$
```

Their spelling is exact and case-sensitive. No Unicode normalization or other
normalization is performed. A dotted rendering such as
`resolution.ambiguous_reference` is informative only and is not a second error
identity.

Messages, diagnostics, severity, typed details, failed-step metadata, source or
cause presentation, and localization are not part of the identity. V1 does not
define a global details map, severity, retryability property, presentation
fallback, CLI envelope, transport envelope, or persistence representation.

**Verification: PR-TEST-0033, PR-TEST-0036, PR-TEST-0037.**

### PR-REQ-0220 - Owner registry and categories

The initial owner registry is:

```text
revision_core_format_v1
snapshot_integrity_format_v1
hook_protocol_v1
domain_validation
resolution
compilation
admission
execution
persistence
recovery
```

The owner registry is append-only rather than a closed V1 enum. Existing owner
spellings MUST NOT be removed, renamed, or repurposed. A new subsystem MAY add
a lexically valid owner after its ownership boundary has a normative source.
Adding an owner does not by itself require a taxonomy version change.

The V1 category vocabulary is closed:

```text
representation_validation
intrinsic_semantic_validation
relational_semantic_validation
resolution
compilation
admission
execution
protocol
persistence
recovery
```

Category is stable classification metadata, not error identity or validation
execution order. An existing `(owner, code)` MUST NOT be reassigned to another
category. An incompatible category-vocabulary change requires taxonomy version
evolution.

`PactrunErrorTaxonomyCatalogV1` has a closed JSON schema containing `format`,
`status`, `owners`, and `codes`. Owner entries contain `owner` and non-empty
`normative_sources`. Code entries contain `owner`, `code`, `category`, and
non-empty `normative_sources`. Every code entry MUST refer to an owner present
in the same catalog. Owner spelling and `(owner, code)` MUST be unique.

The owner and code arrays are semantic sets. Array order does not add meaning;
the repository serialization sorts owners by spelling and codes by
`(owner, code)` for stable review diffs.

**Verification: PR-TEST-0033, PR-TEST-0034.**

### PR-REQ-0221 - Normative source and ownership

A catalog entry requires a resolvable normative source that already establishes
its product-level semantic boundary. A Frozen specification, an explicit
`PR-REQ` requirement, a Frozen local-code registry, or a Frozen conformance
vector may serve as that source. A verifier string, enum variant, adapter error,
or test-harness label alone is not authority.

The direction of authority is:

```text
normative specification
        -> taxonomy catalog
        -> verifier conformance
```

The initial `revision_core_format_v1` entries are:

```text
duplicate_property
invalid_hook_launch
invalid_host_executable_name
invalid_number
invalid_source_binding_role
invalid_unicode_scalar
unknown_field
```

The initial `snapshot_integrity_format_v1` entries are:

```text
content_digest_mismatch
duplicate_property
duplicate_semantic_key
incomplete_binding_state
invalid_binding_protection
invalid_binding_role
invalid_binding_state
invalid_digest
invalid_format_version
invalid_number
invalid_resource_id
invalid_snapshot_path
invalid_unicode_scalar
missing_content_blob
missing_field
unknown_field
```

The `hook_protocol_v1` entries are exactly the Pactrun-originated closed local
code registry in PR-REQ-0217:

```text
invalid_preamble
frame_too_large
invalid_utf8
invalid_json
duplicate_property
invalid_unicode_scalar
unsupported_protocol_version
unknown_field
invalid_message
unexpected_message
request_in_flight
request_id_mismatch
invalid_recovery_transition
invalid_session_relative_path
duplicate_semantic_key
invalid_authority
invalid_completion
completion_with_open_risk
```

The initial cross-layer entries are `resolution.ambiguous_reference`, sourced
from PR-REQ-0019 and PR-REQ-0086, and `admission.plan_invalidated`, sourced from
PR-REQ-0043 and PR-REQ-0096. Empty initial owners do not receive placeholder
`failed`, `internal_failure`, or `unknown_failure` codes.

The following Revision Core verifier spellings are not registered because their
product rejection semantics do not give these spellings Frozen public status:

```text
duplicate_semantic_key
invalid_digest
invalid_identifier
invalid_json_syntax
invalid_reference
invalid_runtime_path
invalid_snapshot
invalid_transition
invalid_type
missing_field
unsupported_format_version
```

The same exclusion applies to Snapshot Integrity verifier spellings
`invalid_identifier`, `invalid_json_syntax`, `invalid_producer_context`, and
`invalid_type`. `invalid_blob_fixture` is test-harness vocabulary. Exclusion
does not weaken the underlying Frozen validation rules and does not prevent a
future separately approved normative source from registering a stable code.

A Hook-authored `HookCodeV1` remains Hook- or Package-owned and MUST NOT be
reclassified as a Pactrun error-taxonomy code. The `hook_protocol_v1` owner is
only for Pactrun-originated PR-REQ-0217 protocol failures.

**Verification: PR-TEST-0035, PR-TEST-0037.**

### PR-REQ-0222 - Append-only evolution and unknown references

V1 MAY append a new owner, a new code under an existing owner, or codes under a
newly appended owner. Stable semantics MUST exist first, the normative source
MUST be recorded, and all existing entries MUST remain unchanged.

Within V1 an existing owner MUST NOT be removed, renamed, or repurposed. An
existing `(owner, code)` MUST NOT be removed, renamed, moved to another owner,
reused, incompatibly broadened or narrowed, or assigned another category. A
code that is no longer emitted remains registered and cannot be reused.

A generic `PactrunErrorRefV1` decoder validates only the lexical profiles. A
syntactically valid reference whose owner and/or code is unknown to an older
catalog MUST remain representable and preserve the exact spelling. It MUST NOT
be treated as success, silently mapped to another identity, or rejected solely
because the consumer catalog is older. No semantic understanding or
presentation behavior is implied.

Catalog conformance is different: a catalog code entry whose owner is not
registered in that same catalog is invalid.

An incompatible change to `PactrunErrorRefV1`, owner/code grammar, identity
meaning, the closed category vocabulary, or catalog structural semantics
requires taxonomy version evolution. A new owner or code alone does not.

**Verification: PR-TEST-0034, PR-TEST-0036.**

### PR-REQ-0223 - Error identity is not outcome or precedence

Run Outcome, `PrimaryFailure`, secondary failure-handling records,
`ManualRecoveryRequired`, readiness and trust state, Hook result, and
diagnostics are separate concepts rather than error identities. A Run may
reference a Pactrun error without replacing its outcome or trust consequence.
A Hook-reported failure retains its Hook-owned result instead of being silently
translated into a Pactrun-owned identity.

Resolution and compilation failures do not create a Run. The established
`admission.plan_invalidated` error occurs after durable Run creation. Catalog
fixtures verify only these semantic mappings; actual Resolver, Admission, Run
publication, persistence, and recovery behavior remain later integration work.

`PrimaryFailure` ordering is workflow-causal ordering under PR-REQ-0051, not a
global validation-precedence contract. Negative fixtures SHOULD isolate one
violation. Unless another normative contract defines an order, a multi-fault
input establishes no stable first-error ordering.

**Verification: PR-TEST-0037, PR-TEST-0038.**

### PR-REQ-0224 - Verifier and coverage boundary

The M0-C Rust verifier owns catalog closed-schema validation, owner/code lexical
validation, uniqueness, category membership, normative-source resolution,
Frozen local-code coverage, append-only fixture behavior, generic unknown
reference preservation, Candidate/Frozen metadata, and requirement/test
traceability.

It does not implement or verify production error propagation, Resolver or
Compiler behavior, Admission, Executor failures, persistence, Run failure
publication, recovery, CLI rendering, structured output, adapter behavior, or
runtime compatibility with unknown errors. No Node oracle is required because
this contract defines no canonical bytes, framing, digest, or cross-language
calculation.

The initial Freeze changes only this page and the catalog from Candidate to
Frozen status. Owners, codes, categories, normative sources, fixtures, and
semantic behavior MUST remain unchanged across that transition.

**Verification: PR-TEST-0033, PR-TEST-0034, PR-TEST-0035, PR-TEST-0036,
PR-TEST-0037, PR-TEST-0038.**
