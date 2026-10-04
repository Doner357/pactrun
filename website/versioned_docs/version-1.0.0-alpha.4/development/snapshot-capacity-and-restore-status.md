---
title: Snapshot Capacity and Restore Status
---

# Snapshot capacity and Restore workflow implementation record

**Status: S0-S5 implemented, verified and integrated into local `develop`.
Local commit/merge authorized on 2026-09-18; not pushed, released or published.**

The operator approved the [baseline](./design-notes/snapshot-capacity-and-restore-baseline.md)
and continuous S0-S5 execution. The original 64 GiB remote filesystem was too
small for required acceptance; the operator expanded it to 200 GiB. No scope or
acceptance threshold was weakened to work around the resource blocker.

## Authorized Git integration

On 2026-09-18 the operator authorized local commit, merge and closeout.
Implementation commit `ead810f1a81e5a52e351e96b11987a8b3d766d36` was merged into
`develop` by no-fast-forward merge `b99ffbd748f5417c0aa8537fe9c5e2234142a31c`.
The merge tree exactly matches the feature tree; there were no conflicts or
runtime edits during integration. The subsequent documentation-only closeout
updates current entry points to V10 and records Shell Adapter / Loader as the
next separately planned scope, not an instruction to implement it.

The accepted source manifest still matches runtime, DDL, fixtures, dependencies
and build inputs. Full CI and all three capacity journeys are reused evidence,
not newly rerun gates. Closeout runs documentation/link and traceability checks
plus the remote website typecheck/build. No push, release or deployment was
requested. Existing unrelated untracked files are excluded from these commits.

## Historical finding: SQLite WAL-index growth

The disk expansion resolved the original resource blocker. A different issue was
found during real large-payload acceptance: fixed Pactrun copy buffers do not
make the complete SQLite WAL path independent of payload size. The existing
WAL profile and single-transaction Snapshot publication grow the mapped WAL-index
as chunk frames accumulate. The SQLite WAL File Format documentation, sections
2.1 and 3, describes that index growth; the live process confirms it.

During the two-17-GiB-plus-one-byte case, an observation recorded:

| Measurement | Observed value |
| --- | --- |
| WAL bytes during Capture publication | 23,276,442,672 |
| WAL-index file bytes | 45,219,840 |
| Resident WAL-index mapping | 44,160 KiB |
| Process VmRSS at that observation | 63,844 KiB |

This is payload-related backend metadata growth, not a large application copy
buffer. A finite peak below 128 MiB cannot prove the approved scale-independent
memory requirement. Do not raise a benchmark threshold or silently exclude
backend memory and then declare the milestone complete.

The original plan excluded schema changes. The operator subsequently authorized
necessary adjustments while retaining the memory and safety goals. The
[V10 contract](../spec/persistence/persistence-baseline.md) now reuses the
existing immutable file store and its publication/GC guard. This avoids a new
candidate-chunk engine and moves new service bytes out of SQLite WAL.

The large test was deliberately terminated after collecting this evidence, not
reported as a successful round trip or a product test assertion failure. Its
owned temporary fixture was removed after process termination; small diagnostic
historical artifacts remain in `target/snapshot-capacity-evidence/`. The original 160 GiB
free-space gate remains unchanged. ZFS delayed reclamation is handled by a bounded
wait rather than deleting unrelated files or reducing that gate.

## Implemented behavior

Service-data byte counters no longer impose the old 8/16/32 GiB product ceilings
or total bundle ceiling. Fixed structural limits remain separate from checked
aggregate arithmetic and SQLite's signed blob-length representation. V10 changes
internal persistence, with explicit V8/V9 upgrade and legacy inline readers; no
identity, canonical-byte or Hook wire change is needed.

`instance create <name> --revision <reference> --restore-from <snapshot-id>`
performs read-only preflight, then existing Create and exact-ID Restore. It keeps
the created Instance on any subsequent failure and reports partial completion
and any known Run, including uncertain acceptance. It is not an atomic workflow.
Initial Input options and combined `--plan` are rejected. Parameter sources are
read once before Create, including stdin; existing Restore options are reused.

