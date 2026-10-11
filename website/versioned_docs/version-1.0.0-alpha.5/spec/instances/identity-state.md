---
title: Instance Identity and State
---

# Instance Identity and State

An Instance has a stable identity and an opaque state version. The state version tracks Pactrun-owned publications, not arbitrary changes to live service data.

A lookup name can be reused after deletion; the object lifetime cannot. Historical
Runs therefore refer to InstanceId rather than transferring to a new Instance
with the same name. [Kubernetes names and UIDs](https://kubernetes.io/docs/concepts/overview/working-with-objects/names/)
provide a related model for separating human lookup from durable object identity.

### PR-REQ-0023 - Stable Instance identity

`InstanceId` MUST be Pactrun-generated, opaque, globally strong, stable, and
non-reusable. Migration, Input mutation, and Snapshot Restore MUST NOT change
it. Recreating a deleted Instance name MUST produce a new `InstanceId`.

**Verification: PR-TEST-0258, PR-TEST-0436, PR-TEST-0591, PR-TEST-0604.**

### PR-REQ-0024 - Instance names and provenance

`InstanceName` MUST be a human reference that is unique among live Instances in
one management environment. Names MAY be reused after deletion, but durable
provenance MUST use `InstanceId` as its authoritative historical referent.
The [name and inspection rules](../core/objects.md#pr-req-0263---instance-name-and-inspection-order)
define exact syntax and ordering.

**Verification: PR-TEST-0531.**

### PR-REQ-0025 - Opaque InstanceStateVersion

Every Instance MUST have one opaque, durable, non-revivable
`InstanceStateVersion` representing a Pactrun-owned authoritative state
publication event. It MUST NOT be an external-service hash. A rollback or
recovery that recreates earlier field values MUST publish a new token to prevent
ABA behavior. Service-owned live bytes are not part of this token.

**Verification: PR-TEST-0259, PR-TEST-0301, PR-TEST-0330, PR-TEST-0346, PR-TEST-0591.**

### PR-REQ-0026 - State-version publication

Any committed Pactrun-authoritative Instance-state change that can affect future
compilation, admission, execution-visible managed context, lifecycle, or
recovery MUST atomically publish one new `InstanceStateVersion`. This includes
active Revision changes, managed binding mutations, Restore commits, Migration
edge commits, ManualRecoveryRequired transitions, and
`ResolveManualRecovery`. It does not make service-owned live bytes part of the
same publication.

**Verification: PR-TEST-0263, PR-TEST-0301, PR-TEST-0330, PR-TEST-0377, PR-TEST-0591.**

### PR-REQ-0027 - Non-versioned observations

Hook-only external side effects, service-created or service-modified Managed
Service Resource bytes within ServiceStorage, Snapshot object creation, Run and
Artifact creation, and presentation-only metadata MUST NOT independently change
an Instance's state version. Pactrun MUST NOT claim that
`InstanceStateVersion` linearizes mutations that Pactrun does not
authoritatively own or observe.

**Verification: PR-TEST-0246, PR-TEST-0346, PR-TEST-0558, PR-TEST-0560, PR-TEST-0602.**
