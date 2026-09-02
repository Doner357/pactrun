---
title: Inputs, Secrets, and Readiness
---

# Inputs, Secrets, and Readiness

**Status: Normative product behavior specification except where linked to an
owning requirement.**

## Persistent and execution data scopes

Pactrun keeps ownership and lifetime explicit:

| Concept | Lifetime | Scope | Authority and example |
| --- | --- | --- | --- |
| Revision Runtime Content | Immutable | Revision | Pactrun-owned installed executable and support content |
| Managed Input Binding | Persistent | Instance | Pactrun-authoritative detached domain, password, or configuration payload |
| `ServiceStorage` / ServiceStorage-backed Managed Service Resource | Persistent | Instance | Pactrun-provided storage lifetime with service-authoritative live contents |
| Workspace | One execution | Execution | Hook-writable temporary scratch |
| Run Artifact | Retained by policy | Run | Pactrun-managed committed Action output |
| Snapshot | Durable immutable object | Snapshot | Validated recovery state, not a live-state mirror |

The first two persistent Instance-data models are not interchangeable. A
Managed Input Binding is an opaque detached value whose presence, replacement,
retention, and Secret protection are managed by Pactrun. A ServiceStorage-backed
Managed Service Resource is live state that the service may create or mutate
directly. Pactrun may identify and expose its contract without owning,
versioning, linearizing, content-addressing, or automatically Snapshotting the
live bytes.

Visibility does not decide ownership: a Pactrun-visible persistent file is not
therefore required to be an Input, and an Input MUST NOT be kept synchronized
implicitly with a service-owned file.

## Three value-delivery scopes

Pactrun also keeps three value-delivery concepts separate:

| Concept | Lifetime | Scope | Example |
| --- | --- | --- | --- |
| Instance Input or Secret | Persistent | Instance | Domain, password, detached configuration payload |
| Invocation Parameter | One invocation | Action or Snapshot operation | Tail count, follow mode, one-time credential |
| Managed Input | One managed workflow | Workflow | Snapshot payload or Migration content |

They are not interchangeable generic maps.

## Informative live-file example

A NetBird Revision may declare a future Managed Service File with semantic
identity `management_config`, associated with storage `netbird_state` and path
`management.json`. The declaration may exist while the file is absent. The
service may later create and continuously use the same file, and a user may
need to inspect or edit that live state. A compatible Revision transition or
container recreation continues to use that one persistent resource without
copying or rematerializing it.

This case is not modeled as `Input -> materialize -> synchronize`. Formal
identity encoding, authoring, authority wire shape, access encoding,
compatibility representation, retention persistence, and durable representation
remain future-version design gates. The example does not classify non-
ServiceStorage-backed resources.

## PR-REQ-0090 - Ordinary execution context

An ordinary Action or Snapshot Capture MUST receive the active Revision's
current active Input and Secret bindings as one Instance context. Retained
bindings MUST NOT appear in ordinary execution context. Migration and Cleanup
MAY receive typed active and retained contexts defined by their own contracts.

PersistenceSchemaV3 stores only the one binding registry; these roles and any
future execution context are derived from the strict active Revision.

**Verification: Pending automated coverage.**

## PR-REQ-0091 - Readiness admission

Ordinary Actions and Snapshot Capture MUST require
`RequiredInputsSatisfied == true` before launching a Hook. Missing bindings MUST
produce actionable diagnostics. Migration and Cleanup MUST instead use their
operation-specific requirements, and Restore MUST validate the staged Snapshot
state rather than target pre-restore completeness.

M2 creation MAY commit this incomplete state and inspection MUST derive missing
requirements without persisting a readiness flag, as specified by PR-REQ-0265.

**Verification: Pending automated coverage.**

## Managing active and retained bindings

