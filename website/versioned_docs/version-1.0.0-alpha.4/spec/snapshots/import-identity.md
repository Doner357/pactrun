---
title: Snapshot Import Identity
---

# Snapshot Import Identity

### PR-REQ-0085 - Snapshot import identity

Snapshot export and import MUST preserve `SnapshotId`, integrity format and
digest, producer Revision identity, immutable provenance, complete managed
binding state, and service recovery content. The same Snapshot ID and digest
MUST be idempotent; the same ID with a different digest MUST be rejected as an
identity collision.

**Verification: PR-TEST-0206, PR-TEST-0208, PR-TEST-0209, PR-TEST-0213, PR-TEST-0269.**
