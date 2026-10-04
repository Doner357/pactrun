---
title: M4 Runtime Capabilities
---

# M4 Runtime Capabilities

**Status: Approved normative structural capabilities and checked byte accounting; revised on 2026-09-17.**

<!-- spec-navigation:start -->
## Reading map (informative)

Read the exact inclusive limits for each Snapshot acquisition and import profile. A limit from one profile does not replace another profile's checks.

Start with the [specification map](../index.md)
and [shared vocabulary](../glossary.md) if a term is unfamiliar.
Check [implementation status and remaining decisions](../../development/next-milestone.md)
before treating an approved contract as available runtime behavior.
The original status, rules, exceptions, and verification declarations below retain their meaning.
<!-- spec-navigation:end -->

### PR-REQ-0293 - Separate inclusive capability profiles

The build MUST enforce each applicable structural profile independently. Every fixed limit
is inclusive: equality passes that check; a larger value is a capability
refusal, not proof of format corruption. Passing one check does not waive any
other format, protocol, or execution prerequisite. KiB/MiB/GiB are powers of
1024. No user quota, bypass, force, or accept-larger option is exposed.

| Profile | Bound |
| --- | --- |
| Import/storage single blob | No fixed product byte ceiling |
| Import/storage distinct-digest closure | No fixed product byte ceiling |
| managed_bindings plus service_content descriptors | 65,536 |
| Raw manifest and canonical manifest, each | 16 MiB |
| Capture single service blob | No fixed product byte ceiling |
| Capture distinct-digest service closure | No fixed product byte ceiling |
| Capture logical source acquisition | No fixed product byte ceiling |
| Restore target Managed Input, each | Existing 512 MiB contract |
| Restore logical expansion | No fixed product byte ceiling |
| Bundle envelope bytes | 64 KiB |
| ZIP entries | 65,538 |
| ZIP non-member-data metadata bytes, aggregate | 64 MiB |
| Complete input bundle bytes | No fixed product byte ceiling |
| JSON nesting depth | 16 |

ZIP metadata includes headers, directory records, extra fields, and comments;
it excludes member data. The parser MUST bound preflight and staging before
allocating full entry collections from untrusted counts. ZIP64 grants no
unlimited-resource acceptance. Existing Frozen HookProtocolV1 frame limits
remain independent and authoritative.

Snapshot unique closure counts a digest once across bound and service
references. Capture service closure counts a digest once among service
references. Restore expansion sums every bound descriptor and every service
descriptor's payload length, including repeated digest references. Host
deduplication MUST NOT alter this logical expansion budget.

A format-valid binding exceeding 512 MiB MAY be imported within the separate
storage/parser limits. Restore planning MUST diagnose capability refusal when
determinable; Admission MUST revalidate before launch. Capture acquisition,
stored closure and Restore expansion retain separate accounting, without fixed
product byte ceilings.

These profiles are not Snapshot integrity validity or permanent persisted
identity constraints. An otherwise valid stored object over a build limit MUST
NOT be repaired, truncated, deleted, or relabeled corrupt. If verification stops
at a limit, report incomplete verification, not valid or corrupt. Host disk,
allocation, and I/O failures are separate failures. Capabilities do not promise
performance, available disk, OS quotas, or containment of trusted Hook writes.

**Verification: PR-TEST-0178, PR-TEST-0180, PR-TEST-0181, PR-TEST-0188, PR-TEST-0207, PR-TEST-0215, PR-TEST-0216, PR-TEST-0217, PR-TEST-0237, PR-TEST-0251, PR-TEST-0266, PR-TEST-0274, PR-TEST-0276, PR-TEST-0479, PR-TEST-0480, PR-TEST-0481, PR-TEST-0482.**

Service payload processing MUST use bounded buffers, independent of total payload
bytes, across acquisition, staging, storage, verification, bundle import/export
and Restore. Metadata collections remain subject to the structural limits above.
Individual stored blob lengths must fit the backend's signed 64-bit length;
aggregate accounting and ZIP offsets use checked unsigned 64-bit arithmetic.
These representation constraints are not new integrity-format validity rules.
No user quota, automatic rollback, resume or new Snapshot integrity/bundle format is introduced.
The [V10 internal persistence activation](../persistence/persistence-baseline.md)
moves new payload bytes out of SQLite WAL while retaining atomic reference publication.
Existing transactional publication, pins and recovery behavior remain unchanged.

### PR-REQ-0294 - Capture logical acquisition accounting

The pre-dedup acquisition budget MUST count distinct logical acquisition
sources, not descriptors or final unique digests. One immutable managed payload
source counts once even when referenced repeatedly. Each distinct submitted
candidate source within this Capture counts once. Different sources with equal
bytes/digests still count separately. Hardlinks, inode equality, filesystem
aliases, cache hits, or implementation dedup MUST NOT reduce this accounting.
Internal repeated I/O does not count the same logical source twice.

Candidate source identity is operation-scoped and uses the existing submitted
source reference under the candidate authority. It MUST NOT become a persistent
pathname identity, new Hook Protocol field, Snapshot manifest field, or hash
input. Snapshot/domain IDs do not encode temporary host paths.

Lengths MUST be checked with overflow-safe arithmetic during actual acquisition
as well as any advisory size preflight. An unrepresentable count MUST refuse
Capture and publish no Snapshot; existing terminal risk rules still apply. There
is no fixed logical acquisition byte ceiling. Accounting overflow is not disk
exhaustion or an Import/Restore validity condition.

**Verification: PR-TEST-0179, PR-TEST-0181, PR-TEST-0247, PR-TEST-0479.**

The counter tests cover retained inclusive limits, removed byte ceilings and overflow-safe refusal. S5's real
Hook acquisition test additionally checks repeated submitted sources, distinct
hardlink paths with equal bytes, unique stored closure and publication retry
without resetting the source budget. No test substitutes counters for the real
greater-than-512-MiB streaming path linked in PR-REQ-0293.