The normative mutation rules are
[PR-REQ-0033](../architecture/identity-and-state.md#pr-req-0033---binding-mutation-invariants).
In user terms:

- a required Input can be absent initially, but cannot be ordinarily deleted
  after it is bound under the active Revision;
- optional active Inputs can be set, replaced, or removed;
- retained Inputs can be inspected, exported, or deleted, but not directly set
  or replaced;
- Input mutations are management operations and do not create Runs.

## M2 persistent payload and operation boundaries

### PR-REQ-0264 - Managed Input payload and binding registry

`ManagedInputPayloadId` MUST be an opaque, instance-scoped 128-bit value from
the operating-system cryptographic random source. It MUST NOT be derived from
payload bytes, reused for a replacement, or used as a content digest. A
`ManagedInputPayload` is immutable, opaque, non-deduplicated, and MAY be empty.
Its protection is exactly `Normal | Secret`. M2 supports payloads through
536,870,912 bytes inclusive.

The protection stored on the immutable payload is the binding's persistent
sticky floor. For an active binding, effective protection is the stronger of
that floor and the strict-decoded active declaration. For a retained binding,
the stored payload protection alone is authoritative because no active
declaration exists. Active Normal with a Secret payload is a valid sticky state;
active Secret with a Normal payload is not a valid committed state.

Each `(InstanceId, InputIdentity)` has at most one binding in one registry.
Active and retained are roles derived from the strict-decoded active Revision,
not separate tables or payload histories. Requiredness and readiness are also
derived and MUST NOT be persisted. A missing binding is represented by absence,
not an empty payload or tombstone. Persistent representation uses the header
and one-megabyte chunks in PR-REQ-0269 and MUST be validated as one exact byte
sequence on load.

A replacement MUST create a fresh payload even when bytes equal the current
payload. The old payload remains available while reachable by a current
binding, an acquired operation-local export observation, or a future durable
execution or recovery reference. Reclamation MUST NOT invalidate any such
reference and MUST NOT infer reachability from payload equality.

**Verification: PR-TEST-0075.**

### PR-REQ-0265 - Instance creation and transient payload staging

Initial Input acquisition MUST accept only explicit file sources and at most
one stdin source. `SetInput` MUST likewise accept exactly one explicit file or
stdin source. Creation MUST reject duplicate Input identities and multiple
stdin declarations before acquisition. Every path MUST preserve exact bytes
without text conversion, validate the identity against the active Revision,
and finish acquisition before beginning its SQLite publication transaction.
Values supplied through argv literals, environment variables, prompts, or
networks are outside M2.

M2 MUST stage the complete bytes in regular files while using only bounded
streaming memory. Whole-payload memory staging is not the supported model.
Every operation file MUST live below a random per-process session directory in
the dedicated caller-provisioned `<pactrun-storage-root>/staging/` child. That
child uses the same trusted-root, regular-directory, no-reparse, and supported-
filesystem assumptions as the database and runtime-content roots: local fixed
NTFS on Windows and the established ext4, XFS, Btrfs, or ZFS profiles on Linux.
Pactrun MUST NOT place this staging in ambient `%TEMP%`, `TMPDIR`, a system
temporary directory, the current directory, or the source tree.

Session and operation names MUST use random non-Domain nonces and MUST NOT
contain payload digests, Input identities, or Secret-derived material. POSIX
files MUST be owner-only; Windows relies on the dedicated root ACL and
restricted sharing. The process MUST hold an exclusive operating-system session
lease. Normal completion removes the operation file and normal shutdown removes
the session. A crash releases the lease but MAY leave residue. Before accepting
new operations, startup MUST inspect staging sessions and may delete only a
session for which it acquires a non-blocking exclusive lease. It MUST NOT delete
a session still leased by another process. Cleanup failure MUST be diagnosed,
and a stale path MUST NOT be reused or interpreted as a committed payload,
retry record, pin, or recovery state.

File or stdin bytes MUST stream into one opened staging object while Pactrun
counts them. Acquisition MUST fail immediately once the count exceeds
536,870,912 bytes. After completion Pactrun MUST rewind and read the same object
rather than re-resolving a path. Payload publication streams fixed one-megabyte
chunks into SQLite and MUST NOT require one unbounded bind buffer.

One transaction MUST publish the new Instance, every initial payload header and
chunk, every binding, and one fresh `InstanceStateVersion`. Acquisition,
validation, name conflict, constraint, or commit failure MUST leave none of
those persistent objects. Missing required Inputs remain legal and derived as
not ready. Operation staging is deleted after commit or rollback and required
response processing.

A set operation follows the same acquire-before-transaction and staging
lifetime rule, then publishes its fresh payload, binding switch, and state token
in one transaction. Failure leaves its staging object transient and the old
persistent binding unchanged.

Transient staging is not a persistent `ManagedInputPayload`, Workspace,
ServiceStorage, Managed Service Resource, M3 durable pin, Run reference, GC
root, M6 recovery state, or cross-crash retry log.

**Verification: PR-TEST-0074, PR-TEST-0079.**

### PR-REQ-0266 - Managed Input mutation, export, and reclamation

Input `set` MUST bind an absent active Input or replace a present active Input.
It MUST reject set or replace for a retained Input. Active required bindings,
once present, MUST reject ordinary deletion. Active optional and retained
bindings MAY be deleted. Deleting an absent active Input with a matching state
version is an idempotent no-op. Accepted bind or replacement MUST publish a new
random payload and fresh Instance state version even for equal bytes. Every
operation MUST revalidate active Revision, role, requiredness, protection, and
readiness in its write transaction; failure preserves the old binding, payload
pointer, and state version.

An initial or absent-input bind MUST set the new payload's stored protection to
the active declaration. A replacement MUST set it to the stronger of the old
payload's stored protection and the active declaration; ordinary set therefore
cannot downgrade a sticky Secret binding. Because payload headers are
immutable, a promotion MUST publish a fresh Secret payload and atomically
switch the binding rather than rewrite the existing rank.

Only deletion permitted by PR-REQ-0033 terminates the old binding continuity.
Active required bindings remain non-deletable once bound. After an allowed
active-optional or retained deletion, a later bind is newly acquired and takes
its initial protection from the then-active declaration. Neither the deletion
nor a later bind declassifies the old payload or implies secure erasure.

`ExportInput` is Observe. In one short SQLite read snapshot it MUST resolve the
exact Instance, observed state version, role, effective protection, and payload
identity; authorize Secret disclosure before acquiring bytes; and stream the
exact ordered chunks into a private operation staging file before ending that
read snapshot. Output MUST then read only the staging file and MUST NOT re-query
the binding. A concurrent set or delete MAY commit without waiting for export and
MUST NOT switch, truncate, or remove the already selected bytes. Until staging
is complete, the snapshot or equivalent transient observation prevents
application reclamation of the selected payload. After staging, private bytes
carry the operation and otherwise unreachable database rows MAY be reclaimed.
A crash aborts export and a retry selects current state anew. This observation
is not a durable pin or recovery state.

For `--output <host-path>`, Pactrun MUST finish internal staging before opening
the caller-selected destination parent. The final name MUST be absent: a file,
directory, symlink, or reparse object at that name is a no-clobber failure.
Pactrun MUST create a random sibling temporary regular file in the same parent,
stream exact bytes, synchronize that file, and publish with a platform atomic
no-replace rename or link primitive. It MUST then synchronize the parent where
the supported platform profile provides that operation. A platform or
filesystem without same-directory atomic no-replace publication MUST fail
before final-name publication.

Before atomic publication the final name remains absent; afterward it denotes
only the complete payload. A crash MAY leave an unpublished sibling temporary
file, which the current failure path SHOULD remove best-effort, but Pactrun MUST
NOT scan or delete arbitrary caller directories. A crash after publication but
before response leaves the complete final file; retry fails no-clobber and MUST
NOT overwrite it. M2 has no force or overwrite option.

For `--output -`, authorization and payload observation MUST finish before the
first byte. Stdout carries only raw payload bytes and diagnostics use stderr.
The stream is non-rollbackable: downstream close, I/O failure, or crash MAY
leave a prefix and MUST result in failure. Pactrun MUST NOT claim atomic stdout,
replay, or resume.

**Verification: PR-TEST-0076, PR-TEST-0077, PR-TEST-0078.**

### PR-REQ-0268 - M2 Secret storage and disclosure boundary

Secret protection MUST use the sticky floor in PR-REQ-0034 and MUST NOT be
chosen by an arbitrary input-management flag. The exact rule is:

```text
StoredProtection(binding) = referenced immutable payload.protection
EffectiveProtection(active binding) =
    max(StoredProtection, ActiveDeclarationProtection)
EffectiveProtection(retained binding) = StoredProtection
```

A committed active Secret binding MUST reference a Secret payload. Active
Normal with a Secret payload is valid and effectively Secret. A retained
binding has no active declaration and MUST preserve the payload's stored
protection. Future Migration target publication MUST satisfy this persisted
invariant, but M2 does not define or execute Migration transformation or
declassification.

Ordinary views, inspection, diagnostics, and human output MUST apply effective
protection and MUST NOT reveal Secret bytes, a preview, digest, length, payload
identifier, or another payload-derived value. Raw export requires the purpose-
specific authorization in PR-REQ-0092 before bytes are acquired, including
when an active Normal declaration references a sticky Secret payload. That
authorization permits disclosure only: it does not select a destination,
decode text, declassify, or authorize any other sensitive operation.

M2 provides no cryptographic encryption at rest, vault or key management, or
secure erasure guarantee. Database pages, WAL/journal files, backups, internal
staging files, and authorized file-export temporary files MAY contain plaintext
Secret bytes. POSIX modes, Windows root ACLs, restricted sharing, and dedicated
storage are host protection assumptions rather than Pactrun cryptographic
guarantees. Removing a binding, payload row, staging file, or output temporary
name MUST NOT be described as physical secure erasure.

**Verification: PR-TEST-0074, PR-TEST-0076, PR-TEST-0078.**

### PR-REQ-0092 - Explicit Secret export

Export of a Secret Input MUST require an explicit sensitive-data operation and
authorization. A generic confirmation option MUST NOT implicitly authorize
Secret disclosure, Snapshot Secret export, or Secret declassification.

The fixed M2 spelling is `--authorize-secret-export`; its acquisition and
destination boundaries are defined by PR-REQ-0266 and PR-REQ-0268.

**Verification: PR-TEST-0078.**

## PR-REQ-0093 - Secret deletion disclaimer

Deleting or discarding a Secret MUST remove its managed reference according to
the operation contract, but Pactrun MUST NOT claim physical secure erasure of
underlying storage.

This includes plaintext database, WAL/journal, backup, internal staging, and
authorized export temporary bytes under the M2 negative guarantee in
PR-REQ-0268.

**Verification: Pending automated coverage.**

## UTF-8 and opaque Input data

Input payloads are opaque bytes and may contain empty, binary, or UTF-8 data.
Pactrun preserves them without interpreting their language or file extension.
Textual Package metadata, parameters, and output interfaces that specify UTF-8
must accept full Unicode, including symbols and emoji.
