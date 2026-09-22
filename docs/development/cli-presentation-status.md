---
title: CLI Presentation Implementation Status
---

# CLI presentation implementation status

**Status: D S0-S4 implemented, verified and integrated into local `develop`;
not pushed or published.**

The [baseline](./design-notes/cli-presentation-baseline.md) and
[JSON contract](../spec/contracts/cli-json-v1.md) record the approved scope.
The matrix below records the delivered command families and explicit stream
exceptions. Verification evidence is recorded separately below.

## Complete public command inventory

`JSON` means a Pactrun-owned response. `Raw` means unchanged Input bytes without
a success envelope. `Conditional` means JSON planning and terminal-free execution;
actual execution requiring Hook output/interactive streams is rejected before
execution side effects. Internal shell-loader/adapter dispatch and the `hook`
protocol helper are not public management presentation commands.

| Commands | Target presentation | Result facts / human review |
| --- | --- | --- |
| help; version; pack generate-id | JSON | Help content, product version, generated identity; clear human usage |
| pack install; revision export | JSON | Exact identity, metadata outcomes, migration relations, published destination; warning visibility |
| instance create; list; show | JSON | Exact identity, configuration completeness, guard and Inputs; grouping and empty lists |
| instance resolve-manual-recovery | JSON | Resulting state token; no execution eligibility inference |
| input list; set; delete | JSON | Bindings and resulting state token; distinguish absent/retained/Secret |
| input export | JSON / Raw for --output - | Export facts for file destination; no payload wrapper |
| action list; show | JSON | Authored declarations; no arbitrary Hook output |
| invoke, including --plan | Conditional | Plan checks, Run identity and outcome, known failure facts |
| run show; reconcile | JSON | State, diagnostic policy/provenance and recovery results |
| run list | JSON | Selector, items, exact continuation; full IDs regardless of --no-trunc |
| revision list; show; metadata show | JSON | Exact identities, declarations and permitted metadata; paging and grouping |
| revision alias show; set; clear | JSON | Typed current/desired state and mutation outcome |
| revision note show; set; clear | JSON | Typed optional local note and mutation outcome |
| revision trust show; set; clear | JSON | Typed optional trust and mutation outcome |
| instance history list; show; instance deletion list | JSON | Current versus recorded identity, management and deletion state |
| revision delete; run delete; snapshot delete | JSON | Deleted/already absent/blocked; never conflate blocked with success |
| run artifact export; delete | JSON | Authorized operation result; original payload unchanged |
| storage upgrade; gc, including --plan | JSON | Schema or collection facts; partial collection failures remain visible |
| service-storage list | JSON | Active/retained declarations and preserved allocation facts |
| resource list; show; observe; locate | JSON | Typed observation/access facts; preserve location disclosure boundary |
| service-storage detached list; show; discard | JSON | Handoff state, explicit location reveal and discard outcome |
| instance delete; abandon; deletion show; deletion confirm-complete | Conditional for execution, JSON for inspection/confirmation | Typed deletion phase/work/outcome; keep external service state distinct |
| instance migration-paths; migrate, including --plan | JSON discovery / Conditional execution | Exact paths, cursor, Plan and per-edge known results |
| snapshot list; show; verify; import; export | JSON | Inspection, verification and transport facts; no archive serialization |
| snapshot capture; restore, including --plan | Conditional | Plan and managed Run/Snapshot result |
| instance create --restore-from | Conditional | Created Instance and known Restore result, including partial failure |

All 62 canonical command names have explicit schema cases and typed projections.
Coverage combines the schema drift gate, positive/negative JSON journeys and the
existing backend/system suite; it does not claim a distinct end-to-end test for
every possible field combination. Parser spellings, rather than the abbreviated
grouping above, remain authoritative. Human-only truncation does not truncate
JSON facts or cursors.

## Delivered slices

- S0: canonical JSON V1 contract and checked-in schema, complete command/channel
  inventory, PR-REQ-0358 through PR-REQ-0360 and existing PR-REQ-0083/0120 linkage.
- S1: leading format selection before parsing/startup errors, one typed envelope,
  explicit failure/absence representation, native paths without lossy conversion,
  and preserved stable error references without parsing prose.
- S2: management, catalogs, history, metadata, service inspection and Plans use
  explicit projections of the same backend facts. Human empty collections are
  explicit, while existing useful catalog grouping/pagination is retained.
- S3: terminal-free execution and transport results, known Run/creation/collection/
  publication partial facts, raw Input preservation and pre-execution refusal of
  Hook terminal-stream combinations. Legacy sensitive Run text remains withheld;
  retained diagnostics remain attributed to the Hook and follow retention policy.
- S4: schema/negative cases, CLI and real-process acceptance, complete configured-
  remote CI, source-manifest verification and documentation closeout.

