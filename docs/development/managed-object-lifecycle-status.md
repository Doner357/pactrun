---
title: Managed Object Lifecycle Implementation
---

# Managed object lifecycle implementation

Approved scope: 2026-09-17. **All S0-S5 slices are implemented, verified and
integrated into local `develop`.** No remote push, release or deployment is claimed.

## Ownership and reference inventory

| Object | Owned content | Strong protection | Independent provenance |
| --- | --- | --- | --- |
| Revision | Runtime references and installation metadata | Active Instance, Run pins | Package, historical Run/Snapshot identities |
| Runtime blob | Verified runtime-content file | Installed references; publication/read coordination | Filename alone is not authority |
| Snapshot | Manifest, blob rows and chunks | Restore admission and recovery | Origin Instance/Run and producer installation |
| Run | Invocation, outcome and Artifacts | Execution, guards, transition state, authorizations, receipts | Snapshot content |
| Artifact | Header and chunks | Consistent in-flight reader | Parent Run survives Artifact deletion |
| Input payload | Existing payload/chunks | Bindings, pins, recovery | Existing reclamation unchanged |
| ServiceStorage | Service-authoritative allocation | Custody and explicit retirement/discard | Never generic GC |
| Workspace/staging | Session-scoped scratch | Existing owner liveness | Not persistent object GC |

The following exact-V8 relationships were checked by reconstructing the embedded
DDL in an in-memory database and reading its foreign keys (no product database was
modified):

- Revision guards: `instances`, `run_revision_pins`,
  `run_migration_revision_pins`, `service_storage_preparations`,
  `instance_service_storages`, and `instance_service_resources`. The last three
  must not be overlooked merely because the active Instance Revision changed.
- `revision_runtime_content_refs` has a non-cascading owner FK and must be removed
  explicitly in the same approved deletion transaction. Its digest references are
  the runtime-store roots, not authority to remove files before transaction commit.
- All Revision presentation/current metadata, reference-label bindings, source URI
  and attribution claims, aliases, notes and trust use owner cascades. Package
  identities and service-allocation origin identities remain independent.
- Snapshot protection is `run_restore_admissions.snapshot_id`; Snapshot blob and
  chunk rows are owned cascades. Capture results and historical Restore invocation
  IDs are provenance, not Snapshot lifetime roots.
- Run retention has explicit RESTRICT edges from `instance_deletion_obligations`,
  `deletion_finalization_authorizations`, `instance_retirement_receipts`, and
  `instance_recovery_guards` (through `run_outcomes`). Artifacts and ordinary
  invocation/outcome rows are owned cascades. Execution, migration checkpoint and
  service-target rows need semantic eligibility checks before any cascade: a
  cascade is not evidence that their outstanding obligations may be discarded.

Content-coordination implementation must audit publication witnesses between
`put_runtime_content` and `persist_revision_internal`, unpinned verification, and
execution/migration materialization. Adding a lock only around blob unlink is
insufficient. Existing methods that hold a database transaction while verifying
content must not introduce the reverse content-lock acquisition order.

## Delivery state

- S0: approved contracts and exact-V8 reference inventory recorded and self-reviewed.
  Concrete coordination, reference guards and native deletion paths are self-reviewed.
- S1: V9 activation, exact predecessor upgrade, writer fencing and content coordination implemented and verified.
- S2: Artifact delivery/deletion implemented, verified and integrated into local develop;
  not published.
- S3: transactional Snapshot/Revision deletion and independent provenance implemented and verified.
- S4: Run deletion, explicit Artifact cascade intent and evidence guards implemented and verified.
- S5: explicit GC, read-only preview, native safe removal and cross-object tests implemented;
  final full-CI acceptance passed.

Owning requirements: PR-REQ-0072 through PR-REQ-0076,
[PR-REQ-0341 through PR-REQ-0344](../spec/behavior/managed-object-lifecycle.md), and
[PR-REQ-0345](../spec/persistence/persistence-schema-v9.md).

