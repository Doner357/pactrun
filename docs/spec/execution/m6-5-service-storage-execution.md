---
title: M6.5 ServiceStorage Execution
---

# M6.5 ServiceStorage execution

**Status: Implemented normative ServiceStorage execution contract.**

<!-- spec-navigation:start -->
## Reading map (informative)

Read the [Core](../contracts/revision-core-format-v2.md),
[Hook](../contracts/hook-protocol-v2.md), [V7](../persistence/persistence-schema-v7.md),
and [CLI](../behavior/m6-5-service-storage-command-reference.md) contracts.
Existing [M5](./m5-migration-execution.md) and [M6 recovery](./recovery-and-reconciliation.md)
remain the starting contracts. The [implementation record](../../development/m6-5-implementation-status.md)
distinguishes implemented behavior from final-source validation and integration.
<!-- spec-navigation:end -->

### PR-REQ-0326 - Compilation, admission and publication

Instance creation allocates every declared Storage before successful Instance
publication; it does not create declared resources or service parent trees.
The user sees either no Instance or the complete Instance with its durable
storage associations. No public Creating/Allocating state is introduced. V7
defines pre-publication ownership and cleanup for failures between filesystem
and SQLite boundaries; they are not one distributed atomic transaction.

Compilation remains read-only. It selects an exact same-Package Revision path,
symbolically evaluates all Input and service mappings, roles, target writers,
Hook versions, expected resource kinds and explicit retained dependencies.
It reads current/retained associations, not contents. No allocation, Run,
mutation, service scan, risk marker or live Snapshot occurs during --plan.
Future target presence is not guessed: requirements depending on unexecuted
transformations are reported as runtime checks, not falsely proven preflight.

One Run and one continuing Mutate admission own a Migration chain. Initial
Admission atomically revalidates the Instance token, exact Revision path,
current/retained source associations, guard override, ordinary Input/Secret
invariants, host launch facts and source allocation availability, then pins the
needed allocated roots and declaration Revisions. A new service association
cannot be smuggled into M1-D metadata or ManagedInputBindings. The Run's own
commits advance its boundary; unrelated changes do not silently rebase the Plan.

Before each Hook, materialize the current edge's source/target association
records under the same owner and Mutate protection. Allocate new target roots
before issuing authority, and publish protection of an allocation before its
path can leave Pactrun. Evaluate definitive presence prerequisites once at this
admission-to-launch boundary; Unknown never satisfies either predicate. The
observation is not a service lock or a guarantee at Hook execution time.
Resource-only creation authority requires an existing safe parent, unless an
explicit containing-directory/storage write grant covers creating missing
ancestors. No service directories are implicitly created to make access succeed.

Paths must remain under their owned allocation through handle-relative/no-follow
prefix validation. Refuse symlinks, Windows reparse traversal, non-regular file
resources, and multiply linked file resources for mediated access. A wrong leaf
kind can be observed Present but fails authority qualification; Present never
proves validity/coherence. External native services may race afterward; no OS
confinement, atomic live-byte snapshot, reader lock or mutation linearization
is promised. Live existence changes never increment InstanceStateVersion.

On a compatible/create-only declarative edge, revalidate mapping predicates
and explicit create-presence predicates immediately before atomic publication.
On a Hook edge, create presence is checked before grant; the Hook may then create
an absent object. A create with presence any imposes no existence prerequisite,
but does not waive safe path qualification for an actual grant. Transform edges
use PR-REQ-0322; other Hook edges retain
ordinary completion. Never infer compatible state from bytes, matching names,
physical locators or successful process launch.

The target transaction publishes all of the following or none:

1. target Revision and staged Managed Input state/dispositions;
2. active service storage/resource associations, preserving only unmapped
   source-only associations as retained, consuming mapped source associations
   and inserting/replacing their targets atomically without deleting bytes;
3. fresh InstanceStateVersion and committed edge/recovery-boundary evidence;
4. this Run's risk clear for an accepted, owner-held target proposal;
5. final Run Succeeded on the final edge, or Running at the next edge boundary.

For transform proposals, risk may remain Open at entry to this transaction but
must be cleared inside it before publishing successful terminal evidence. This
is the distinct V2 target-publication path, not permission to pass an ordinary
success-with-Open completion into the existing finalizer. The publication
capability comes only from the validated owner continuation after observed Hook
termination, never from a database flag or caller-chosen operation name.

No service bytes are in this transaction. Intermediate commits survive later
failure. Final publication and target proposal do not clear a pre-existing
manual-recovery guard. Owner loss reuses M6's actual lease probes, durable
boundary, risk, guard and reference rules, without Plan replay, output salvage
or service compensation. All operation-specific publication retries retain
one owner; no uncertain target-ready receipt authorizes replay.

**Verification: PR-TEST-0354, PR-TEST-0355, PR-TEST-0366, PR-TEST-0367, PR-TEST-0368, PR-TEST-0370, PR-TEST-0371, PR-TEST-0373, PR-TEST-0374, PR-TEST-0375, PR-TEST-0376, PR-TEST-0377, PR-TEST-0378, PR-TEST-0380, PR-TEST-0381, PR-TEST-0382, PR-TEST-0383, PR-TEST-0386, PR-TEST-0387, PR-TEST-0388.**

Filesystem observation substrate coverage: absence, observed kind,
file link counts, and refusal of symlink/reparse traversal without content reads.
These primitive tests do not establish supervised operation or Migration
publication and target-risk coordination.

