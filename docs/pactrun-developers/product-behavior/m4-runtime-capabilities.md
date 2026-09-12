---
title: M4 Runtime Capabilities
---

# M4 Runtime Capabilities

**Status: Approved normative fixed build capabilities.**

### PR-REQ-0293 - Separate inclusive capability profiles

The M4 build MUST enforce each applicable profile independently. Every limit
is inclusive: equality passes that check; a larger value is a capability
refusal, not proof of format corruption. Passing one check does not waive any
other format, protocol, or execution prerequisite. KiB/MiB/GiB are powers of
1024. No user quota, bypass, force, or accept-larger option is exposed.

| Profile | Bound |
| --- | --- |
| Import/storage single blob | 8 GiB |
| Import/storage distinct-digest closure | 32 GiB |
| managed_bindings plus service_content descriptors | 65,536 |
| Raw manifest and canonical manifest, each | 16 MiB |
| Capture single service blob | 8 GiB |
| Capture distinct-digest service closure | 16 GiB |
| Capture logical source acquisition | 32 GiB |
| Restore target Managed Input, each | Existing 512 MiB contract |
| Restore logical expansion | 32 GiB |
| Bundle envelope bytes | 64 KiB |
| ZIP entries | 65,538 |
| ZIP non-member-data metadata bytes, aggregate | 64 MiB |
| Complete input bundle bytes | 32 GiB + 128 MiB |
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
determinable; Admission MUST revalidate before launch. The Capture 16 GiB
service closure and Restore 32 GiB expansion MUST NOT become Import limits.

These profiles are not Snapshot integrity validity or permanent persisted
identity constraints. An otherwise valid stored object over a build limit MUST
NOT be repaired, truncated, deleted, or relabeled corrupt. If verification stops
at a limit, report incomplete verification, not valid or corrupt. Host disk,
allocation, and I/O failures are separate failures. Capabilities do not promise
performance, available disk, OS quotas, or containment of trusted Hook writes.

**Verification: PR-TEST-0178, PR-TEST-0180, PR-TEST-0181, PR-TEST-0188, PR-TEST-0207, PR-TEST-0215, PR-TEST-0216, PR-TEST-0217, PR-TEST-0237, PR-TEST-0251, PR-TEST-0266, PR-TEST-0274, PR-TEST-0276.**

Coverage now includes bounded bundle parsing, ZIP64 entry counts, and a real
stored service blob above the Managed Input publication limit, plus real S5
Capture acquisition and chunk publication of a 512 MiB + 1 byte service source.
S6 also streams a real greater-than-512-MiB service payload through Restore
materialization and the Hook, including repeated logical references. Human CLI
enforcement remains later-slice work; storage
acceptance is not a claim of executable Restore eligibility.
S4 independently applies Restore capability checks in the Compiler and in
serialized Admission. A physically verified, valid imported closure whose
logical expansion exceeds this build's capability cannot obtain a launch claim.

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
as well as any advisory size preflight. Exceeding 32 GiB MUST refuse Capture and
publish no Snapshot; existing terminal risk rules still apply. It is not disk
exhaustion or an Import/Restore validity condition.

**Verification: PR-TEST-0179, PR-TEST-0181, PR-TEST-0247.**

The counter tests cover inclusive limits and overflow-safe refusal. S5's real
Hook acquisition test additionally checks repeated submitted sources, distinct
hardlink paths with equal bytes, unique stored closure and publication retry
without resetting the source budget. No test substitutes counters for the real
greater-than-512-MiB streaming path linked in PR-REQ-0293.
