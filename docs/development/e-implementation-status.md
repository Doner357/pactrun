---
title: E Implementation Status
---

# E implementation status

**Status: In progress; read-coordination clarification approved, final verification pending. NOT complete and NOT merged to develop.**

The operator authorized the entire E milestone on 2026-09-26, including local
implementation, all required verification and eventual merge to develop. There
is no slice-by-slice approval gate. Publication/push, Pages/document restructuring
and unscoped real data/resource deletion remain excluded. The confirmed plan is
[E S0](./design-notes/e-versioning-s0-plan.md); the whole-milestone objective has
not been reduced by recording intermediate evidence here.

## Current implementation inventory

| Area | Actual state |
| --- | --- |
| Product candidate | Product 1.0.0-alpha.1 is now the local qualification candidate; this is NOT a completion or publication claim. |
| Version policy | Shared bounded product/format parsers, explicit eight-domain support, separate defaults, release precedence and no implicit downgrade/cross-Major implemented. |
| Runtime contracts | Pack source, complete Revision canonical/service/shell model, Hook protocol, Snapshot integrity/bundle, Pack distribution, machine interface and Persistence use exact string baselines. Old numeric readers/dispatch are removed. |
| Identity | Revision/Snapshot frames no longer carry the redundant integer version. Independent Node oracles calculate replacement baseline vectors; blob SHA-256 remains unchanged. |
| Persistence | Complete 72-table DDL, metadata-first refusal and transactional admission are active. Historical ladders and inline Snapshot storage are removed; active Managed Input chunks, ownership, recovery, references and service custody remain. |
| Native integration | Program-only normal/test launchers and a metadata-only source selector are implemented. Major-scoped clones, real Git tracking/origin-HEAD checks, immutable exact refs and monotonic local source publication have focused tests. The real Scoop/Homebrew matrix passed on the qualified Windows/Linux hosts; exact source and fixture identities are recorded below. |
| Artifact tooling | Source-qualified build provenance, deterministic archives, platform/flavor manifests, explicit formal-default gates and local-only publication tooling implemented; six actual alpha.1 candidate archives and measured platform dependencies are recorded in the local delivery directory. |
| Owning Specs | Current format requirements moved once to consolidated owners; old pages are non-normative explanations. Persistence obligations now have one owner. Bidirectional traceability and current document links have passed focused checks; final site and full-source checks remain. |
| Full E CI and merge | NOT completed. No E completion or merge to develop is claimed. |

## Focused evidence

- Version unit cases PR-TEST-0618, PR-TEST-0619 and PR-TEST-0620 passed.
- CLI schema contract generation check passed after the interface reset.
- The focused CLI library suite passed 63 tests, retaining redaction, raw payload,
  partial result, cancellation, output-failure, ownership and delivery assertions.
- Fresh DDL test PR-TEST-0621 passed: exact owning SQL, integrity, representative
  tables, singleton/admission constraints and transactional bootstrap rollback.
- Persistence activation checks passed: PR-TEST-0622/0623; admission suite 10
  cases; retained SQL constraint suite 7 cases; reopening preservation 4 cases;
  Revision persistence suite 8 cases; metadata suite 7 cases. The old-only test
  IDs and surviving safety assertions are recorded in the retirement ledger.
- CLI schema was regenerated after removing storage upgrade and its focused check
  passed. The current CLI suite passed all 63 cases after replacing a fixed-sleep
  test race with bounded observation while the writer stays blocked; product
  timeout values and the independent-finalization assertion are unchanged.
- Windows service-storage subset passed 27 cases. Remote Persistence verification
  initially found 16 failures caused by retained pre-V7/pre-V8 numeric guards;
  those guards and test-only table-presence fallbacks are now removed. The repaired
  remote Persistence subset passed all 180 cases (670.90 seconds); the failed
  attempt is retained separately, not relabeled as a pass.
- Remote documentation tests passed 27 cases, typecheck and site build passed
  without broken-link warnings. Source-qualified evidence is in the dedicated
  e-persistence-baseline-20260926 workspace; these are not full product CI.
- A subsequent bounded Persistence version-read change uses the shared exact
  support selector and rejects malformed/oversized labels. Its three focused
  Windows tests and current workspace/all-target/all-feature Clippy passed; this
  delta is separate from the preceding 180-case remote source snapshot.
