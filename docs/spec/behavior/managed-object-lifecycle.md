---
title: Managed Object Lifecycle and GC
---

# Managed Object Lifecycle and GC

**Status: Implemented normative contract; verification and integration are tracked separately.**

<!-- spec-navigation:start -->
Read [resource lifetimes](../foundations/resources-and-versioning.md) and
[command conventions](./command-and-output-reference.md) first. Consult the
[implementation record](../../development/managed-object-lifecycle-status.md)
before using a command. [V9](../persistence/persistence-baseline.md) is the
coordination boundary; this page does not claim integration or publication.
<!-- spec-navigation:end -->

### PR-REQ-0341 - Explicit object deletion

Deletion MUST check eligibility and commit removal in one transaction with foreign
keys enabled. A missing exact Run, Snapshot or Revision is an idempotent success;
unresolved or ambiguous human Revision references remain errors. Snapshot deletion
MUST reject accepted Restore and recovery references. Revision deletion MUST
reject active Instances, durable pins and unresolved obligations; historical Run
identity and Snapshot provenance MUST NOT retain installation. Installation-local
metadata, aliases, notes and trust are removed; Package identity remains.

A Run MUST be terminal and free of execution, recovery and retirement obligations
before removal. Guards, pins, transition state, deletion obligations, finalization
authorizations and retirement receipts MUST be considered. Artifacts require
`--delete-artifacts`; refusal preserves both history and Artifacts. This flag
does not override evidence retention. Removing an Artifact preserves its Run and
other outputs. An absent Artifact under an existing Run succeeds; an absent parent
Run is an error. SQLite object-owned content rows are removed with their owner,
permitting internal space reuse, not guaranteed file shrinkage or secure erasure.
No deletion cascades to an independent Snapshot or service-owned data.

**Verification: PR-TEST-0450, PR-TEST-0451, PR-TEST-0458, PR-TEST-0459,
PR-TEST-0460, PR-TEST-0462, PR-TEST-0465, PR-TEST-0466, PR-TEST-0469, PR-TEST-0470, PR-TEST-0472, PR-TEST-0482, PR-TEST-0484.**

### PR-REQ-0342 - Sensitive Artifact delivery

