---
title: E Implementation Status
---

# E implementation status

**Status: In progress; NOT complete and NOT merged to develop.**

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
| Native integration | Program-only normal/test launchers and a metadata-only source selector are implemented. Major-scoped clones, real Git tracking/origin-HEAD checks, immutable exact refs and monotonic local source publication have focused tests. Actual Scoop/Homebrew candidate acceptance is still pending. |
| Artifact tooling | Source-qualified build provenance, deterministic archives, platform/flavor manifests, explicit formal-default gates and local-only publication tooling implemented; real candidate artifacts and platform dependency inspection are pending. |
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
