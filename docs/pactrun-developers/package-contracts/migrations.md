---
title: Migrations
---

# Migrations

**Status: Normative Package contract specification.**

Migration is a managed, staged transition along a Revision graph. It is owned
by the target Revision and is not an Action.

## Edge declaration

### PR-REQ-0153 - Target-owned inbound edge

A target Revision MUST declare every supported inbound edge from an exact
source Revision in the same Package lineage. Each normalized edge MUST contain
explicit Input transitions, source and target requirements, mandatory target
outputs, and an optional Package Migration implementation.

M2 installation evaluates the separate repository-context relation in
PR-REQ-0262. A missing source is `NotEvaluated`, not an intrinsic target error;
an installed incompatible source is `Invalid`, and only `Valid` is eligible for
later execution.

**Verification: Pending automated coverage.**

### PR-REQ-0154 - Separate predicates

`MigrationRequirementsSatisfied`, `MigrationCompletionValid`, and target
`RequiredInputsSatisfied` MUST remain distinct. Edge success MUST require the
first two and MAY still leave the target ordinarily incomplete.

**Verification: Pending automated coverage.**

### PR-REQ-0155 - Per-edge commit

Each edge in a Migration chain MUST establish a separate durable commit
boundary. The active Revision and managed bindings remain at the source boundary
until the edge commit atomically publishes target Revision, staged binding
state, Managed Input disposition, and a new Instance state version. This is an
atomic Pactrun-owned state commit. It does not include service-owned filesystem,
database, Docker volume, or external-resource bytes, and it does not define a
future persisted representation for ServiceStorage-backed Managed Service
Resource association, continuity, retention, or discard.

**Verification: Pending automated coverage.**

### PR-REQ-0156 - Whole-path preflight

Before the first side effect, the Compiler SHOULD symbolically preflight the
complete selected path for statically detectable missing requirements, illegal
Secret transitions, target-writer conflicts, and unavailable resources.

**Verification: Pending automated coverage.**

## Input transitions

### PR-REQ-0157 - Canonical transition set

Normalized Managed Input Migration semantics MUST use explicit
`Carry(source -> target)`, `Declassify(source -> target)`, `Keep(source)`, and
`Discard(source)` transitions. `Discard` is always explicit. Input identity
rename or change requires an explicit mapping. These transitions MUST NOT be
silently generalized into a ServiceStorage-backed Managed Service Resource
transition schema.

**Verification: Pending automated coverage.**

### PR-REQ-0158 - Conservative shorthand

An authoring frontend MAY default the same stable Input identity to `Carry` and
a source-only Input to `Keep`, but normalization MUST materialize those choices.
Existing unrelated retained Input bindings persist without repeated `Keep`
declarations unless explicitly consumed, reactivated, or discarded. No
corresponding default or persisted retained-resource registry is defined here
for ServiceStorage-backed Managed Service Resources.

**Verification: Pending automated coverage.**

### PR-REQ-0159 - Declarative transition scope

Declarative Managed Input transitions MUST express binding continuity and
disposition only. They MUST NOT perform one-to-many, many-to-one, or arbitrary
payload transformation. Opaque Managed Input transformation belongs to a typed
Migration Hook output. Service-owned transformation follows the separate
future-version direction in PR-REQ-0237 and PR-REQ-0238.

**Verification: Pending automated coverage.**

### PR-REQ-0160 - Declassification contract

A Secret-to-Normal target MUST use explicit `Declassify` and require operator
authorization at execution. `Keep` MUST preserve protection, and a Hook MUST
NOT downgrade protection through its output.

**Verification: Pending automated coverage.**

## Requirements and outputs

### PR-REQ-0161 - Typed source and target requirements

An edge MAY require typed active or retained source bindings and staged target
bindings. Requirements are admission prerequisites rather than ACLs. A Pack MAY
use retained state for compatibility or recovery, and plan or lint output SHOULD
make that dependence visible.

The V1 identity spelling and the boundary between intrinsic target validation,
exact-source relational validation, and Admission are defined by
[Revision Core Format V1](./revision-core-format-v1.md). Source availability is
not made an installation prerequisite by that format specification.
PR-REQ-0262 fixes the M2 inspection and installation-order behavior without
changing the Frozen format.

**Verification: Pending automated coverage.**

### PR-REQ-0162 - Single target writer

Each target Input in one edge execution MUST have exactly zero or one effective
writer: Carry, Declassify, operator-provided target binding, Hook-produced target
binding, or absence. Multiple writers MUST fail validation or admission; hidden
overwrite precedence is prohibited.

**Verification: Pending automated coverage.**

### PR-REQ-0163 - Mandatory output completion

