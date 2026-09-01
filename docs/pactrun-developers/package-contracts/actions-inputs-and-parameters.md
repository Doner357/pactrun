---
title: Actions, Inputs, and Parameters
---

# Actions, Inputs, and Parameters

**Status: Normative Package contract specification.**

## Actions

### PR-REQ-0125 - Action semantics

An Action MUST represent a Package-defined user operation. Pactrun MUST treat
its stable identity, parameter schema, execution requirements, I/O contract,
access requirement, implementation, and declared managed outputs as semantics;
it MUST NOT infer service behavior from the Action's name.

**Verification: Pending automated coverage.**

### PR-REQ-0126 - Action is not Hook

An Action MUST remain the user operation and a Hook MUST remain one possible
implementation mechanism. Snapshot, Migration, and Cleanup MUST NOT be authored
as Actions even when they ultimately launch Hooks.

**Verification: Pending automated coverage.**

## Input declarations

### PR-REQ-0127 - Stable InputIdentity

Every Input declaration MUST have a stable semantic `InputIdentity`. Within one
Package lineage, an author MUST NOT reuse an identity for a different meaning.
Candidate `PackSourceYamlV1` supplies the fixed M2 authoring spelling while
reusing the Frozen ASCII semantic grammar. Other future authoring frontends
remain independently versioned.

**Verification: Pending automated coverage.**

### PR-REQ-0128 - Input payload opacity

Authors MUST treat Pactrun-managed Input payloads as opaque bytes. Pactrun MUST
preserve zero-length and arbitrary payloads without parsing, converting, or
inferring a type from file extensions. The payload is a detached,
Pactrun-authoritative binding; this contract does not turn a service-owned live
file into an Input or authorize implicit synchronization with one.

**Verification: Pending automated coverage.**

### PR-REQ-0129 - Required declaration meaning

An active Input marked required MUST mean that an ordinary Action or Snapshot
Capture needs a binding in its Instance context. It MUST NOT mean that an
Instance without the binding is illegal, and it MUST NOT automatically become a
Migration or Cleanup requirement.

**Verification: Pending automated coverage.**

### PR-REQ-0130 - Active and retained contexts

Ordinary Actions MUST receive active bindings only. Migration and Cleanup MAY
use typed references to active and retained bindings under their own
requirements. Authors SHOULD treat dependence on retained data as a visible
compatibility or recovery smell, not as a prohibited capability.

**Verification: Pending automated coverage.**

## ServiceStorage-backed service state is not an Input

A Package MUST distinguish an Input from a future ServiceStorage-backed Managed
Service Resource. An Input is a Pactrun-authoritative persistent binding. A
ServiceStorage-backed Managed Service Resource is attached live state whose
authoritative contents belong to the service even when Pactrun identifies or
exposes the resource contractually. A persistent configuration file therefore
is not automatically an Input merely because a user needs to inspect or edit it.

For example, a service's live `management.json` may be declared as a Managed
Service File, remain absent until the service creates it, and then be edited in
place by the service or user. Pactrun MUST NOT model that case as
Input-to-file materialization followed by implicit two-way synchronization.
The accepted ownership invariants are defined by
[PR-REQ-0235](../architecture/identity-and-state.md#pr-req-0235---persistent-instance-data-ownership)
and
[PR-REQ-0236](../architecture/identity-and-state.md#pr-req-0236---managed-service-resource-declaration-and-existence).

The authoring spelling for `ServiceStorage` and its Managed Service Resources is
a future Revision Core format design gate and is not added by this page. This
section does not decide whether another kind of service-owned resource uses the
same abstraction.

### PR-REQ-0243 - Resource exposure and mutation route

A ServiceStorage-backed Managed Service Resource exposure contract MUST keep
read exposure separate from its user mutation route. Read exposure is
conceptually hidden or readable. User mutation is conceptually unavailable,
direct, or mediated by a Pack operation. Representation of those choices
remains future format work.

Direct mutation MUST mean only that the user is authorized to modify the same
service-authoritative live state. It MUST NOT imply that the service can safely
consume the change while running or that Pactrun provides quiescence, atomicity,
conflict detection, rollback, reload, restart, Snapshot creation, continuous
observation, or `InstanceStateVersion` advancement. When safe mutation requires
service-specific validation, quiescence, reload, or restart, the Package MUST
own that behavior through an operation and its Hook rather than relying on the
direct-exposure contract.

**Verification: Pending automated coverage.**

## Secrets

### PR-REQ-0131 - Secret is protection metadata

`Secret` MUST be Pactrun-owned protection metadata rather than a payload type.
Authors and Hooks MUST NOT rely on Pactrun parsing the payload, providing a
vault-grade guarantee, or securely erasing discarded bytes.

**Verification: Pending automated coverage.**

### PR-REQ-0132 - No implicit declassification

An authoring model MUST NOT express an implicit Secret-to-Normal transition.
Migration must use explicit `Declassify`, and execution additionally requires
operator authorization. A Hook MUST NOT declassify output by choosing its own
protection metadata.

**Verification: Pending automated coverage.**

## Invocation Parameters

### PR-REQ-0133 - Invocation-parameter lifecycle

Invocation Parameters MUST be typed, single-invocation values processed through
parse, validation, normalization, and binding. They MUST NOT persist as Instance
Inputs or become Managed Data.

**Verification: Pending automated coverage.**

### PR-REQ-0134 - Initial parameter types

The initial parameter model MAY include integer, float, boolean, and string
types and MAY mark parameters sensitive. Raw argument-vector passthrough MUST
NOT be part of the initial contract.

**Verification: Pending automated coverage.**

### PR-REQ-0135 - Sensitive parameter channel

A sensitive parameter value MUST NOT be recorded in Run history. The CLI MUST
offer a value path that does not expose the value directly in command-line
arguments, such as prompt, file, or standard-input binding.

**Verification: Pending automated coverage.**

### PR-REQ-0136 - Shared machinery is not shared identity

Actions and Snapshot operations MAY reuse parameter parsing and binding
machinery, but this MUST NOT turn them into the same domain operation.

**Verification: Pending automated coverage.**
