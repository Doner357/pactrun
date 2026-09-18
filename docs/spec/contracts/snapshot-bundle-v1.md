---
title: Snapshot Bundle V1
---

# Snapshot Bundle V1

**Status: Normative transport contract. Adapter, application services and human
CLI are implemented, verified and integrated into develop with M4.**

<!-- spec-navigation:start -->
## Reading map (informative)

Read the transport envelope and validation boundary independently of Snapshot semantic identity. Follow the separate integrity contract for exact identity.

Start with the [specification map](../index.md)
and [shared vocabulary](../glossary.md) if a term is unfamiliar.
Check [implementation status and remaining decisions](../../development/next-milestone.md)
before treating an approved contract as available runtime behavior.
The original status, rules, exceptions, and verification declarations below retain their meaning.
<!-- spec-navigation:end -->

### PR-REQ-0292 - Closed Stored-ZIP transport profile

SnapshotBundleV1 MUST be one closed Stored-ZIP archive. Only method Stored is
supported; ZIP64 is permitted when needed. Encryption, compression, split or
multi-volume archives, self-extracting prefixes, duplicate entries, illegal
paths, symbolic links, special files, missing members, and extra members MUST
be rejected. A parser MUST validate record bounds and header/directory
consistency with checked arithmetic before allocating from hostile counts.
CRC validation MUST NOT substitute for Snapshot cryptographic verification.

Exactly these regular-file member names are allowed:

```text
bundle.json
manifest.json
blobs/sha256/<64 lowercase hexadecimal characters>
```

bundle.json MUST have exactly these fields:

```text
kind: "pactrun_snapshot_bundle"
bundle_version: 1
integrity_format: 1 | 2
integrity_digest: "sha256:<64 lowercase hexadecimal characters>"
```

The envelope is dispatch metadata, not Snapshot authority. Its version and
digest MUST agree with manifest.json under the selected V1/V2 verifier. The
manifest and exact referenced payload closure determine Snapshot authority.
Each distinct referenced digest MUST have one member, including empty bytes;
missing or unreferenced blob members MUST fail. Producer RevisionCore, Hooks,
runtime content, installation state, and presentation metadata MUST NOT occur.

Archive bytes need not be canonical. Member ordering, ZIP timestamps,
attributes, and other container metadata MUST NOT affect Snapshot identity or
be applied to the host. Export writes the version-specific canonical manifest;
import MUST perform that version's strict validation and normalization.

Import MUST acquire input into private operation staging, perform bounded
container/manifest/full-content verification, and publish the Snapshot in one
transaction. It MUST NOT extract arbitrary archive paths, launch a Hook,
create a Run, require a target Instance, or install a producer. Missing producer
semantics are reported as not_evaluated. Same SnapshotId/digest is idempotent
only after full input validation; same ID/different digest is a collision.

Every complete export MUST require the one-shot --authorize-sensitive-export
authorization before sensitive payload acquisition. No Secret descriptor does
not imply safe service content. Warn that the bundle is unencrypted and may
contain sensitive data. Authorization grants neither declassification,
overwrite, trust, nor reusable consent. Export MUST privately stage and then
atomically publish without replacing an existing destination. Sensitive
temporary output MUST have restrictive host permissions from creation.

M4 accepts explicit filesystem paths only, not '-', stdin/stdout, force,
overwrite, or pipe retry/resume. Failed pre-publication work leaves no partial
authoritative Snapshot or final output name. Cleanup is best-effort and makes
no secure-erasure claim. Actual host resource failures remain distinct from
the [structural and representation limits](../behavior/m4-runtime-capabilities.md).

**Verification: PR-TEST-0206, PR-TEST-0207, PR-TEST-0208, PR-TEST-0209, PR-TEST-0210, PR-TEST-0211, PR-TEST-0212, PR-TEST-0213, PR-TEST-0214, PR-TEST-0215, PR-TEST-0216, PR-TEST-0217, PR-TEST-0218, PR-TEST-0219, PR-TEST-0276, PR-TEST-0479, PR-TEST-0480, PR-TEST-0481.**

Implementation note: bounded preflight checks the original local/central
records and exact archive extents before the ZIP adapter allocates metadata.
The adapter then receives a read-only directory projection constructed from
those verified entries, with unchanged original payload ranges. This prevents
a second footer/extra-field interpretation from introducing unbounded entry
counts. The original staged archive is not rewritten and no invalid input is
repaired. The projection is not Snapshot authority or a canonical archive rule.
The writer preserves I/O failure while preventing its dependency's Drop path
from logging raw errors or turning failed output into success. Snapshot human
command dispatch remains S7 work.
