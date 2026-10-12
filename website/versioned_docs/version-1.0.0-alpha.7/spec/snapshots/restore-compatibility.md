---
title: Operation-scoped Restore
---

# Operation-scoped Restore

This page applies to the specified [Revision `1.0-alpha.2`](../packages/operation-format.md).
Snapshot integrity and bundle contracts remain unchanged.

### PR-REQ-0384 - Exact Restore source selection

Restore MUST require exact equality between the target RevisionIdentity and the
Snapshot producer RevisionIdentity. This includes Package identity and content
digest. Operation-scoped Input requirements MUST NOT expand Snapshot acceptance.
No compatible-revision declaration, Migration path, local name or metadata
version authorizes cross-Revision Restore. The selected Revision MUST declare
an executable Restore Hook and satisfy ordinary execution trust and policy.

**Verification: PR-TEST-0724, PR-TEST-0731.**

### PR-REQ-0385 - Restore binding requirements without conversion

Pactrun MUST validate the immutable Snapshot against its exact producer. It
MUST preserve the complete recorded binding state, including active/retained
roles, absence, payload bytes and protection. Existing protection and
continuity checks remain applicable. Restore MUST NOT rename, convert or
merge Inputs, infer a Migration, or rewrite producer identity.

The staged Snapshot view MUST satisfy the Restore operation's required Inputs.
Unmet requirements of unrelated operations MUST NOT block Restore. Current
Instance values MUST NOT fill Snapshot absences. Operation removal semantics
follow [Input authority](../operations/input-authority.md).

**Verification: PR-TEST-0716, PR-TEST-0724, PR-TEST-0731.**

### PR-REQ-0386 - Operation-scoped Restore lifecycle and publication

Existing-Instance Restore and create-and-restore MUST apply the same exact
source selection and staged requirements before Hook launch. Create-and-restore
MUST also qualify before publishing the new Instance under its existing contract.

The Hook receives only its declared present Input read grants and the selected
Snapshot's read-only service content authority. Filtering Hook-visible Inputs
MUST NOT filter the complete state used for Restore publication. Successful
Restore replaces the complete managed binding state with fresh target-scoped
payload identities while preserving target Instance identity, name and Revision.
Snapshot identity, producer, origin, integrity and payloads remain immutable.

Existing state-version checks, immutable closure pins, exclusivity, atomic
publication and recovery rules remain applicable. A conflict MUST NOT refresh
the plan, replay the Hook or publish partial bindings. Failure, cancellation,
timeout and owner loss MUST NOT clear a pre-existing recovery guard. External
service effects do not prove managed publication succeeded.

**Verification: PR-TEST-0724, PR-TEST-0731, PR-TEST-0732, PR-TEST-0734.**