Examples (use actual installed references and Snapshot IDs):

```text
pactrun instance create recovered --revision label:stable --restore-from <snapshot-id>
pactrun instance create recovered --revision <exact-reference> --restore-from <snapshot-id> --param-file mode=<path> --execution-timeout-ms 60000
```

Normative behavior lives in the [capacity contract](../spec/behavior/m4-runtime-capabilities.md)
and [command contract](../spec/behavior/m4-snapshot-command-reference.md).

## Complete-path audit

| Stage | Memory/accounting | Publication and failure boundary |
| --- | --- | --- |
| Capture acquisition | Streamed digest/staging; distinct logical source accounting, no payload-sized allocation | Candidate no-follow qualification and expected-length checks remain; source failure publishes no Snapshot |
| Private staging | Fixed copy buffers, checked byte totals; source/staged files consume disk rather than heap | Owner-scoped private files; failure drops staged candidates without secure-erasure claims |
| SQLite persistence | V10 stores new service bytes in immutable files, with only bounded metadata/references in SQL | Atomic Snapshot metadata publication after verified durable files; pre-commit orphan files are non-authoritative |
| Verification | One chunk stream and incremental digest; bounded manifest/descriptors | No repair or truncation; resource refusal cannot imply complete verification |
| Bundle import/export | 64 KiB copy buffers, bounded ZIP metadata, Stored/ZIP64 only | Full validation before import commit; export private staging and atomic no-clobber publication |
| Restore | Streamed content and fresh Input identities referencing immutable bytes, without new value copies into WAL | Cancellation between prelaunch writes, existing guarded replacement; legacy Input limits and reclamation remain independent |
| Lifecycle | Existing Snapshot admission pins and store content-coordination guard | Delete refuses active Restore, terminal cleanup releases pins; GC retains its existing cross-process exclusion |

Capture post-completion publication retains existing outcome arbitration and
owner-held retry semantics; late cancellation does not invent a new terminal
boundary or replay a Hook. Large transactions may need substantial disk and
hold the writer; this change promises neither throughput nor unlimited resources.

## Verification

New automated coverage includes exact-ID name replacement, preflight races,
partial completion, option/parameter rejection, cancellation, SQLite disk-full
rollback/reopen, staging I/O failure and actual increasing-payload round trips.
The former expansion-refusal test now proves admission beyond the removed limit;
Managed Input refusal and all metadata bounds remain independently tested.

The explicit capacity cases use two distinct service blobs and a fixed descriptor
count: 1 MiB + 1, 1 GiB + 1, and 17 GiB + 1 bytes per blob. The largest case
exceeds every former service-data ceiling, including the complete bundle limit.
Each writes real candidate bytes, Captures, verifies persisted content, Restores
through a real Hook, exports, imports into another store and verifies again.
Exact canonical manifest bytes and integrity digests agree after the round trip.

Run the opt-in cases sequentially on the configured persistent remote:

```text
cargo test --release --lib --no-run
python3 tests/snapshot_capacity_evidence.py <release-test-executable>
```

The runner checks available disk, records the test executable SHA-256, exit code,
elapsed time and per-case Linux wait4 peak RSS under
`target/snapshot-capacity-evidence-v10/`. The 128 MiB RSS ceiling is paired with
an allocation audit, zero inline Snapshot chunk assertions, and a 64 MiB control
SQLite/WAL ceiling in the fixed-descriptor fixture. These are acceptance checks,
not product byte quotas or a promise of arbitrary-scale performance/resources.

### Completed slices

| Slice | Result |
| --- | --- |
| S0 | Approved scope, expanded persistence decision, normative contracts and traceability recorded |
| S1 | Fixed service-data ceilings removed; structural/representation limits retained; V10 file-backed data active |
| S2 | Cancellation, resource failure, corruption, crash publication, upgrade, reference lifetime and GC checks passed |
| S3 | Exact-ID create-and-restore, read-only preflight and partial-completion behavior passed |
| S4 | All three actual-byte capacity journeys passed, including two distinct 17 GiB + 1 byte service blobs |
| S5 | Full remote CI, Windows regression checks and documentation verification passed; authorized local develop integration; no push/publication |

