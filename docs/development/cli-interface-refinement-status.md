---
title: CLI Interface Refinement
---

# CLI interface refinement

**Status: Implemented, verified and locally integrated, 2026-09-23.**

The [short-ID integration record](./short-id-selectors-status.md) records the
subsequently authorized commit and `develop` merge that includes this CLI work.

## Scope

The approved work separates concise human presentation from complete machine
inspection, and adds ephemeral noninteractive Hook output delivery. The owning
rules and schemas are in the [CLI contract](../spec/contracts/cli-json-v1.md).
[Design references](./design-references.md) record CLIG, GOV.UK, Git, Docker,
Terraform and JSON Lines, including the specific principles adopted.

- Human lists and previews omit fixed disclaimers and internal bookkeeping.
  Detailed diagnostics remain available through Run inspection.
- Machine catalog projections include public definitions, public defaults,
  current metadata and full inspection facts. Sensitive defaults stay redacted.
  Related Revision identities remain exact and unavailable Revisions explicit.
- JSON retains `pactrun.cli.v1`; JSONL uses `pactrun.cli.events.v1`.
- Output bytes use separate Base64 streams and per-Hook offsets. Delivery is
  independent of historical Hook text retention and uses a leased, segmented
  temporary spool. Default disconnect preserves execution ownership; the explicit
  JSONL flag requests ordinary cancellation on a confirmed broken pipe.
- Interactive machine execution and JSONL raw-stdout export are refused before
  operation effects. Existing human and JSON raw Input export remains available.

No persistence schema, identity, admission, recovery or Pack-authoring format is
changed. Existing IPC initialization regressions remain covered.
The initial verification checkpoint performed no service deployment, release,
commit or push. The later local integration is recorded above.

## Verification record

The configured Linux `cargo xtask ci` completed with exit code 0 against source
archive SHA-256
`061a41a4c3ff543a0a76f9216417f1c97a761fb36d471319283ca1e1853f56a7`.
This archive includes the final runtime, schemas and regression tests, before
this documentation-only verification-record update.

| Check | Result |
| --- | --- |
| Linux library tests | Passed: 509; existing ignored: 4 |
| Linux executable system tests | Passed: 73 |
| Other Linux integration tests | Passed: 23 |
| xtask tests | Passed: 40 |
| Format, Clippy and cross-language conformance | Passed |
| Documentation tests | Passed: 25 |
| Docusaurus typecheck and production build | Passed |
| Native Windows CLI regression group | Passed: 58 |
| Windows-specific executable tests | Passed: 5 |
| Windows Migration integration tests | Passed: 11 |
| Windows focused stream/spool, Shell Loader lifecycle and affected query tests | Passed |
| Final Windows workspace/all-target Clippy and format checks | Passed |

The Windows retirement group (4 tests) was rechecked after sharing the same
read-only projection between human and machine inspection. The affected fresh-
process Instance, Action and Snapshot scenarios were also rechecked individually.
This is focused native Windows coverage, not a separate Windows full-suite run.

Linux used `CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_PROFILE_TEST_OPT_LEVEL=1`,
debug assertions and overflow checks enabled, and `RUST_TEST_THREADS=1`.
The four pre-existing ignored capacity/RSS acceptance tests were not run in this
task. They cover the small/medium/beyond-former-ceilings Snapshot profiles and the
large Pack round trip. Service deployment and long-running service qualification
were outside this task.

The first full attempt and the subsequent system preflight exposed assertions
bound to the former human wording/layout. Those checks now inspect the appropriate
preview, detailed Run or typed JSON view while retaining no-execution, readiness,
recovery and secret-disclosure assertions. The final complete gate above passed.

The focused regression coverage includes binary/no-newline output, diagnostics
with retention disabled, event ordering and offsets, live delivery, receiver
disconnect, explicit cancellation, slow consumers with timeout, spool failure
and cleanup, same-Revision metadata updates, read-only queries, and Shell Loader
Action/Capture/Restore/Migration/Cleanup execution including a two-edge chain.

The dedicated persistent Linux workspace is
`/home/test/pactrun-test-workspace/cli-interface-HImYbPZP`.
It has its own source, build output, website dependencies and test resources.
The completed CI process exited, and a workspace scan found zero remaining
`staging/session-*/delivery` directories. The persistent source/build workspace,
dependency caches and verification logs are retained for development. No new
background service or public listener was started.

Evidence lives under that workspace's `target/`: `full-ci.log`, `full-ci.exit`,
`ci-source.sha256`, `focused-final.log`, and the retained first-attempt logs.
Local Windows evidence is in the repository's `target/cli-*-final.log` files,
with dedicated machine-delivery, spool and lifecycle logs. Documentation-only
closeout uses traceability, typecheck and docs-build checks without rerunning the
unchanged runtime suite.

## Documentation impact

Updated CLI contracts and schemas, design references, command behavior,
execution-diagnostic disclosure, README and planned-guide entry wording.
Implementation history remains in the existing development records; this task
does not write the deferred user tutorials.
