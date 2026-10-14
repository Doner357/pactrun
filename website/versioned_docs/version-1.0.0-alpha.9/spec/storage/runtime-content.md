---
title: Immutable Runtime Blob Storage
---

# Immutable Runtime Blob Storage

Runtime blobs store immutable bytes by digest. Semantic content roles belong to Revision descriptors, not to physical blob names or storage paths.

### PR-REQ-0228 - Immutable runtime-content blob-store boundary

The production runtime-content store MUST remain crate-private and store opaque
bytes addressed only by their expected SHA-256 blob digest. Its flat physical
namespace, staging names, publication lock, and filesystem paths are local
implementation details and MUST NOT enter `RuntimeContentClosureIdentityV1`,
Revision identity, canonical bytes, or `RevisionContentDigest`. `ContentId`,
runtime path, executable role, and other semantic descriptor fields MUST NOT
participate in physical blob addressing.

Semantic runtime-content membership and physical blob availability are
separate validations. Once Pactrun publishes a correct blob under a digest,
Pactrun MUST NOT overwrite, mutate, replace, truncate, or repair it in place.
This immutability is a store-protocol and ownership invariant, not a filesystem
read-only-bit or ACL guarantee.

**Verification: PR-TEST-0047, PR-TEST-0048, PR-TEST-0049, PR-TEST-0051.**

### PR-REQ-0229 - Verified durable blob publication

Runtime-content publication MUST stream bytes into a create-new staging file in
the already-existing final directory, calculate SHA-256 and a checked `u64`
length, persist the staging file, and reject an incoming digest mismatch before
publication. The final publication operation MUST be no-replace. A correct
existing regular file is idempotent success only after Pactrun fully verifies
its digest and completes the supported platform persistence barriers; a corrupt
or unacceptable existing entry MUST be rejected and MUST NOT be repaired.

Publication success MUST be returned only after every documented persistence
primitive required by the supported platform profile has succeeded. On Windows
the supported profile is a local fixed NTFS volume and publication uses the
same open write-through staging handle for no-replace handle-based rename. On
supported Linux local filesystems, publication uses no-replace rename, file
synchronization, and root-directory synchronization. The supported Linux
filesystems are ext4, XFS, Btrfs, and ZFS. Other Windows and Linux
filesystem profiles MUST be rejected unless their persistence protocol is
separately established. The dedicated store root MUST already exist. Blob publication does not
durably create that root or its parent hierarchy.

The final namespace MUST contain either no digest entry or complete immutable
bytes. A failure after final publication MAY leave a correct unreferenced blob,
but MUST NOT return the successful value that authorizes later durable reference
publication. A later identical put MUST be able to verify that object, complete
the persistence barriers, and return success. Durability is relative to
successful documented operating-system, filesystem, virtualization, and
storage persistence contracts; process-crash tests are not hardware power-loss
certification.

**Verification: PR-TEST-0047, PR-TEST-0048, PR-TEST-0049, PR-TEST-0050.**

### PR-REQ-0230 - Physical runtime-content availability

Physical availability validation MUST deduplicate a
`RuntimeContentClosureIdentityV1` by blob digest and fully verify each distinct
blob once. Multiple content identities, runtime paths, and executable roles MAY
reference one physical blob. Availability validation MUST NOT change semantic
closure bytes, Revision Core bytes, or `RevisionContentDigest`.

A verified blob means that the opened regular-file object was fully hashed at
acquisition under a dedicated trusted-root and cooperative-writer model. This
does not provide hostile post-verification tamper resistance or an operating
system sandbox. Durable blob publication MUST precede any database reference
publication. Availability verification does not decide deletion, collection,
reachability, pins, or database-reference ownership; those responsibilities
belong to the [object lifecycle](../lifecycle/objects-gc.md) and
[persistence](../persistence/persistence-baseline.md) contracts.

**Verification: PR-TEST-0048, PR-TEST-0051.**
