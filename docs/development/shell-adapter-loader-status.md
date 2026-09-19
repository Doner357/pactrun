---
title: Shell Adapter and Loader Status
---

# Shell Adapter and Loader implementation record

**Status: S0-S6 implemented, verified and integrated into local `develop`.
Local commit/merge authorized on 2026-09-18; not pushed, released or published.**

The operator approved the [baseline](./design-notes/shell-adapter-loader-baseline.md)
and continuous S0-S6 execution on 2026-09-18. Work was prepared on
`feature/shell-adapter-loader`, starting from local `develop`. Existing unrelated
untracked files are excluded. On 2026-09-18 the operator separately authorized
milestone closeout, local commit and merge into develop. Push, release, deployment
and changes to the unrelated Pages workflow remain outside that authorization.

## Authorized local integration

Implementation commit `4c294e0bfc3f6cb023d5a837764f898f1b0509d2` was merged into
local develop by no-fast-forward merge `5d90472d99a57e33b49d9da712eee891b1861066`.
The feature and merge trees are identical
(`3724f68cfeb690bb993dc2e484b07295daaabd85`); there were no conflicts or runtime
edits during integration. Existing untracked Pages and ZIP files were not staged.

The validated closeout manifest matched every file before the authorized
integration task. Its only pre-commit additions were documentation of that
authorization and the diagnostic presentation gap below. This subsequent
documentation-only closeout updates current navigation and records actual Git
integration; it changes no runtime, DDL, dependency, toolchain or identity vector.
Previously scoped full-CI and platform evidence is reused, not represented as a
fresh whole-suite run. Documentation/link, traceability and configured-remote
site checks cover these closeout changes. No push, tag or deployment is included.

| Slice | Status |
| --- | --- |
| S0 | Approved baseline, owning Core/YAML/Loader contracts and bidirectional test IDs recorded |
| S1 | Explicit V3 typed projection, canonical codec, independent Node vectors, content closure and exact-launch admission implemented |
| S2 | Ordinary scripts and ordered arguments execute without helper/bootstrap boilerplate on the four selected shells |
| S3 | Private socket/pipe helper IPC, typed requests, file interfaces, explicit registration and acknowledged risk implemented |
| S4 | Action, Migration, Capture, Restore, Cleanup and V2 transform journeys implemented and exercised |
| S5 | Boundary, malformed-frame, helper-loss, process-loss, timeout, terminal, concurrent-helper and byte-fidelity checks implemented |
| S6 | Complete remote CI candidate passed; fresh bounded-delta platform/contract checks passed; Core V3 status-only Freeze and documentation closeout completed |

The [format review](./design-notes/shell-loader-format-activation-review.md) records
the exact full-CI and later adapter-validation inputs and the bounded evidence
reuse. Core V3 is Frozen; YAML V3 remains Candidate. Local develop integration
does not imply release, publication, a formal product version bump, or completion
of the diagnostic presentation follow-up.

## Environment blocker, 2026-09-18

The operator subsequently authorized the bounded acceptance-process-only
`RemoteSigned` exception. Work resumed. The acceptance harness sets
`PSExecutionPolicyPreference` only for its child process; production Loader does
not set it or supply an execution-policy command-line flag. A read-only probe
confirmed the authorized process policy is RemoteSigned while CurrentUser and
LocalMachine remain Undefined for Windows PowerShell 5.1.

Read-only version and execution-policy probes found Windows PowerShell
5.1.26100.9444 with effective policy `Restricted` (all listed scopes Undefined),
and PowerShell 7.6.5 with effective policy `RemoteSigned` (LocalMachine).
A workspace-local script containing only `exit 0`, invoked with
`powershell.exe -NoProfile -NonInteractive -File`, was rejected with
`SecurityError` / `UnauthorizedAccess`. These observations concern the
current test host, not a product minimum-version or universal platform claim.

The approved plan forbids changing execution policy. The implementation must not
silently add `Bypass`, evaluate file contents as command text to evade the policy,
or claim the Windows PowerShell 5.1 script matrix passed without executing it.
No persistent policy scope, profile or host setting has been changed. The operator
authorized the test-process-only adjustment before the 5.1 acceptance runs.
Production Loader policy handling must remain unchanged by any test exception.

