---
title: Revision Transitions
---

# Revision Transitions

## Revision Migration

### PR-REQ-0105 - Declared Migration path

Migration MUST follow exact inbound edges declared by target Revisions within
the same Package lineage. Pactrun MUST NOT invent an undeclared direct edge.

**Verification: PR-TEST-0277, PR-TEST-0278, PR-TEST-0290.**

### PR-REQ-0106 - Chained progress

When a path contains multiple edges, each edge MUST be an independent durable
commit boundary. If a later edge fails, the Instance MUST remain at the last
successfully committed intermediate Revision.

**Verification: PR-TEST-0303, PR-TEST-0304, PR-TEST-0317.**

### PR-REQ-0107 - Incomplete Migration result

A successful Migration MAY leave the target Instance incomplete. This MUST be
reported as configuration readiness, not failure or manual recovery. A chain
MAY continue through an incomplete intermediate Revision when the next edge's
own requirements are satisfied.

**Verification: PR-TEST-0301, PR-TEST-0321, PR-TEST-0604.**

### PR-REQ-0108 - Declassification authorization

A Secret-to-Normal transition MUST be explicitly declared by the Migration edge
and explicitly authorized by the operator for that Migration. Users MAY instead
retain or discard the old Secret and provide a new Normal binding.

**Verification: PR-TEST-0280, PR-TEST-0281, PR-TEST-0311.**

### ServiceStorage-backed resource continuity

The existing `Carry`, `Keep`, `Discard`, and `Declassify` model applies to
Pactrun-authoritative Managed Input Bindings. It is not a Service Resource
transition schema. This section does not classify Docker volumes, external
databases, remote objects, or other non-ServiceStorage-backed service state.

Compatible source and target Revisions use the same Instance persistent live
resource without copying or rematerializing its bytes. A source-only resource
is retained conservatively, and destructive removal must be explicit. If a path
or representation changes, a resource splits or merges, or another
service-specific transformation is required, target-owned inbound Migration
semantics and a Migration Hook own the transformation and use the existing
recovery-risk handshake.

This policy defines resource semantics. The concrete association, compatibility,
continuity, access, prerequisite and target-publication mechanisms are owned by
[ServiceStorage execution](../instances/service-storage.md) and
the [Persistence baseline](../persistence/persistence-baseline.md).
[Instance retirement](../lifecycle/retirement.md) owns detached
custody and explicit discard. These owners do not imply a taxonomy for resources
outside ServiceStorage.
