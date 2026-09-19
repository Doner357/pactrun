---
title: Execution Diagnostics and Instance Observability
---

# Execution diagnostics and Instance observability

**Status: Complete. Implemented, verified and integrated into local develop; not published.**

## Approved baseline and delivery

The operator approved the complete milestone on 2026-09-19, prioritizing post-run
traceability and debugging without replaying side-effectful operations. S0-S3 are
implemented and verified. The old blanket non-retention and text-suppression rules
were explicitly revised; execution ownership, outcome arbitration, no-replay,
Input/Secret export authority and operation commit semantics remain unchanged.

- New accepted Hook diagnostic, completion and protocol-error explanations are
  retained and presented by default. Core-owned projections do not expose Input
  or parameter values. Hook authors remain responsible for their own text; no
  universal taint detector or Secret vault is claimed.
- --no-retain-hook-text disables persistence, not live presentation, for the
  invocation. It does not erase earlier Runs. Raw terminal streams are not recorded.
- Diagnostic prefix/suffix budgets are 1 MiB/1,024 and 3 MiB/3,072 events. Terminal
  explanations have a separate newest 1 MiB/256-event budget. Messages retain
  UTF-8-safe first/last 32 KiB portions with an exact truncation split.
- Protocol queues, collection and output are bounded. Pending evidence is committed
  incrementally; contention retries and shutdown waits are bounded. Diagnostic
  failures cannot change outcomes or authorize replay. Missing/unclosed evidence,
  known omissions and unavailable receipt timestamps are explicit.
- Run inspection combines evidence, historical facts and the current guard in one
  read snapshot. Instance inspection separates identity, exact Revision, required
  Input completeness and recovery guard; completeness is not general eligibility.
- The feature initializes V11 and explicitly upgrades exact V8/V9/V10. Legacy text
  is not retroactively exposed. Evidence has the Run lifetime, not a new GC root.

## Shared obligations

The [shared obligation inventory](./shared-obligation-inventory.md) enumerates
352 numbered rules and three unnumbered introductory constraint groups. Other
unnumbered paragraphs remain owned by their enclosing rules. Cited tests are
linkage, not independent proof of every possible behavior. A's actual verification
is in the owning [diagnostic contract](../spec/behavior/execution-diagnostics.md),
[V11 contract](../spec/persistence/persistence-schema-v11.md) and updated Instance,
Run and protection rules. B-E retain their individually assigned follow-ups.

## Verification evidence

The complete configured-remote gate passed on the source archive with SHA-256:

```text
9990145d7e76bba650d6c9a025422b5bc92ceef0017459574ed9efbdb2dd4ca2
```

It ran in the persistent Pactrun test workspace, not /tmp or tmpfs. The temporary
build setting CARGO_PROFILE_TEST_DEBUG=0 omits test-executable debug symbols to
reduce real Hook fixture copying. It does not change product source, test filters
or timeout values. No tests were disabled beyond the pre-existing ignored cases.

| Status | Scope |
| --- | --- |
| Passed; fresh full gate | cargo xtask ci: conformance, cross-language vectors, bidirectional traceability, formatting, workspace/all-target Clippy, workspace tests, site typecheck and build |
| Passed | Primary library: 454 passed; three pre-existing explicit capacity tests ignored, not claimed as executed |
| Passed | Linux system suite: 70 passed; real CLI e2e: 4; Migration selection: 10; retirement CLI: 2; lifecycle CLI: 2; Artifact CLI: 1; xtask: 40 |
| Passed | Windows PowerShell 7/5.1 Shell acceptance: 23 passed, run serially on the final implementation |
| Passed | Focused bounded retention, UTF-8/unknown-time persistence, process-loss evidence, opt-out, write failure, flood/deadline, blocked stderr, schema upgrade and Instance guard/completeness checks |
| Passed | Site source checks: 24; typecheck and Docusaurus production build, including compatibility anchor preservation |
| Passed; documentation-only closeout | Final source/document/traceability checks and configured-remote site typecheck/build; unchanged runtime reuses the full gate above |
| Completed after explicit authorization | Local implementation commit and no-fast-forward develop merge; the feature and merge trees are identical |
| Not performed | Push, release, Pages workflow changes, deployment or implementation of B |

Earlier attempts are not passing evidence: the first remote library candidate had
429 passes, 17 failures and three ignored tests. Failures identified stale retention
assertions, current-schema counts and fixture qualification; one V11 admission
version pairing required repair. Later preflights exposed old integration fixture
assumptions and a remaining default-retention setup. Those were corrected without
weakening core protection or restoring discarded diagnostics. One parallel Windows
attempt hit the harness's 30-second watchdog; the final serial run passed without
changing product timeouts. A known-failed intermediate CI run was stopped before
retesting. None of these attempts is represented as a successful full gate.

This record and navigation are documentation-only closeout after the tested source.
The full runtime evidence is reused for unchanged code; the final documentation
recheck is recorded separately rather than described as another fresh full CI.

## Integration and retained resources

After explicit operator authorization, the implementation was committed as
`cc4b8c1958aff88434189810351b238d8de64bf3` on
`feature/execution-diagnostics-observability` and merged into local develop with
no-fast-forward merge `1c881c2d8913968db4bbdbfb6fa43ddab07fe0e6`. The feature and
merge trees were compared and are identical. V11 is now the integrated baseline.

This integration closeout changes documentation and its navigation checks only.
Unchanged runtime, tests, fixtures, dependencies and build inputs reuse the full-CI
evidence above; no fresh full-CI run is claimed for an identical Git merge.
Integration documentation is checked separately, including source/traceability
checks and the configured-remote site typecheck/build. B, Object Catalog/History/
Metadata, is next, beginning with its own S0; no B implementation is authorized.

Local evidence remains under target/. Source archives, test logs, caches and the
generated documentation remain in the dedicated remote execution-diagnostics
workspace. Its site dependencies were installed from the unchanged lockfile into
its own node_modules, without modifying the older preview's modules directory.
No documentation preview server was started or stopped. Existing unrelated
untracked Pages configuration and the user archive remain untouched.