Only published Artifacts can be exported. Export requires purpose-specific
`--authorize-sensitive-export` before staging or output effects. A filesystem
destination is required; stdout, overwrite and resume are unsupported. Content
is opaque and may contain Secrets; diagnostics MUST NOT reveal content or
sensitive derived information. Streaming MUST NOT inherit a Managed Input cap.
The existing [Artifact representation maximum](../persistence/persistence-baseline.md#pr-req-0275---exact-persistenceschemav4)
of 536,870,912 bytes remains unchanged; this scope does not increase publication
or storage capacity. No additional export-only ceiling is introduced.

Metadata and chunks MUST come from one consistent read. A read established before
deletion may complete; subsequent reads cannot access the object. This also applies
to Snapshot export/verification; accepted Restore retains its deletion guard.

Export uses protected staging and atomic no-clobber publication. Failure before
publication leaves no completed destination. A durability failure after publication
MUST acknowledge that the destination may exist and MUST NOT delete it to pretend
publication never happened.

**Verification: PR-TEST-0448, PR-TEST-0449, PR-TEST-0450, PR-TEST-0451,
PR-TEST-0452, PR-TEST-0453, PR-TEST-0454, PR-TEST-0455, PR-TEST-0466.**

Artifact evidence includes streaming at the existing Artifact maximum and a
cross-process deletion during an established read. Snapshot export retains its
read snapshot when another process commits deletion.

### PR-REQ-0343 - Foreground content collection

GC is explicit and separate from logical deletion. It MUST NOT expire objects or
remove service-owned state, including abandoned allocations. Only verified
Pactrun-owned immutable runtime, Snapshot and restored Input blobs unreachable from managed objects, accepted Runs,
checkpoints and recovery obligations are eligible. Shared references protect bytes.
Both execution and preview require an existing exact current reference catalog;
GC MUST NOT bootstrap a missing or pristine database and then treat its empty
reference set as authority to remove existing files.

Blob publication through reference commit, unpinned reads, and GC MUST coordinate
across processes. Lock order is content coordination before database transactions.
GC MUST NOT collect a publication-to-registration interval or race new references.
An unreliable reference graph stops collection before destructive work. Unknown
ownership, corrupt or unsupported entries are retained and reported. Names or age
alone are not ownership evidence. No symlink/reparse traversal or arbitrary
recursive deletion is allowed.

Collection need not be batch-atomic. Retry freshly checks eligibility. Partial I/O
failure reports completed and incomplete work and fails. Database reuse, estimated
blob bytes and actual filesystem free space MUST NOT be conflated. Workspace,
staging, service allocations, detached custody and retirement progress retain
their specialized lifetimes. No background worker, expiry, vacuum or secure erase
is introduced.

The implementation conservatively shares a content-coordination guard across
each ordinary store opening, including publication witnesses and unpinned read
handles. GC requires an exclusive guard and returns a retryable busy failure
while such users exist. The existing publication lock remains responsible for
blob publication mechanics; the outer guard spans publication through SQL commit.
Read-only preview opens an existing guard without creating a lease or lock file.
This transient observation does not reserve the candidates for a later execution.

On Linux, removal first claims the exact entry into a fresh owner-private directory
inside runtime-content, then validates the moved inode before unlink. A replaced
entry is not deleted; restoration uses no-replace semantics. Interrupted claims
are re-evaluated by digest and current references on retry. Foreign-shaped, empty
or unsupported claim directories are retained rather than recursively removed.
Claim traversal also refuses mount crossings, including same-filesystem bind
mounts; inability to qualify that boundary is not deletion authority.
Windows pins a no-write/no-delete-sharing file handle before verification and
removes that qualified handle. Outside hard links are refused on both platforms.
These mechanisms do not make the control store a sandbox against deliberate
tampering by its owning OS account.

Reports distinguish verified candidates, confirmed removals, referenced entries,
unsupported entries and failed qualifications/removals. Failed removals may
already be absent when a durability barrier failed; retry freshly observes them.
No count claims guaranteed filesystem free-space growth.

**Verification: PR-TEST-0457, PR-TEST-0461, PR-TEST-0463, PR-TEST-0464, PR-TEST-0467, PR-TEST-0469, PR-TEST-0471, PR-TEST-0474, PR-TEST-0475, PR-TEST-0484.**

### PR-REQ-0344 - Lifecycle command boundary

```text
pactrun run artifact export <run-id> <output-id> --output <path> --authorize-sensitive-export
pactrun run artifact delete <run-id> <output-id>
pactrun snapshot delete <snapshot-id>
pactrun revision delete <revision-reference>
pactrun run delete <run-id> [--delete-artifacts]
pactrun storage gc [--plan]
```

Existing global options and exit conventions apply. `--plan` observes candidates;
it is not a saved executable Plan or lifetime reservation. It MUST NOT create a
Run, pin, lease, staging area, upgrade or persistent plan. Execution rechecks under
coordination. There is no `--dry-run` alias, JSON envelope or public Rust API.

**Verification: PR-TEST-0449, PR-TEST-0450, PR-TEST-0455, PR-TEST-0458,
PR-TEST-0459, PR-TEST-0460, PR-TEST-0461, PR-TEST-0465, PR-TEST-0469, PR-TEST-0474.**

## Rationale (informative)

Separating object removal from external collection avoids coupling user intent to
maintenance. SQLite already owns its transactional content lifetime; tombstones
would add unnecessary machinery. Refusing protected history preserves existing
evidence without another archive. Explicit disclosure/cascade intent protects
outputs. `--plan` follows existing CLI conventions without making GC a Hook.
