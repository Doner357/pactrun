---
title: CLI Object ID Selectors
---

# CLI object ID selectors

**Status: Approved CLI contract, 2026-09-23.**

<!-- spec-navigation:start -->
## Reading map (informative)

This page owns abbreviated CLI object references and usable human list IDs.
Read [CLI JSON V1](./cli-machine-interface.md) for machine output and
[command behavior](../behavior/command-and-output-reference.md) for operations.
The [implementation record](../../development/short-id-selectors-status.md)
tracks verification and integration separately from this contract.
<!-- spec-navigation:end -->

### PR-REQ-0369 - Unique object prefixes

CLI object-ID operands accept lowercase hexadecimal prefixes of at least eight
digits. This covers Run, Snapshot, historical Instance and service allocation IDs,
including mutation operands, parent IDs, and pagination cursors. Full IDs retain
their existing semantics, including absent-object deletion and deleted cursors.
State-version/CAS tokens and author-assigned names remain exact.

Revision references retain `exact:<package>/sha256:<digest>`; either hexadecimal
component may be abbreviated. Labels and aliases keep their explicit syntax.
Migration paths accept `mp1-<8..63 hex digits>` as a fingerprint prefix; complete
`mp1-` selectors retain their existing self-contained path semantics. Migration
Input target digests retain `sha256:` and may be abbreviated within the Instance's
installed Package lineage.

Parsing validates the entire command before resolving prefixes. Resolution is
read-only and precedes staging, payload acquisition, acceptance, and mutation.
Zero matches fails; multiple matches reports bounded, deterministic full candidates
and requests a longer prefix. All objects of the selected kind participate,
regardless of status, deletion eligibility, list filters or page boundaries.
Migration paths use all relational candidates from one graph observation for the
selected source Instance and target Revision, without filtering for readiness.
Resolved references are fixed full identities. Execution must never re-resolve a
prefix or redirect it to a newly created object; existing admission, CAS and
authorization checks still apply.

JSON/JSONL errors retain their envelopes and add typed ambiguity candidates in
the existing nullable result. Malformed selectors are usage errors; absent or
ambiguous prefixes are operation errors. No persistence or identity format changes.

**Verification: PR-TEST-0586, PR-TEST-0587, PR-TEST-0588, PR-TEST-0589.**

### PR-REQ-0370 - Usable human abbreviations

Human lists use at least twelve hexadecimal digits for primary object IDs, extending
through collisions across the entire corresponding namespace, including objects
outside the current page. Displayed abbreviations must be accepted as selectors.
Revision and Migration syntax tags remain visible. A compound Migration path whose
fingerprint cannot be shortened uniquely retains its complete selector.
`--no-trunc`, detailed inspections, JSON and JSONL retain full identities.
Machine pagination tokens and identity/state fields retain their existing meaning.
Historical provenance references that may have no local catalog object retain
their full spelling, including imported Snapshot origin Instance IDs.
Names and authored IDs are not subject to prefix inference.

**Verification: PR-TEST-0587, PR-TEST-0589.**

The [CLI JSON contract](./cli-machine-interface.md) and
[command reference](../behavior/command-and-output-reference.md) retain ownership
of response and operation semantics. This selector layer does not add authority.

The minimum eight-digit input length, twelve-digit display floor and global
collision checks are Pactrun choices. They avoid silent guesses and preserve
existing exact identity, CAS and authorization semantics. Docker's named/full/short
container references inform the usability goal; see the
[design references](../../development/design-references.md).