### Current V10 evidence

| Actual service bytes per blob (two distinct blobs) | Complete journey | Linux wait4 peak RSS | Elapsed |
| --- | --- | --- | --- |
| 1 MiB + 1 byte | Passed | 14,700 KiB | 2.598 s |
| 1 GiB + 1 byte | Passed | 14,704 KiB | 146.642 s |
| 17 GiB + 1 byte | Passed | 14,704 KiB | 3,160.385 s |

Every journey includes Capture, full verification, real-Hook Restore, export,
import into another root and full verification there. The largest service closure
is 34 GiB + 2 bytes; its complete bundle exceeds the former bundle ceiling. No
counter-only or sparse-file substitution stands in for those actual bytes.

After largest-case Capture, the control database was 4,096 bytes and its WAL was
486,192 bytes. Independent read-only observations after Restore and during
import verification found one published Snapshot and three file-backed blobs in
each store, with **zero inline Snapshot chunks**. Source DB/WAL were
4,096 / 560,352 bytes; imported DB/WAL were 4,096 / 370,832 bytes. Thus the measured
backend overhead did not move to an unbounded WAL index. wait4 reports process
peak RSS, not an aggregate host/filesystem-cache memory budget.

| Verification | Status |
| --- | --- |
| Windows formatting and all-target/all-feature Clippy | Passed |
| Windows Snapshot, integrity, lifecycle/upgrade, Migration and CLI focused checks | Passed |
| Windows system target | Passed: 48 tests; also an explicit small capacity round trip |
| Remote full `cargo xtask ci` | Passed: conformance, formatting, Clippy, workspace tests, website typecheck/build |
| Remote library test target | Passed: 435 tests; three opt-in capacity cases executed separately as shown above |
| Remote integration targets | Passed, including 47 system tests and 10 Migration CLI tests |
| Requirement/test traceability and document/link tests | Passed; 22 document tests |

Full CI used `CARGO_PROFILE_TEST_OPT_LEVEL=2`,
`CARGO_PROFILE_TEST_DEBUG_ASSERTIONS=true`,
`CARGO_PROFILE_TEST_OVERFLOW_CHECKS=true` and `RUST_TEST_THREADS=1` on the persistent
remote workspace. Assertions, overflow checks, test bodies and product deadline
semantics were retained. Initial attempts exposed stale predecessor fixtures and
test synchronization problems: predecessor construction now really removes new
columns, result markers publish atomically, and paused-risk inspection no longer
creates a new writer session while the Hook deadline runs. No product assertion
or required acceptance scenario was dropped.

The verified 342-file input manifest SHA-256 is
`96fb29b38af7fedbd364f861261361307fc7b7e76cb6eee1d73284eec04bf8a8`.
The capacity executable SHA-256 is
`d64a43a2c0d0a0ea2bf9b2f07dd591e4abc2628569bc1334b167e8571674b1a3`.
Logs/receipts are retained under `target/` locally and in the configured persistent
remote workspace. Large-case temporary data was removed; approximately 164 GiB
was available afterward. No new preview/background service was started.

Documentation-only closeout reuses this runtime evidence: runtime code, DDL,
fixtures, dependencies, toolchain and test inputs are unchanged. Relevant
traceability, links, website typecheck and build are rechecked after closeout;
that is not a claim of another fresh full-CI or capacity run.

### Historical V9 evidence (not current acceptance)

The previous inline implementation passed small and medium journeys but its
largest run was stopped at the WAL-index finding above. It is not reused as a
successful large-data or bounded-memory result.

The historical V9 capacity executable SHA-256 was
`622febf91fb8059a8cac6afc09b3cf8f0f518bcb7e7bf70dc04d70cbe81947b9`, built from
the transferred source archive SHA-256
`39eb83db8260c40feffe078c4c7aa08d035aa2e240902ce2c716f57fc486aaab`.
That historical record motivated the separately authorized V10 change; current
acceptance is the V10 evidence above.