The approved bounded exception is `RemoteSigned` for the Windows PowerShell 5.1
acceptance process only, not CurrentUser/LocalMachine, not `Bypass`, and not a
shipping Loader flag. PowerShell 7 uses its existing host policy without this override.

## Verification before the environment pause

- Passed: `cargo test --offline --locked -p xtask
  candidate_or_frozen_metadata_and_traceability_are_valid -- --test-threads=1`
  (one focused metadata/traceability test).
- Passed: `node --test website/scripts/spec-docs.test.mjs
  website/scripts/text-docs.test.mjs` (22 documentation/link tests). The first
  sandboxed attempt failed to spawn test subprocesses with EPERM; the approved
  rerun outside the sandbox passed. Temporary test data stayed in the workspace.
- Blocked: Windows PowerShell 5.1 script execution under the unchanged policy.
- Not run: product implementation tests, remote full CI, website typecheck/build
  and the four-shell acceptance matrix. No remote connection or new remote
  background resource was established.

The preceding list is historical evidence before implementation, not current
availability. Development resumed after authorization.

## Capability and evidence inventory

| Capability | Verification |
| --- | --- |
| Core V3 and YAML V3; exact old-format rejection and identity separation | PR-TEST-0486, PR-TEST-0487, PR-TEST-0492, PR-TEST-0493, PR-TEST-0494, PR-TEST-0510 |
| No-helper scripts and special arguments | PR-TEST-0486, PR-TEST-0498 |
| Bound parameters, Input file access and non-retention of Secret text | PR-TEST-0502, PR-TEST-0507 |
| Workspace, repeated/concurrent helpers and Session isolation boundaries | PR-TEST-0499, PR-TEST-0500, PR-TEST-0503, PR-TEST-0511 |
| Explicit Action output registration and bytes larger than a protocol frame | PR-TEST-0499, PR-TEST-0512 |
| Capture descriptors/content, Restore and Cleanup | PR-TEST-0489, PR-TEST-0497, PR-TEST-0512 |
| Migration source view, required target outputs and atomic target boundary | PR-TEST-0490, PR-TEST-0495, PR-TEST-0507, PR-TEST-0514 |
| Exact risk acknowledgment and Open-risk failure | PR-TEST-0488, PR-TEST-0491, PR-TEST-0506 |
| Helper loss, script loss, Loader loss and timeout | PR-TEST-0496, PR-TEST-0504, PR-TEST-0505, PR-TEST-0513 |
| Terminal none/output/interactive and interactive input | PR-TEST-0501, PR-TEST-0509 |
| Handleable local errors without automatic replay | PR-TEST-0503, PR-TEST-0508 |
| Maximum-size Session through bounded private IPC without changing the wire limit | PR-TEST-0515 |
| Windows discovery never opens an ambient regular file as IPC | PR-TEST-0516 |
| Backpressure does not introduce an independent helper execution deadline | PR-TEST-0517 |

The runtime uses the unchanged V10 persistence schema. Core V3 dispatch adds new
explicitly versioned content, not a schema conversion, historic-row rewrite,
fallback reader or persisted Loader state. Core/Hook V1/V2 and Snapshot vectors
are unchanged. Snapshot import/export continues under its existing contracts;
this work does not invent a Pack bundle/import/export product capability.

## Findings corrected during implementation

- PowerShell's authorization manager rejects extended-prefix script paths.
  Loader uses an equivalent drive-absolute spelling only after rejecting ambiguous
  components and verifying canonical equality. No execution-policy bypass is used.
- Failed Capture cannot submit descriptors. Registration validates the success
  descriptor shape; failure completion submits an empty collection.
- Explicit failure with Open risk remains valid failure even when the script
  exits zero; transform receipts still do not clear risk or commit a target.
- Private reply envelopes need bounded overhead beyond a full-size Session.
  This does not enlarge the canonical 16 MiB Core frame limit.
- Core supervises the POSIX helper directory so Loader loss cannot orphan its
  socket while the execution owner remains alive. Windows pipe names are kernel-owned.