- These tests are not a complete E integration or compatibility certification.
  Evidence is under local target/e-implementation/evidence. Checks after later
  changes must be tied to the actual affected inputs, not reused by filename.

## Remaining ordered work

1. Complete source-qualified integration/conformance validation of the new wire
   baselines and owning Specs; repair all regressions without restoring old readers.
2. Build actual candidate artifacts from reviewed committed inputs, inspect platform
   dependencies, and exercise native Scoop/Homebrew install, update, exact selection,
   explicit switch, normal/test coexistence, cleanup/uninstall and live-operation
   continuity. Do not substitute earlier prototype evidence.
3. Complete error/selector and promotion/continuity coverage and remaining traceability.
4. Run full remote cargo xtask ci plus current Windows/platform checks. Record exact
   source/binary/manager inputs, complete local engineering documentation and merge
   the completed result to develop. No push or GitHub publication is authorized.

Existing development-era runtime acceptance must not be mistaken for compliance
with the new Spec during this transition. Pending behavior is explicit, not
permission to leave historical support or substitute safe refusal for future
formal same-Major compatibility. No future codec is to be invented in advance.

## Wire/native-tooling continuation evidence

New-baseline focused checks passed: source authoring 11; canonical service/shell
codec 6; shared declarations 8; Snapshot integrity 11; Hook common/authority 17;
Shell Loader 6; Snapshot bundle 9; Pack transport 16 (the explicit >512 MiB case
remains separately scheduled); machine/CLI 64. Later bounded-refusal and build
provenance edits require their own affected rechecks. Reference verifier tests
passed 40 cases; actual Revision/Snapshot/Hook/Error conformance commands have
passed after retargeting baseline fixtures. Native launcher, real source-selection
Git tracking, local immutable publication and artifact-assembly tests passed in
workspace-local fixtures. These results are NOT the full E gate.

Necessary native build/selection references are in
[native package engineering delivery](./native-package-delivery.md).

## Source-qualified integration repair checkpoint (2026-09-27)

The first Linux gate at 52ecd80 stopped at a redundant test-only allocation;
513435a repairs that Clippy warning. Its full library run then reported 498
passed, 20 failed and four explicitly ignored tests. That is a failed gate, not
completion. Failures identified remaining numeric Cleanup/source and workflow
fixtures, Snapshot producer/corruption fixtures, and a real retirement bug:
parent cascades could reach payloads before bindings under the fresh DDL order.
Retirement now explicitly removes its own bindings before the Instance cascade,
in the existing authorized retirement transaction; no upgrade rewrites data.
Existing lifecycle, service/resource preservation and crash assertions remain.
Windows repair checks passed Cleanup 13, workflow eight and five individually
selected lifecycle/Snapshot/service cases; source-refusal PR-TEST-0638 also passed.

Actual native rehearsals on the initial 52ecd80 product plus an isolated,
software-version-only alpha.2 fixture passed Scoop and Homebrew installation,
upgrade, hold/pin, exact downgrade, isolation, checksum refusal and live Hook
update/cleanup/uninstall. They are preliminary evidence: the final input set and
the expanded PR-TEST-0637 matrix still require qualification. The native failure
history exposed Git archive inheriting host CRLF settings; source capture now
pins Git's conversion configuration, SQL checkout has an LF attribute and a
regression proves identical snapshots under LF/CRLF host settings. Failed
CRLF-derived fixtures remain evidence, not qualified artifacts.

The reproducible opt-in runner is tools/native_package_acceptance.py, invoked by
PR-TEST-0637. Its Windows gate requires explicit temporary user PATH permission;
normal CI ignores this external-manager test, so it must be run separately on
both platforms. The expanded gate verifies object bytes/identities, exact and
preview selection after real manager metadata refresh, and live explicit switch.
No final full-CI pass or merge is claimed at this checkpoint.

## Current acceptance and blocking counterexample

