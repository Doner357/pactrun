---
title: Snapshot Integrity Format V2
---

# Snapshot Integrity Format V2

**Status: Frozen normative Package contract specification.**

This is a separate integrity format, not a correction to Frozen V1. Its S1
format gate passed on the M4 feature branch on September 11, 2026: production
Rust verification, independent Node 24 verification, golden vectors, and
traceability. All V1 requirements, verifier semantics, canonical bytes, and
golden vectors remain unchanged. This format freeze does not claim implemented
Snapshot Capture, Restore, bundle operations, or V5 production persistence.

Candidate-to-Frozen changes lifecycle metadata only. These V2 schema,
relational semantics, canonical bytes, and framing are now Frozen; an
integrity-affecting change requires another integrity format version.

### PR-REQ-0295 - V2 schema, normalization, and hash domain

V2 MUST use the same closed object/union fields as SnapshotIntegrityManifestV1
in PR-REQ-0197, with format_version exactly 2 instead of 1. Identifier,
timestamp, Unicode, duplicate-property, semantic-key, payload-closure, and
ordering rules remain those specified for V1 in PR-REQ-0198 through
PR-REQ-0202. In particular, absence is not an empty payload and every referenced
blob, including a zero-length blob, MUST be present and cryptographically
verified before full content verification can succeed. Resource limits are
not integrity validity rules.

The normalized manifest is encoded using the same RFC 8785 JCS contract.
The exact SHA-256 input is:

```text
ASCII("pactrun.snapshot-integrity-digest\0")
U32BE(2)
ASCII("snapshot-integrity-manifest\0")
U64BE(len(manifest_jcs))
manifest_jcs
```

Lengths count octets. The selected version, in-band manifest version, and
framing version MUST agree. SnapshotId remains object identity; the integrity
digest MUST NOT substitute for it. Origin names, creator Runs, local metadata,
trust, archive encoding, and Session locators remain outside the manifest.

**Verification: PR-TEST-0185, PR-TEST-0187, PR-TEST-0188, PR-TEST-0189, PR-TEST-0190, PR-TEST-0194.**

### PR-REQ-0296 - V2 binding protection semantics

The single protection field MUST NOT be universally interpreted as stored
protection. For a bound descriptor it records authoritative captured binding
protection. For an active absent descriptor it records producer declaration
protection because no stored binding exists. Retained absent remains invalid.

With exact producer semantics supplied, V2 MUST validate the following table,
as well as V1's complete active declaration coverage and active/retained roles:

| Descriptor | Producer declaration | Allowed protection |
| --- | --- | --- |
| active bound | Normal | Normal or Secret |
| active bound | Secret | Secret only |
| active absent | Normal | Normal only |
| active absent | Secret | Secret only |
| retained bound | undeclared | Preserve captured Normal or Secret |
| retained absent | undeclared | Invalid |

There are no declared_protection/stored_protection fields. No producer context
means these relational predicates are not_evaluated, not implicitly valid.
A mismatched supplied producer identity is invalid context. Runtime Capture
completeness and required-input readiness remain separate requirements;
format-representable required absence does not authorize incomplete Capture.

**Verification: PR-TEST-0185, PR-TEST-0186, PR-TEST-0189, PR-TEST-0190, PR-TEST-0192.**

### PR-REQ-0297 - Current writer and historical readers

M4 Capture MUST emit V2 only, without automatic V1 selection or downgrade
options. Import, Verify, and Restore MUST dispatch explicitly to V1 or V2 and
apply that version's original rules. V2 acceptance MUST NOT rescue invalid V1.
Unsupported versions MUST fail closed rather than be guessed.

Import/export MUST preserve the selected integrity version, normalized
canonical representation, SnapshotId, digest, and exact payload closure. Raw
JSON whitespace is not canonical identity. No implicit upgrade or downgrade
is allowed. Producer installation is unnecessary for intrinsic/content
verification and import; Restore needs exact producer semantics.

V2 MUST have checked-in positive/negative vectors and an independent Node
oracle using public fixture bytes, not Rust-produced expected digests as
calculation input. Original V1 verification MUST continue unchanged. Raw JSON
duplicate and numeric validation remain Rust-verifier responsibilities.

**Verification: PR-TEST-0184, PR-TEST-0185, PR-TEST-0186, PR-TEST-0187, PR-TEST-0190, PR-TEST-0191, PR-TEST-0193, PR-TEST-0206, PR-TEST-0208, PR-TEST-0214.**

The S1 tests cover the crate-private codecs, explicit version dispatch,
producer checks, canonical bytes/digests, referenced-content verification,
safe diagnostic values, and independent normalized-fixture oracle. They do not
claim implemented Capture or Restore commands. S3 services now enforce
original-version bundle import/export and producer verification; Capture,
Restore execution, and human command dispatch remain S5/S6/S7 gates.