- Raw control integers are validated before binary64 conversion; fractional tokens
  that would round to an integer are rejected, not repaired.
- Windows discovery rejects regular-file/device/network locators before opening.
  A busy helper pipe waits under its execution owner's deadline instead of adding
  an arbitrary connection deadline; issued requests are never replayed.
- Two historical dispatcher tests treated V3 as an unknown source version.
  Their unknown-version cases now use unsupported V4; V2 invalid-field, duplicate,
  scalar-style and no-publication assertions remain intact.

No claim of independent runtime review is made. Self-review is engineering work;
the independent Node oracle proves only normalized canonical component/frame/digest
parity.

## Known follow-up: usable Hook diagnostics

The 2026-09-18 usage review identified a pre-existing observability gap. The
diagnostic helper validates and forwards a canonical diagnostic, but Core's
decoder discards its severity/code/message after validation. There is currently
no live diagnostic presentation or diagnostic history consumer. Successful
delivery therefore does not establish a usable operator diagnostic feature.

The original Run-detail goal in PR-REQ-0097 must be distinguished from the later
non-retention and structural-only presentation boundaries in PR-REQ-0283 and
PR-REQ-0285. This gap is not silently retired, treated as completed by negative
redaction tests, or automatically assigned to the next milestone. Presentation
and sensitive-text policy need a separately agreed follow-up with positive
end-to-end acceptance. The operator requested integrating this completed Loader
milestone first; this closeout does not change diagnostic behavior or those rules.

On 2026-09-19 the operator explicitly assigned this gap to A, Execution Diagnostics
and Instance State Observability, in the approved
[remaining capability plan](./remaining-capability-milestones.md). Live and post-run
usability and their retention/disclosure policy are A's responsibility. The task
records planning only; it does not change the historical Loader acceptance or
claim that diagnostics are now displayed or retained.

## Verification and retained resources

| Status | Scope |
| --- | --- |
| Passed; reused for unchanged components | Complete configured-remote `cargo xtask ci`: conformance, formatting, Clippy, workspace/integration tests, website typecheck/build. The primary library result was 444 passed with three pre-existing ignored tests; the system result was 67 passed. No ignored test is claimed as executed. |
| Passed; fresh after adapter hardening | Windows PowerShell 7/5.1: 21 `shell_loader_` system tests, six Loader unit tests and nine existing native process/terminal probes |
| Passed; fresh after adapter hardening | POSIX sh/Bash: 21 `shell_loader_` system tests, six Loader unit tests, 14 Core conformance tests and three YAML V2/V3 tests |
| Passed | Independent Node/Rust comparison of four V3 canonical components, full frames and checked-in digests; old Frozen vectors unchanged |
| Passed | Formatting, all-target Clippy and bidirectional requirement/test traceability |
| Passed | Status-only closeout: Core V3 metadata/conformance, documentation/link checks and configured-remote site typecheck/build |
| Completed after separate authorization | Local milestone commit and no-fast-forward develop merge, with identical feature/merge trees |
| Not run or authorized | Push, release, Pages/workflow changes, deployment and implementation of the next product capability or diagnostic follow-up |

The full-CI candidate and delivered-runtime manifest hashes are in the format
review. The complete CI log, checked manifests, private source archives, build
caches and generated documentation are retained in the configured remote's
persistent `pactrun-test-workspace/shell-adapter-loader` workspace. Local generated
evidence remains under `target/`. No new background server was started and no
existing preview service was stopped. The host PowerShell policies remain unchanged;
only the specifically authorized 5.1 acceptance child receives Process RemoteSigned.

The first full-suite attempt failed the two obsolete unknown-V3 test cases;
the corrected full candidate subsequently passed. Neither the first failed attempt
nor focused checks are presented as a fresh complete-CI pass on the later delta.

## Diagnostic follow-up delivery

The separately approved [execution diagnostics milestone](./execution-diagnostics-observability-status.md)
has now implemented and verified the follow-up on its feature branch. Its new
retention/disclosure rules do not rewrite the historical Loader acceptance above.
Git integration and publication remain separately tracked.
