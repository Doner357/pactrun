---
title: Input and Resource Declarations
---

# Input and Resource Declarations

## Input declarations

### PR-REQ-0127 - Stable InputIdentity

Every Input declaration MUST have a stable semantic `InputIdentity`. Within one
Package lineage, an author MUST NOT reuse an identity for a different meaning.
The [Pack source format](../packages/source-format.md) defines its authoring spelling and
uses the ASCII semantic-identity grammar of the Revision format.

**Verification: PR-TEST-0039, PR-TEST-0069.**

### PR-REQ-0128 - Input payload opacity

Authors MUST treat Pactrun-managed Input payloads as opaque bytes. Pactrun MUST
preserve zero-length and arbitrary payloads without parsing, converting, or
inferring a type from file extensions. The payload is a detached,
Pactrun-authoritative binding; this contract does not turn a service-owned live
file into an Input or authorize implicit synchronization with one.

**Verification: PR-TEST-0075, PR-TEST-0147, PR-TEST-0346, PR-TEST-0590.**

### PR-REQ-0129 - Required declaration meaning

An active Input marked required MUST mean that an ordinary Action or Snapshot
Capture needs a binding in its Instance context. It MUST NOT mean that an
Instance without the binding is illegal, and it MUST NOT automatically become a
Migration or Cleanup requirement.

**Verification: PR-TEST-0225, PR-TEST-0231, PR-TEST-0321, PR-TEST-0424, PR-TEST-0590.**

### PR-REQ-0130 - Active and retained contexts

Ordinary Actions MUST receive active bindings only. Migration and Cleanup MAY
use typed references to active and retained bindings under their own
requirements. Authors SHOULD treat dependence on retained data as a visible
compatibility or recovery smell, not as a prohibited capability.

**Verification: PR-TEST-0249, PR-TEST-0322, PR-TEST-0424, PR-TEST-0603.**

## ServiceStorage-backed service state is not an Input

A Package MUST distinguish an Input from a ServiceStorage-backed Managed
Service Resource. An Input is a Pactrun-authoritative persistent binding. A
ServiceStorage-backed Managed Service Resource is attached live state whose
contents belong to the service, even when Pactrun identifies or exposes the
resource. A persistent configuration file is not automatically an Input merely
because a user needs to inspect or edit it.

For example, a live `management.json` file may remain absent until the service
creates it, then be edited in place by the service or user. Pactrun MUST NOT
model this as Input-to-file materialization with implicit two-way synchronization.
The [resource ownership rules](./service-resources.md) define this distinction.
The [Revision format](../packages/revision-format.md) and
[Pack source format](../packages/source-format.md) define its declaration.
Other resource kinds are outside the ServiceStorage-backed model.

### PR-REQ-0243 - Resource exposure and mutation route

A ServiceStorage-backed Managed Service Resource exposure contract MUST keep
read exposure separate from its user mutation route. Read exposure is
conceptually hidden or readable. User mutation is conceptually unavailable,
direct, or mediated by a Pack operation. The
[Revision format](../packages/revision-format.md) specifies their representation.

Direct mutation MUST mean only that the user is authorized to modify the same
service-authoritative live state. It MUST NOT imply that the service can safely
consume the change while running or that Pactrun provides quiescence, atomicity,
conflict detection, rollback, reload, restart, Snapshot creation, continuous
observation, or `InstanceStateVersion` advancement. When safe mutation requires
service-specific validation, quiescence, reload, or restart, the Package MUST
own that behavior through an operation and its Hook rather than relying on the
direct-exposure contract.

**Verification: PR-TEST-0357, PR-TEST-0359.**

## Secrets

### PR-REQ-0131 - Secret is protection metadata

`Secret` MUST be Pactrun-owned protection metadata rather than a payload type.
Authors and Hooks MUST NOT rely on Pactrun parsing the payload, providing a
vault-grade guarantee, or securely erasing discarded bytes.

**Verification: PR-TEST-0076, PR-TEST-0147, PR-TEST-0346.**

### PR-REQ-0132 - No implicit declassification

An authoring model MUST NOT express an implicit Secret-to-Normal transition.
Migration must use explicit `Declassify`, and execution additionally requires
operator authorization. A Hook MUST NOT declassify output by choosing its own
protection metadata.

**Verification: PR-TEST-0281, PR-TEST-0311, PR-TEST-0313.**
