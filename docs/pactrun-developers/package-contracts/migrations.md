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
state, disposition, and a new Instance state version.

**Verification: Pending automated coverage.**

### PR-REQ-0156 - Whole-path preflight

Before the first side effect, the Compiler SHOULD symbolically preflight the
complete selected path for statically detectable missing requirements, illegal
Secret transitions, target-writer conflicts, and unavailable resources.

**Verification: Pending automated coverage.**

## Input transitions

### PR-REQ-0157 - Canonical transition set

Normalized Migration semantics MUST use explicit `Carry(source -> target)`,
`Declassify(source -> target)`, `Keep(source)`, and `Discard(source)`
transitions. `Discard` is always explicit. Identity rename or change requires
an explicit mapping.

**Verification: Pending automated coverage.**

### PR-REQ-0158 - Conservative shorthand

An authoring frontend MAY default the same stable Input identity to `Carry` and
a source-only Input to `Keep`, but normalization MUST materialize those choices.
Existing unrelated retained bindings persist without repeated `Keep`
declarations unless explicitly consumed, reactivated, or discarded.

**Verification: Pending automated coverage.**

### PR-REQ-0159 - Declarative transition scope

Declarative transitions MUST express binding continuity and disposition only.
They MUST NOT perform one-to-many, many-to-one, or arbitrary payload
transformation. Opaque transformation belongs to a typed Migration Hook output.

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
sufficient.

**Verification: Pending automated coverage.**