Earlier Artifact acceptance is recorded below. Historical acceptance is not
evidence for the subsequent whole-lifecycle changes. The separately authorized
local integration is recorded below; it does not authorize remote publication.

## Design review findings

The V8 writable-admission table constrains the schema version to 8. V9 therefore
requires an exact replacement with CHECK 9, not just a user_version change.
Its schema contract records this correction; the whole-lifecycle feature now writes V9.

Exact V4, preserved by V8, already fixes the Artifact maximum at 536,870,912 bytes.
An initial oversized export fixture correctly failed at Artifact publication.
The corrected acceptance case exports the exact existing maximum; this work does
not raise it. The Artifact repository now names its own same-valued constant
instead of borrowing the Managed Input constant. This is a policy-ownership
correction without a DDL or capacity change.

Artifact delivery reuses existing SQLite consistent reads, independent deletion,
protected operation staging and same-directory no-clobber publication. Its finish
path intentionally does not use the Managed Input size check. Publication errors
carry the final-name boundary so a post-publication durability error cannot claim
that no destination exists. Linux temporary-name cleanup occurs after that boundary.

The Artifact slice was independently verified on V8 before the remaining lifecycle
implementation. Its historical acceptance below is retained without presenting
that partial checkpoint as evidence for V9 or GC.

## Historical Artifact-only acceptance - 2026-09-17

Source-manifest SHA-256:
`c8b0b2fdac0d617901e067610b928018d831c76bf25018b2ce6c0158cb1be893`.
All 325 source files matched before the configured-remote run and after the
separately resumed documentation gate. The workspace uses its own Cargo target
directory on the persistent supported ZFS test filesystem.

**Passed:** the Rust/conformance portion of `cargo xtask ci`: Frozen vector and
oracle checks, bidirectional traceability, formatting, workspace Clippy, **408**
library tests, the new Artifact CLI process test, existing CLI process groups
(**4**, **10**, **2**), **43** system tests and **40** xtask tests. No test was skipped.

**Failed (environment only):** that combined `cargo xtask ci` invocation stopped
at documentation typecheck because pnpm rejected a cross-project node_modules
symlink. The task-created link was removed without modifying its target; this
test project received its own frozen-lockfile dependencies.

**Passed, separately resumed on the same source:** documentation typecheck,
all **22** documentation/link checks and `cargo xtask docs-build`, including the
Docusaurus production build. This is completed split-gate evidence, not a claim
that a second fresh all-in-one `cargo xtask ci` exited successfully.

**Passed on Windows:** Artifact-focused library group **9**, publication group
**3**, the actual Artifact CLI process test **1**, workspace Clippy and formatting.
Groups include test workers and existing regression cases, not that many new
normative promises. The exact 512 MiB streaming case, cross-process deletion, and
process exit before/after final-name publication are included. This is not a full
Windows product-suite claim.

Build tools: Rust/Cargo 1.98.0, Node 24.19.0 and pnpm 11.21.0. Remote development
and test debug information were disabled; Windows used its ordinary debug profile.

An earlier shared-Cargo-target attempt was explicitly discarded: its cached
xtask embedded the old workspace path. It was stopped, and no result from that
attempt is counted. The over-limit fixture failure described above was a corrected
test assumption, not authorization to change the Artifact capacity contract.

After that snapshot, a bounded CLI diagnostic correction maps the existing typed
MissingRun error to an explicit missing-parent message instead of a generic
failure, with a corresponding assertion. No persistence, publication, capacity,
dependency, DDL or vector behavior changed. **Passed on the final diagnostic
revision:** remote formatting/Clippy, the Artifact group **6**, publication group
**3**, actual CLI test **1**, and conformance/traceability; Windows Clippy and the
affected Artifact group **5** also passed (the unchanged large-file case was
already verified above). Earlier broad Rust evidence is reused for unaffected
behavior, not presented as a second fresh full-suite run.