A Hook-produced target binding MUST use the typed canonical Session output and
must be declared. Every `produces.target` binding MUST exist before a successful
edge commit. Success with a missing mandatory output MUST fail completion
validation.

**Verification: Pending automated coverage.**

### PR-REQ-0164 - No requires-produces overlap

Without an explicit overwrite semantic, a target binding MUST NOT appear in
both `requires.target` and `produces.target`. Such a normalized contract MUST be
rejected.

**Verification: Pending automated coverage.**

## Ownership

### PR-REQ-0165 - Pactrun Migration responsibilities

Pactrun MUST own exact resolution, path selection, mutation guard, admission,
pins, staged target state, continuity and protection policy, checkpoints,
recovery directives, final atomic commits, active Revision switching, and Run
and Instance consequences.

**Verification: Pending automated coverage.**

### PR-REQ-0166 - Package Migration responsibilities

The Package MUST own database schema, application data formats, service
configuration semantics, external resources, and arbitrary opaque payload
transformation. A Migration Hook is optional when declarative state movement is
sufficient. For ServiceStorage-backed service-owned live resources, Pactrun does
not parse the payload or infer that a compatible transition requires byte
copying. This requirement does not classify other kinds of service-owned
resources.

**Verification: Pending automated coverage.**

## ServiceStorage-backed resource continuity

This section records accepted semantic policy for a future versioned design. It
does not define authoring syntax, Revision Core fields, Hook Protocol authority,
transition variants, a retained-resource registry, persistence tables, or an
atomic storage mechanism.

### PR-REQ-0237 - Service-resource continuity and conservative retention

When source and target Revisions declare the same ServiceStorage-backed Managed
Service Resource semantic identity and the Package declares their
representations compatible, they MUST interpret and use the same Instance
persistent live resource. Continuity MUST NOT be described as copying source-
Revision bytes into a target-Revision copy, and it MUST NOT require
rematerialization merely because the active Revision or service process changes.

A source-only ServiceStorage-backed Managed Service Resource MUST NOT be deleted
merely because the target Revision no longer declares it. The semantic default
is conservative non-destruction, and destructive removal or discard MUST be
explicit. Loss of an active declaration does not promise future discovery,
exposure, or reattachment. This requirement does not introduce, imply, or name a
persisted retained-resource registry, binding registry, state table, or other
durable representation. How continuity, compatibility, retention, and discard
map to Pactrun-owned durable state remains a future design gate.

**Verification: Pending automated coverage.**

### PR-REQ-0238 - Service transformation and recovery boundary

When a ServiceStorage-backed Managed Service Resource changes locator or
representation, splits, merges, or needs another service-specific
transformation, the target-owned inbound Migration semantics and Migration Hook
MUST own that transformation. Pactrun MUST NOT parse or transform the service-
owned payload itself.

Before crossing a boundary that may leave service-owned state outside a known
coherent state after permanent Hook loss, the Hook MUST use the existing
`EnterRecoveryRisk` acknowledgment. It MUST use `ResolveRecoveryRisk` only after
the service-owned state reaches a target-coherent boundary. Target coherence
alone MUST NOT be presented as a completed Pactrun target publication. Pactrun
MUST NOT claim that the transformation and a Pactrun SQLite commit form one
atomic transaction. The Pactrun-owned publication ordering is defined by
[PR-REQ-0246](../architecture/recovery-and-reconciliation.md#pr-req-0246---service-transformation-target-publication-boundary).

If execution is lost while risk remains open, the Run MAY become `Interrupted`
and the Instance MAY enter `ManualRecoveryRequired`. Recovery of the
Pactrun-owned committed boundary MUST NOT be presented as proof that the
service-owned state remains coherent with the source Revision. No inferred
compensation, automatic Hook replay, or service-state rollback is introduced.

The identity encoding, association schema, compatibility expression,
continuity and retention representation, discard representation, target-commit
coordination mechanism, operation-prerequisite representation, user-access
encoding, and persistence ownership remain future design gates. This section
does not classify non-ServiceStorage-backed service-owned resources.

**Verification: Pending automated coverage.**

### PR-REQ-0245 - Explicit compatibility and resource mapping

Pactrun MUST NOT infer ServiceStorage-backed resource compatibility or semantic
continuity from a locator, filename, payload inspection, or coincidental
content. Same resource identity with a Package-declared compatible
representation means in-place continuity over the same Instance live resource.

A locator change, representation or schema change, identity change, split, or
merge MUST use explicit target-owned semantics. An identity spelling change is
an identity change and requires an explicit source-to-target mapping; one-to-
many and many-to-one relationships likewise require explicit mappings and any
necessary Migration Hook transformation. These semantics do not define the
future declaration fields, mapping syntax, compatibility algorithm, or durable
continuity representation.

**Verification: Pending automated coverage.**
