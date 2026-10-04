---
title: E Implementation Status
---

# E local engineering delivery

**Status: E implementation and local engineering verification complete. Integration target: local develop. Public publication is not authorized.**

The operator authorized the whole E milestone, its local commits and integration,
then approved the SQLite read-coordination clarification on 2026-09-27. The
[approved S0 plan](./design-notes/e-versioning-s0-plan.md) defines the alpha
engineering scope; beta, RC, formal publication and broader documentation/Pages
reorganization remain separate work. This delivery does not clear real managed
data or service resources, push Git, create public releases or modify Pages.

## Delivered scope

| Area | Delivered behavior |
| --- | --- |
| Product/version policy | Product 1.0.0-alpha.1; all eight domains 1.0-alpha.1. Bounded typed versions, exact support lists, independent explicit writer defaults and product-version precedence. |
| Runtime contracts | One complete Pack source, Revision canonical, Hook, Snapshot integrity/bundle, Pack distribution, CLI machine and Persistence baseline. No retired numeric readers, historical schema ladders, storage-upgrade command or inline Snapshot conversion. |
| Identity and existing objects | Canonical version strings bind identity; redundant integer digest prefixes are removed. Blob SHA-256 is unchanged. Product-only upgrades do not relabel canonical bytes, recompute identity or rebind existing objects. |
| Persistence | Fresh 72-table schema, readonly qualification and serialized write revalidation. Ownership, non-terminal Runs, recovery, custody, pins, CAS and active Managed Input chunks remain. |
| Native delivery | Scoop and Homebrew own install/activation/removal. Normal/test launchers use distinct external data roots; source helpers change Git eligibility only. Exact refs are immutable and Major channels do not roll backwards. |
| Artifacts | Six source-qualified Windows/Linux normal/test/standalone archives, SHA-256/length records, actual binary/compiler provenance and platform dependency inspection. |
| Specifications | One current owner per requirement, retired pages are non-normative explanations, stable IDs and incoming lifecycle anchors retained. The [retirement ledger](./e-persistence-retirement-ledger.md) records discarded upgrade-only obligations and preserved safety assertions. |

No future codec or implicit cross-Major compatibility is invented. An unsupported
contract remains an actionable refusal, not authorization for conversion, data
cleanup, guessed support or interference with services.

## Same-version help/README correction

The post-delivery display correction derives the help banner from Cargo's product
version and aligns README's format identifiers and contract link with the actual
schemas. Product 1.0.0-alpha.1, all eight 1.0-alpha.1 contracts, Cargo.lock and
persisted representations are unchanged. Supporting PR-TEST-0625 coverage checks
human/JSON/JSONL help and refusal to initialize storage; documentation checks now
include repository README links and the machine-format declarations.

The original E qualification inputs below remain historical evidence. The current
local delivery receipt at target/e-delivery/delivery.json identifies any refreshed
same-version binaries, their actual source and their targeted archive checks.
Earlier native-manager lifecycle results are not claimed as a fresh run against
those rebuilt payloads. Original artifacts/receipts are retained outside the
shipping payload; public immutable release records are not overwritten.

## Qualified inputs and evidence reuse

- Runtime/artifact source: c673c0cf7c162aef785d31cbbdad5d358a65e83d.
  Source manifest: 549dfd5c717502c2fe71969483a8a14107357ad1781edfff90869ddebcae03e1.
- Whole Rust/conformance gate source: 53a218ad93a74d342ab1f2ed18fa80ccd9f9b35f.
  Source manifest: ceb9000c65eda1baf94eccf2015840fb7b2ef620c75df14b69c43e25d391842a.
- Native upgrade fixture: 995b094494a33755febc700780d2544fced73875.
  Source manifest: f29b0904a01ead9864395ab222f16d6051636367a7c27e66774d84cba8af122f.
  Its source differs from the artifact candidate only in Cargo.toml/Cargo.lock
  product versions. This alpha.2 is a private test fixture, not a publication.
- Windows/Linux release artifacts both used Rust 1.98.1. Remote Rust CI used
  Rust 1.98.0, optimization level 1, debug information disabled, debug assertions
  and overflow checks enabled, one test thread, persistent ZFS data and short
  temporary IPC paths. These are distinct recorded build profiles.

Runtime, Cargo/build inputs and format vectors are unchanged between the artifact
source and the gate source. Differences are documentation, test fixtures and the
Linux-only native-harness mirror fix. The module baseline_schema_tests is cfg(test).
The explicit comparison is target/e-qualification/runtime-evidence-reuse.json.
Accordingly native/capacity evidence is reused for unchanged runtime inputs; it
is not attributed to a newly built binary. Final closeout changes only documents
and their checks; their actual source and build receipt are recorded separately.

## Verification results