PR-TEST-0366 additionally verifies detached typed Action-plan preservation of
V2 authority and presence prerequisites without a live-resource observation.
Actual service admission is separately exercised below.
PR-TEST-0367 covers refusal of unsupported future Hook versions during Action
compilation while preserving their independent Core representation.
PR-TEST-0368 covers revalidation of compiled association facts and the atomic,
deduplicated, owner-scoped pin helper, including release without deleting live
bytes. Its pin transaction fixture is not full V2 Hook Admission evidence.
PR-TEST-0370 covers actual V2 Action service admission, native grants/prerequisites,
ordinary completion and open-risk failure without reverting live contents.
It also executes V2 Capture/Restore with explicit service recovery content and
the existing atomic Restore-only guard-clear rule.
PR-TEST-0371/0373 cover symbolic service-edge evaluation without allocating
future roots. PR-TEST-0374/0375 exercise real metadata-only compatible Migration
publication, consumed renames, unmapped retention, explicit reattachment and
old-or-new crash boundaries. PR-TEST-0376 exercises fresh current-edge target
allocation, protection and exact retry selection before declarative publication.
PR-TEST-0377/0378 exercise real transforming Hooks, the open-risk interval after
target writes, distinct owner-held target permits, atomic publication and
process crashes before/after commit. Multi-edge/output and compatibility
evidence is separately listed below; the implementation record owns platform
and final-source closeout results.
PR-TEST-0380 additionally verifies that an intermediate committed transform's
Managed Input output, service associations and clear-risk boundary survive a
process crash before the following declarative edge commits, without replay.
PR-TEST-0381 verifies that cancellation or timeout after an observed proposal
receipt and scratch cleanup failure prevent target publication while retaining Open
risk and the already-written service contents.
PR-TEST-0382 executes a real split-then-merge Hook chain with separate target
allocations. A failed second Hook preserves the committed split associations
and bytes, leaves risk Open, and exposes the uncommitted merge allocation only
as protected provenance. Success consumes both split sources without aliases.
PR-TEST-0383 covers public installation, V1-to-V2 allocation, V2-to-V1 retention
and explicit V2 reattachment without replacing the live allocation or bytes.
PR-TEST-0386 covers actual file-to-directory transformation and nested grants;
PR-TEST-0387 covers native permission-denial classification. PR-TEST-0388 checks
all resource-create presence predicates without copying, validating or changing
service bytes, and keeps planning independent of those runtime observations.

### PR-REQ-0327 - Retention and M7 handoff boundary

Active versus retained is derived from the current Core's declared IDs and
the association's last published declaration Revision. A retained association
keeps that exact contract and physical allocation; retained is not an existence
observation. A resource moved/deleted by its trusted service can remain a retained
association with Absent live observation. Unmapped source-only storages retain
their allocation. A mapped rename consumes its source association, not its
bytes; it does not create a persistent alias. Reactivation of retained
associations always requires explicit target-owned reattach.

Two layers must remain distinct:

- Published retained associations can be listed and explicitly reattached under
  their last contract. There is at most one published association per typed
  identity per Instance. A reused identity updates its published contract only
  in the target transaction.
- Protected allocations exposed by an unsuccessful, uncommitted target attempt
  are preserved with origin Run/Revision/storage identity but are not a published
  target resource association. Run inspection can identify this retained
  allocation evidence. M6.5 does not silently select it on retry, turn it into
  an active declaration, or offer a special failed-attempt adoption command.
  Manual service repair and a new approved Migration remain explicit work.

The second distinction is the conservative limit approved in S0 review:
reattachment applies to published retained contracts, not arbitrary failed
preparation directories. A retry allocating another root cannot erase the first.

V7 permanently protects published/exposed allocations from ordinary GC,
unreferenced-blob collection, stale-workspace cleanup and Instance cascade.
M6.5 supplies no destructive storage finalization or discard path. Only a
never-published/unexposed preparation may be removed on confirmed owner loss,
and then only by removing its known empty directory without traversal. Unexpected
contents or an unsafe path preserve its durable record for diagnosis.

M7 owns the later explicit lifecycle schema extension and APIs. Before M7 can
delete an Instance or allocation, its approved design must add durable receipts
for Cleanup-completed/do-not-replay, finalization retry and abandonment/handoff.
The receipt must precede physical finalization; after it, only Pactrun-owned
finalization retries. Ambiguous Cleanup completion authorizes neither replay
nor finalization. No-Runner/no-Hook cleanup needs its own explicit receipt, not
a fabricated Hook success.

The V7 ownership boundary is fixed now: allocation IDs, origin identities and
last owner InstanceId outlive Instance removal; no FK or ordinary GC rule can
erase them. M7 must durably preserve that custody and non-destruction obligation
before removing Instance associations. Abandonment cannot be represented by
deleting V7 allocation rows or losing their discoverability. Any later discard
requires separately recorded explicit authority and a destructive finalization
receipt. M7 adds that schema through an explicit versioned upgrade; V7 contains
no speculative M7 operation ranks, nullable receipts or placeholder Deleting
Instance states. This is a decided migration boundary, not permission to mutate
V7 DDL silently when implementing M7.

**Verification: PR-TEST-0371, PR-TEST-0372, PR-TEST-0374, PR-TEST-0375, PR-TEST-0376, PR-TEST-0382, PR-TEST-0383.**

M6.5 association/lifetime evidence only. These tests do not implement
M7 Cleanup, deletion, finalization or AbandonManagement operations.
