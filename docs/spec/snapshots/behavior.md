---
title: Snapshot Capture and Restore
---

# Snapshot Capture and Restore

## Snapshots

A Snapshot is an immutable recovery object produced under one exact Revision.
It contains the complete managed binding state, including active and retained
bindings, optional absence, protection metadata, and Package-produced service
recovery content. It is not an Instance clone and does not contain Run history
or old binding history. Pactrun does not automatically scan `ServiceStorage` or
mirror every ServiceStorage-backed Managed Service Resource into a Snapshot. The
Capture Hook selects and transforms recovery-relevant service-owned state at an
appropriate service consistency boundary before Pactrun validates, hashes, and
commits it.

### PR-REQ-0099 - Snapshot operations

Pactrun MUST provide management and execution operations to create, list, show,
restore, export, import, and delete Snapshots. Import and export MUST NOT launch
a Hook, compile a workflow, or create a Run. Snapshot import MUST NOT require a
target Instance.

**Verification: PR-TEST-0268.**

### PR-REQ-0100 - Capture consistency

Capture MUST use one admitted binding view for both the Capture Hook context and
the Snapshot's managed binding state. It MUST include retained bindings even
though they are not visible to the ordinary Hook context.

**Verification: PR-TEST-0246, PR-TEST-0249.**

### PR-REQ-0101 - Direct Restore compatibility

Restore MUST require the Snapshot producer `RevisionIdentity` to equal the
target Instance's active `RevisionIdentity`, including both Package ID and
Revision content digest. Successful import MUST NOT imply Restore compatibility.

**Verification: PR-TEST-0274.**

### PR-REQ-0102 - Restore replacement

Restore MUST preserve the target Instance identity and name. It MUST stage the
Snapshot's complete managed binding state, give the Restore Hook the same active
view that will be committed, and replace rather than merge the target's earlier
bindings after success.

**Verification: PR-TEST-0258, PR-TEST-0268.**

### PR-REQ-0103 - Snapshot provenance and lifetime

Origin Instance information MUST be provenance rather than ownership. A
Snapshot MAY restore to another exact-compatible Instance and MAY outlive its
origin Instance, producer Revision installation, and creator Run.

**Verification: PR-TEST-0275, PR-TEST-0421.**

### PR-REQ-0104 - Sensitive Snapshot export

Capture MUST include Secrets automatically as managed recovery state. Exporting
a Snapshot containing Secrets MUST require explicit sensitive-data
authorization and clear handling warnings. Ordinary Snapshot inspection MUST
NOT reveal Secret values or value-derived digests.

**Verification: PR-TEST-0268.**