| Check | Result and scope |
| --- | --- |
| Full Rust/conformance portion of cargo xtask ci | Passed at the gate source: independent Node/Rust parity, formatting, all-target/all-feature Clippy, 520 product library tests, all CLI/integration targets including 75 system tests, 48 xtask tests and doc-tests. |
| Documentation | Link/traceability/catalog checks, typecheck and Docusaurus production build are required at the final document tree. The closeout build verifies all four repaired incoming lifecycle anchors; success receipts are retained with the local delivery. |
| Windows affected checks | Passed baseline schema/refusal/live-WAL tests (four), Cleanup (13), workflow (eight), lifecycle/Snapshot/service repairs, public unsupported-version refusal, initialization journeys, CLI diagnostic refusal and Clippy. |
| PR-TEST-0637 native matrix | Passed through the Rust opt-in test on both qualified platforms. Actual source refresh preserves preview/exact tracking; hold/pin, metadata-only selection, no formal fallback, no downgrade/cross-Major, normal/test isolation, wrong-hash refusal and native explicit switch are covered. |
| Live native operations | Upgrade, cleanup, explicit switch and uninstall preserve the admitted Hook and service-file access witness. Deferred Scoop operations complete afterward. Existing Instance state and canonical Revision bytes remain unchanged. |
| Capacity checks | Separately passed Pack >512 MiB and Snapshot 1 MiB/1 GiB. Ordinary suites also exercise real ZIP64 offsets beyond u32. The 17-GiB Snapshot/RSS case was not rerun; it requires at least 160 GiB free. |

The original full cargo xtask ci command completed every Rust component, then
stopped because pnpm rejected a cross-workspace node_modules symlink. That run's
exit status remains failed. Only the owned symlink was removed, dependencies were
installed under the actual document workspace using the frozen lockfile, and
typecheck/build were continued. This is source-matched gate completion with
explicit reuse of the passed Rust components, not a fresh monolithic CI pass.
The final document build must additionally contain no broken-link/anchor warning.

PR-TEST-0637 is intentionally opt-in in ordinary CI because it needs prepared
artifacts and, on Windows, authorization for transient user PATH mutation. It
was run separately on both platforms. Scoop PATH fingerprints match after removal
of only the test shim entry. Fixture HTTP servers and Hook children were closed.
A file/handle witness is not certification of every third-party daemon.

## Approved SQLite read-coordination boundary

PR-TEST-0623 covers unsupported ownership, formats, shape and WAL-mode inspection.
SQLite may create/update its own read-coordination sidecars; that does not admit
Pactrun staging, leases, content coordination, conversion or cleanup. Database
bytes, service sentinels and existing object identities remain protected.
PR-TEST-0639 proves a committed WAL is neither changed nor ignored and its live
writer can continue committing. Existing WAL files are not disposable merely
because they are sidecars. No immutable-mode bypass or sidecar-deletion workaround
was added. The owning rules are PR-REQ-0078 and PR-REQ-0299.

## Repairs and failed-attempt history

Earlier failures are retained, not relabeled as passes: Linux test-only Clippy;
remaining numeric Cleanup/source/workflow fixtures; a fresh-DDL cascade-order bug
fixed by removing owned bindings before the Instance cascade; Snapshot producer
and corruption fixtures; stale private-marker/diagnostic assertions; and Git
archive inheriting host CRLF settings. Source capture now pins conversion policy,
SQL checkout is LF, and regression tests prove host-independent snapshots.

An attempt sharing a Cargo target across source roots reused an xtask with an old
compiled-in workspace. It was stopped and invalidated. Qualified gates use fresh
targets, verify the runner source path and run source-specific smoke tests first.
The native Homebrew refresh harness uses an owned main-branch mirror at its
qualified commit; its first master-only mirror failure did not change product code.

## Platform, delivery and publication limits

Windows verification used Windows 11 build 26200 on NTFS and Scoop commit
b588a06e41d920d2123ec70aee682bae14935939. PE imports are OS DLLs, without a separate
MSVC runtime dependency. Linux used Debian 13, glibc 2.41, kernel 7.0.14-19-pve,
ZFS and Homebrew commit 570982948a8a194f0f42f43f4a5bce2d1c9f64cb. Observed GNU
symbol ceilings are GLIBC_2.39 (product/source helper) and GLIBC_2.34 (launcher).
These are measured current-host facts, not clean-OS/minimum-kernel/all-filesystem
qualification. No Windows 10, macOS, ARM64 or musl artifact is promised here.

The local delivery is target/e-delivery: six alpha.1 assets, checked checksums,
source capture, source-qualified test/platform evidence and a delivery index.
Implementation/test snapshots remain in target/e-qualification. The documented
[native engineering and publication procedure](./native-package-delivery.md)
requires final hosted URLs and a separate trust/license/publication review;
loopback test sources are not public download channels. Wider actual-use signoff,
formal writer defaults, prerelease acceptance dispositions and declared minimum
environments remain explicit later release gates, not an automatic continuation.
