---
title: E Persistence Retirement Ledger
---

# E Persistence retirement ledger

Closed engineering disposition for the authorized 2026-09-26 one-time reset.
Whole-E evidence and its exact scope are in the implementation ledger.
Retired test and requirement IDs remain reserved and must not be reused.

| Retired tests | Former obligation | Disposition |
| --- | --- | --- |
| PR-TEST-0060, PR-TEST-0061, PR-TEST-0073, PR-TEST-0082 | V1–V4 upgrade chains and crash transitions | Removed with development readers/ladders. Exact shape, concurrency and bootstrap crash assertions remain in PR-TEST-0052, PR-TEST-0201 and PR-TEST-0621–0623. |
| PR-TEST-0199, PR-TEST-0200 | Special V4 legacy-session migration fence | Removed with the special bootstrap; no lease scan grants authority to unsupported storage. Supported admission and late-writer fencing remain in PR-TEST-0196–0198. |
| PR-TEST-0205 | Historical storage-upgrade CLI success | Removed with the command. PR-TEST-0624 proves rejection before opening storage. |
| PR-TEST-0294–0298 | V5-to-V6 upgrade matrix, fencing, crashes and preservation | Removed with the upgrade. Shared admission/crash tests remain; migration payload/reference SQL invariants stay in PR-TEST-0293 and PR-TEST-0299. |
| PR-TEST-0341–0343 | V6-to-V7 upgrade | Removed. PR-TEST-0340 retains allocation custody; PR-TEST-0344 retains open migration/checkpoint preservation on reopen without reconciliation. |
| PR-TEST-0393–0395, PR-TEST-0401–0403 | V7-to-V8 replacement, migration rollback/corruption and old DDL oracle | Removed. PR-TEST-0392 retains Run/artifact survival after Instance retirement; PR-TEST-0400 retains custody; PR-TEST-0621 owns the fresh complete DDL oracle. |

Retired upgrade-only requirements: PR-REQ-0257, PR-REQ-0270, PR-REQ-0276,
PR-REQ-0300, PR-REQ-0312, PR-REQ-0325 and PR-REQ-0339. Their anchors remain,
with explicit retirement rather than pending runtime coverage.

Mixed tests were rebased, not discarded: PR-TEST-0182/0183, PR-TEST-0195,
PR-TEST-0201/0202, PR-TEST-0293/0299, PR-TEST-0340/0344, PR-TEST-0392/0400,
PR-TEST-0456/0468/0473, PR-TEST-0483/0485 and PR-TEST-0524. Current safety coverage passed the source-qualified E gate. Historical evidence
is not represented as a passing test of the new baseline.

No pre-existing managed database or service resources were deleted. Source code,
SQL upgrade fragments and obsolete-only tests are the retirement targets.
Whole-E delivery and validation are tracked in
[the implementation ledger](./e-implementation-status.md).

Additional retained cases rebased during full E integration: PR-TEST-0404/0424
now run the one baseline Cleanup contract rather than duplicated numeric protocol
versions; PR-TEST-0426 retains service prerequisites. PR-TEST-0214 now verifies
baseline producer/protection validation (both valid sticky-secret and forbidden
secret-to-normal cases), not a retired V1/V2 reclassification rule. PR-TEST-0383
still proves service-free/service-bearing migration, retention and explicit
reattachment under one current source contract. All these IDs remain active.
