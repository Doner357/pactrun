---
title: Input Bindings and Protection
---

# Input Bindings and Protection

Each Instance has one binding registry. A binding is active or retained according to the active Revision; those roles do not create separate stores or historical values.

### PR-REQ-0028 - Stable Input identity and opaque bytes

Within one Package lineage, the same `InputIdentity` MUST always represent the
same long-lived semantic Input and MUST NOT be reused for another meaning. Input
payloads MUST be preserved byte-for-byte, including empty payloads; Pactrun MUST
NOT infer format or semantics from content or filenames.

**Verification: PR-TEST-0075, PR-TEST-0147, PR-TEST-0590.**

### PR-REQ-0029 - One managed binding registry

An Instance MUST have at most one binding for each `InputIdentity`. `Active` and
`retained` MUST be roles derived relative to the active Revision, not separate
stores or historical value logs. A binding declared by the active Revision is
active; an existing undeclared binding is retained.

**Verification: PR-TEST-0280, PR-TEST-0285, PR-TEST-0591.**

### PR-REQ-0030 - Retained binding lifetime

A retained binding MUST persist until it becomes active again, is explicitly
discarded by Migration, is explicitly deleted by the operator, or the Instance
is removed or abandoned. Historical Revision identity MAY be provenance but
MUST NOT be part of binding identity.

**Verification: PR-TEST-0610, PR-TEST-0612, PR-TEST-0614.**

### PR-REQ-0031 - RequiredInputsSatisfied

`RequiredInputsSatisfied` MUST be derived from required declarations in the
active Revision and current managed bindings. Empty bindings count as present.
Missing required bindings MUST NOT invalidate the Instance or create a separate
lifecycle state.

**Verification: PR-TEST-0225, PR-TEST-0321, PR-TEST-0525, PR-TEST-0590, PR-TEST-0599.**

### PR-REQ-0032 - Initial binding acquisition and Instance publication

Instance creation MAY commit an incomplete Instance. When a request explicitly
supplies an initial binding, it MUST identify the Input and acquisition source
explicitly and deterministically. The Instance and every explicitly supplied
binding MUST be one atomic management commit. Any acquisition or validation
failure for such a binding MUST fail the whole create request; Pactrun MUST NOT
silently omit it and create an Instance more incomplete than requested.

Acquisition and bounded file-backed staging finish before the database
transaction, and the exact atomic publication and cleanup contract is
PR-REQ-0265.

**Verification: PR-TEST-0147, PR-TEST-0590, PR-TEST-0599.**

### PR-REQ-0033 - Binding mutation invariants

An active required binding MAY initially be absent, but once bound under that
active Revision it MUST NOT be ordinarily unset or deleted. Active optional
bindings MAY be set, replaced, or removed. Replacements MUST be atomic. Retained
bindings MAY be inspected, explicitly exported, or deleted, but MUST NOT be
directly set or replaced by the operator.

**Verification: PR-TEST-0076.**

### PR-REQ-0034 - Secret protection

Secret protection MUST be sticky. The immutable payload's persisted
`Normal | Secret` protection is the stored protection floor for a binding. For
an active binding, effective protection is the stronger of that stored floor
and the active declaration. For a retained binding, which has no active
declaration, effective protection is the stored floor. An active Secret
declaration therefore requires a committed Secret payload, while an active
Normal declaration with a Secret payload is valid and remains effectively
Secret.

Normal data MAY be promoted to Secret automatically, but Secret data MUST NOT
be implicitly downgraded. Declassification requires an explicit Migration
transition and explicit operator authorization. Retained Secrets MUST retain
protection, and ordinary inspection, diagnostics, Run records, and user-visible
metadata MUST NOT disclose their values or value-derived digests.

Only a deletion already permitted by PR-REQ-0033 ends the old binding's sticky
continuity. An active required binding remains non-deletable once bound. A
later bind after an allowed deletion is a newly acquired binding whose initial
protection follows the then-active declaration; it MUST NOT be described as
declassifying the old Secret payload and does not add a secure-erasure claim.

Publication, persistence validation, storage, staging, and disclosure apply
this rule through PR-REQ-0264, PR-REQ-0266, PR-REQ-0268, and PR-REQ-0269.
Secret classification MUST NOT be confused with cryptographic storage.

**Verification: PR-TEST-0076.**