Candidate c673c0cf7c162aef785d31cbbdad5d358a65e83d has source-manifest
549dfd5c717502c2fe71969483a8a14107357ad1781edfff90869ddebcae03e1.
Its Windows/Linux release artifacts use Rust 1.98.1. The software-only alpha.2
fixture is 995b094494a33755febc700780d2544fced73875; manifest
f29b0904a01ead9864395ab222f16d6051636367a7c27e66774d84cba8af122f differs
only in Cargo.toml and Cargo.lock. No alpha.2 publication is authorized.

- PR-TEST-0637 passed through the actual Rust opt-in entry point on Windows
  (native-windows-c673c0c) and Linux (native-linux-refresh). Linux's isolated
  Homebrew mirror needed the test-only main-branch correction in 1c4c43c;
  no Pactrun runtime bytes changed. Both exercise real manager source refresh,
  exact/preview persistence, pin/hold, default isolation, canonical/identity
  preservation, checksum failure and active Hook update/cleanup/switch/uninstall.
  Windows PATH fingerprints match after restoration.
- The c673c0c isolated-target full gate passed conformance, formatting, Clippy
  and all 519 active library tests (four ignored). It later stopped on one old
  diagnostic-text expectation in m5_path_selection (14 pass, one fail). The
  assertion now checks the Hook domain, required and supported versions; that
  focused Windows case passed. Remaining full-gate targets and the final site
  build have NOT been certified by this run.
- The three separately selected capacity cases passed: Pack >512 MiB and
  Snapshot 1 MiB/1 GiB. The 17-GiB Snapshot case still requires at least 160 GiB
  free and was not run. There is no current-source beyond-16-GiB capacity claim.
- An earlier attempt using a shared Cargo target embedded the prior xtask
  workspace. It was stopped and invalidated, not counted as c673c0c evidence.
  The qualified retry used test-target-c673c0c, verified the runner's source
  path and ran the new refusal/retirement smoke tests before the full gate.
- Windows imports contain OS DLLs, not an external MSVC runtime. The Linux
  build was exercised on Debian 13, glibc 2.41, kernel 7.0.14-19-pve and ZFS;
  the observed GNU symbol ceiling is GLIBC_2.39 (launcher GLIBC_2.34).
  Windows host is build 26200 on NTFS. These are current-host observations,
  not clean-OS/minimum-kernel/all-filesystem certification.

**Do not merge:** final safety review found that a read-only SQLite connection
can create an empty -wal and a 32-KiB -shm while rejecting an unsupported WAL-mode
database. The real candidate counterexample leaves the main database SHA-256
unchanged, but violates this Spec's stronger requirement of no journal or
coordination creation before refusal. No real operator data was used.
PR-TEST-0623 now includes WAL-mode unsupported metadata/bootstrap cases; case
19 reproduces the failure (database directory entries increase from one to three).
This regression is intentionally not ignored or weakened. The approved normative
rule has NOT been changed. Any proposed narrowly scoped allowance for SQLite's
own read coordination needs explicit semantic approval; otherwise implementation
must satisfy the existing zero-sidecar rule before completion.

Evidence: target/e-qualification/unsupported-wal-counterexample.json and
unsupported-wal-regression.log. Candidate artifacts are staged in target/e-delivery;
they are engineering candidates, not a completed or published release. Uncommitted
regression/assertion and documentation changes are retained for continuation.

## Read-coordination decision accepted (2026-09-27)

The operator approved the narrow SQLite read-coordination exception described
above. The prior blocker is resolved by the owning-contract clarification, not
by suppressing the counterexample or introducing an unsafe reader. PR-TEST-0623
now allows only SQLite -wal/-shm additions for WAL-mode inspection while keeping
main database bytes, service sentinels and absence of Pactrun staging/content
coordination mandatory. A newly created WAL in that quiescent fixture must stay
empty. PR-TEST-0639 separately verifies that an already committed WAL is not
changed or ignored and that its existing writer retains control and can commit.
The final gate and documentation closeout remain pending; no merge is claimed.

The four Windows baseline-schema tests (including PR-TEST-0639) passed after the
approved clarification, with main-file/committed-WAL byte preservation and a
continuing live writer. Local documentation/link/traceability checks passed 27
cases. The former migration refusal's stale prose assertion was also repaired
without weakening its no-execution assertions. These focused results precede
the final source-qualified full gate and are not a substitute for it.
