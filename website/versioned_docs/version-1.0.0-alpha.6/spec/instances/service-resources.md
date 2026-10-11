---
title: Service Resource Ownership and Identity
---

# Service Resource Ownership and Identity

Pactrun provides storage lifetime and manages resource declarations. The service owns the live contents. A declaration, a locator, and an observation of existence describe different facts.

### PR-REQ-0235 - Persistent Instance-data ownership

Pactrun MUST distinguish Pactrun-authoritative Managed Input Bindings from
service-authoritative live state in Pactrun-provided ServiceStorage. A Managed
Input Binding is an Instance-scoped, persistent, detached opaque value whose
authoritative copy, presence, replacement, retention, and Secret protection are
managed by Pactrun. A ServiceStorage-backed Managed Service Resource is attached
live state whose authoritative bytes or contents are owned and used by the
service. Pactrun ownership of the provided storage lifetime MUST NOT be
misrepresented as ownership of those contents.

A persistent file MUST NOT be classified as an Input merely because Pactrun or
a user needs to see or modify it. Pactrun MUST NOT maintain an Input and a
service-owned file as implicit bidirectionally synchronized authoritative
copies. A service-owned file is not implicitly transferred into a Managed Input.

**Verification: PR-TEST-0346, PR-TEST-0370.**

### PR-REQ-0236 - Managed Service Resource declaration and existence

`ServiceStorage` MUST mean Pactrun-provided, Instance-scoped persistent storage
made available to service or Hook behavior across Runs. A ServiceStorage-backed
Managed Service Resource MUST have a stable semantic identity declared by a
Revision and a contractual association with a ServiceStorage semantic identity.
It MAY also have a locator, user exposure, access, and operation prerequisites.
A file-shaped ServiceStorage-backed resource MAY be called a Managed Service
File.

Pactrun may manage those contracts and cross-Revision semantic continuity, but
MUST NOT thereby claim ownership, content addressing, versioning, automatic
Snapshot inclusion, or mutation linearization of the service-owned live bytes.
The declaration exists independently from the live object: a Revision MAY
declare a resource that is currently absent and that the service creates later.

The identity encoding, association schema, access encoding, prerequisite
encoding, authoring form, wire authority, and durable representation remain
independent version boundaries defined by the Revision, Hook Protocol, Pack
source, and persistence contracts. This
requirement does not classify non-ServiceStorage-backed service-owned resources.

**Verification: PR-TEST-0346.**

### PR-REQ-0241 - ServiceStorage and resource semantic identity

A ServiceStorage declaration and each ServiceStorage-backed Managed Service
Resource declaration MUST have separate stable semantic identities within one
Package lineage. An identity MUST retain one long-lived semantic meaning across
Revisions and MUST NOT be reused for another storage or resource meaning. The
same declaration identity in different Instances denotes the same contract role,
not a shared physical storage or live object, and identities in different
Packages MUST NOT be treated as implicitly equivalent.

A Managed Service File is a file-shaped subtype of a ServiceStorage-backed
Managed Service Resource, not a third persistent ownership model. A resource's
storage-relative locator identifies its contractual logical location within the
associated storage; it MUST NOT become resource identity, a host-path contract,
an existence assertion, or evidence of representation compatibility.

**Verification: PR-TEST-0346, PR-TEST-0371.**

Package authors are responsible for preserving an identity's long-lived
semantic meaning. Pactrun does not infer that meaning from service contents.

### PR-REQ-0242 - Declaration, existence, and observation

A Revision declaration and the corresponding Instance live resource existence
MUST remain separate facts. Live existence has the point-in-time semantic states
`Present`, `Absent`, and `Unknown`. `Present` means only that the resource was
observed to exist at the relevant observation boundary; it MUST NOT assert that
the contents are valid, coherent, unchanged afterward, or compatible with a
Revision. `Unknown` MUST NOT be interpreted as either presence or absence.

Pactrun MAY observe existence only through a capability that can make the
relevant point-in-time observation. It MUST NOT claim continuous observation,
advance `InstanceStateVersion` merely because observed existence or live bytes
change, or apply Managed Input presence and mutation linearization to the live
resource.

**Verification: PR-TEST-0346, PR-TEST-0354, PR-TEST-0357, PR-TEST-0387, PR-TEST-0388.**

### PR-REQ-0272 - Authoring and managed-data boundaries {#pr-req-0272---m2-scope-and-anti-backdoor-boundary}

Pactrun MUST NOT reinterpret a Managed Input, descriptive metadata value,
transient staging file, payload chunk, Instance state token, or database row as
ServiceStorage or a ServiceStorage-backed Managed Service Resource. Authoring,
installation, and binding management do not supply service-storage authority.

Successful validation and projection of capability declarations establish an
installation definition, not satisfaction of an operation's execution
prerequisites. Runtime execution follows its own operation, authority, and
publication contracts.

Pack source and internal persistence have independently versioned contracts.
They MUST NOT modify or acquire the compatibility guarantees of Revision Core,
Snapshot Integrity, Hook Protocol, or Error Taxonomy. Authoring, binding, and
metadata changes MUST NOT invent service semantics outside the owning resource
and execution contracts.

**Verification: PR-TEST-0079.**