Only presentation and private error plumbing changed. V11 persistence, frozen
identity/transport/Hook contracts, authorization and execution/recovery semantics
are unchanged. No new result-file destination, event stream, API service, usage
guide project or publication workflow was added. Schema generation/validation
dependencies are test-only, not product runtime dependencies.

## Verification

The complete passing `cargo xtask ci` used this source archive SHA-256:

```text
41b88bb4dd22180b7232c7d283da24146f033143bb87c51a504ed25dabfdbad0
```

All 400 per-file source checks passed before and after the gate on the persistent
supported remote filesystem. The target directory is isolated to this source
workspace, and the xtask executable's embedded workspace was checked before use.
IPC temporary files use a short dedicated path on the same persistent filesystem.

| Status | Evidence |
| --- | --- |
| Passed | Windows formatting and workspace/all-target/all-feature Clippy |
| Passed | Windows CLI library selection: 50; actual Artifact/Catalog/Migration/Pack process suites: 15 |
| Passed | Complete configured-remote `cargo xtask ci`: conformance, formatting, Clippy, workspace tests, site typecheck and production build |
| Passed | Linux library: 495 passed, four existing explicit ignores; system: 70 passed; xtask: 40 passed; all CLI process suites passed |
| Passed | All 25 documentation/link/catalog checks; schema is included unchanged in the generated text edition and its source digest |
| Passed | Affected requirements have direct bidirectional PR-REQ/PR-TEST linkage, including actual post-publication failure and broken-response evidence |
| Not run | The four pre-existing explicitly ignored library cases; no new ignore, assertion relaxation or product-timeout change was introduced |

The final remote test profile used `CARGO_PROFILE_TEST_DEBUG=0`,
`CARGO_PROFILE_TEST_OPT_LEVEL=1`, `CARGO_PROFILE_TEST_DEBUG_ASSERTIONS=true`,
`CARGO_PROFILE_TEST_OVERFLOW_CHECKS=true` and `RUST_TEST_THREADS=1`. Compiler
evidence records optimization with debug assertions enabled; overflow checking
was explicitly configured on. Serialization of the harness does not serialize
the explicit concurrency and cross-process scenarios inside tests.

An earlier unoptimized run passed all 495 library tests but failed six system
tests because their old adapters treated human empty-result hints as data or
required empty stdout. The adapters now assert typed empty JSON collections (or
recognize the exact human empty-Instance hint), preserving and strengthening the
no-object/no-extra-reconciliation checks. The focused original-profile regression
passed, followed by the complete passing gate above. This was a test-presentation
adaptation, not removal of a product assertion.

Other preliminary attempts are not passing gate evidence: a shared target reused
an xtask binary embedding an earlier workspace, and an overlong custom TMPDIR
produced 115-byte Unix socket paths rejected by the OS. These were corrected by
source-isolated artifacts and a short temporary path. Runs superseded during
partial-result repairs were stopped and are not reported as complete passes.

This results-only closeout changes Markdown status/navigation explanations, not
tested Rust code, schemas, fixtures, dependencies or build inputs. The full runtime
gate is reused for those unchanged inputs; final documentation/link/traceability
and remote site typecheck/build are checked separately, not called a fresh full CI.

## Git, retained resources and next work

After explicit operator authorization, implementation commit
`faa41593affc35559333c5365137709a128afa64` on
`feature/cli-presentation-json` was merged into local develop with no-fast-forward
merge `2b69837197c8cd7216bbc51a27677dfe02d20440`. Both have tree
`1cb7ec3ac33d7aab75392813b502295527cb72d6`. The merge had no conflicts and
introduced no runtime changes. All 400 delivery-manifest entries matched before
commit, and staged bytes matched the reviewed working tree. No consumed Git-derived
build input was found, so the recorded runtime evidence applies to the identical
implementation and merge trees; this is reused evidence, not a new full-CI run.

The subsequent integration closeout changes status Markdown and its navigation
assertion only. Documentation/link/traceability checks and remote site typecheck/
build are rerun for that closeout. It does not modify Rust, schemas, fixtures,
dependencies or runtime build inputs.

Feature branches are retained. No push, release or publication was performed.
Existing unrelated untracked Pages configuration, the user archive and local
agent guidance were excluded from commits and preserved.

Source archives, manifests, logs, isolated Cargo artifacts, dedicated test scratch
and generated documentation remain in the configured remote test workspace.
Local evidence and the isolated dependency cache remain under target/. No preview
server was started or stopped and no network exposure or sharing was changed.

E (Versioning and Baseline Consolidation) is the next design milestone and still
requires its own S0 approval. D integration does not authorize E implementation.

This record grants no authorization for further Git mutations, push, release or
publication.