The final status-only closeout receives documentation checks/build separately.
Self-review is not independent review.
The persistent remote test workspace and caches are retained. No preview service,
product daemon, deployment or network-facing product capability was introduced.

## Whole-lifecycle acceptance

**Passed:** fresh configured-remote `cargo xtask ci`, exit 0, against source-manifest
SHA-256 `2d418f16f303b4044311570f2b7b9e69dab34fc0733190dfcb2c53e548474adb`.
All **334** source files matched before and after acceptance, on the persistent
supported ZFS workspace with its own Cargo target directory.

- Rust library: **428 passed**, no failed/ignored tests.
- Actual CLI groups: Artifact **1**, Action **4**, Migration **10**, retirement
  **2**, whole-lifecycle **2** passed. Worker tests are not extra product promises.
- System tests: **43 passed**; xtask tests: **40 passed**.
- Frozen conformance/vectors and independent Node oracles, bidirectional
  traceability, formatting, all-target/all-feature Clippy passed.
- Documentation typecheck, all **22** documentation/link checks and the Docusaurus
  production build passed. No deployment was performed.

**Passed on Windows:** the final lifecycle group **18**, whole-lifecycle CLI group
**2**, Artifact CLI **1**, Migration CLI **10**, the targeted predecessor/version
regressions, formatting and workspace Clippy. Earlier Artifact/512 MiB and native
publication evidence is retained for unchanged paths; this is not a complete
Windows product-suite claim.

The acceptance includes whole-object deletion and cross-process commit crashes,
explicit Artifact cascade intent, receipt/recovery/pin guards, exact populated
V8-to-V9 preservation, stale-admission fencing, read-only preview, missing-catalog
refusal, actual eligible reclamation, partial I/O reporting, hard links,
symlink/FIFO refusal, native namespace replacement and interrupted Linux claims.
Domain facts own deletion eligibility; persistence owns observation/transactions.

Earlier whole-lifecycle attempts identified historical-fixture DDL spelling and
predecessor-version assumptions. Those fixtures now use exact historical DDL and
explicit test-only intermediate upgrades. Historical requirements/tests were not
weakened; PR-TEST-0300 follows the current binary's V8-to-V9 gate with its stable
test ID, while V7-to-V8 retains separate historical verification.

Only informative completion/handoff documents change after this acceptance.
Runtime, tests, dependencies, normative contracts, DDL and vectors remain unchanged;
the affected documentation checks/typecheck/build are rerun for closeout. This is
not another fresh Rust-suite claim after those editorial updates. Self-review
remains distinct from independent review. The persistent test workspace and caches
are retained; no new preview service or product daemon is running.

## Local integration

The operator authorized closeout, commit and merge after acceptance. Implementation
commit `96750aa89ad5a023ebc2451ba076aaf782c51959` was merged without conflicts into
`develop` as `596476ffbd4e38b60f33a2b273fab6aa57e921e9`. The merge tree exactly
matches the verified feature tree. The implementation branch is retained.

Integration closeout updates informative baseline/navigation pages and their
documentation assertions only. Runtime, Rust tests, normative behavior, DDL,
vectors, dependencies and toolchain inputs are unchanged. The earlier full-CI
and Windows evidence is reused; the affected traceability, documentation tests,
typecheck and production documentation build are rerun. This is not a second
fresh full product-suite result. Documentation generation is rechecked rather
than inferred from merge success.

**Passed:** integration-closeout traceability, all 22 documentation/link checks,
documentation typecheck and the Docusaurus production build. The unchanged
runtime retains the full-CI and Windows evidence recorded above.

The local integrated persistence baseline is V9 with explicit exact-V8 upgrade
only. The next planned completion scope is Snapshot Capacity and Restore Workflow;
its detailed design/implementation remains a separately scoped task. No push,
release, Pages workflow change or deployment occurred.
