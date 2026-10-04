---
title: Running Executable Lifetime Fix
---

# Running executable lifetime fix

**Status: Implemented and verified focused fix, 2026-09-26. Local integration
is scoped to feature/e-versioning-design; publication remains separate.**

## Scope and authorization

The operator authorized a focused fix on `feature/executable-lifetime-fix`,
based on `feature/e-versioning-design` at
`0117987786fab1a9b31f3472dd821e035a30321d`, followed by local merge back to that
design branch once Scoop and Homebrew validation succeeds. The independent
worktree preserves the design branch's pre-existing uncommitted documents,
workflow and archives. This is not an E baseline reset, a release from main,
publication, or authorization to push.

## Implementation

The existing [Shell Loader contract](../spec/contracts/shell-loader.md) says
PACTRUN_EXECUTABLE identifies the running executable. Linux self-launch sites
now use a process-scoped `/proc/<pid>/exe` locator rather than reopening the
removable installation pathname. The locator is deliberately not canonicalized
back into that pathname. Non-Linux lookup retains std::env::current_exe.

The caller remains alive while its child/Hook uses the locator. It is not a
durable identifier, a Session token, an installed-version selector or a recovery
mechanism. No fallback selects a different installed executable when discovery
fails. Existing private IPC, authority, owner-loss, cancellation and completion
handling remains binding. Namespace/procfs permission support must be qualified
for supported deployment environments; no promise is made for arbitrary detached
use or PID reuse after the valid execution lifetime.

Self-launch recognition, materializer launch, interactive adapter launch and
the loader's script environment share the same crate-private helper. No updater,
manager fork, retained executable cache, new dependency, schema/format change,
unsafe-code expansion or service-data conversion is introduced. Package managers
may remove their program files without the active operation looking up a newer
program at the old filename. New invocations after uninstallation are not promised.

## Regression coverage

- PR-TEST-0616 re-executes an unlinked running image after replacing its original
  pathname with a different executable; it rejects the replacement image.
- PR-TEST-0617 verifies delayed real helpers across unlink/replacement in none,
  output and interactive terminal modes, and checks normal locator expiry.
- Existing shell-loader tests cover authority, terminal isolation, failure,
  owner loss, cancellation, completion and output behavior.
- Actual isolated Scoop/Homebrew operations exercise acquisition, hashes,
  version progression, pinning, coexistence, explicit selection, in-flight
  operations and non-destructive program removal using the fix candidate.
- Fresh source-qualified configured-remote `cargo xtask ci`, focused Windows
  execution and Clippy, and relevant documentation checks precede integration.

## Verification results

| Check | Result and scope |
| --- | --- |
| Fresh configured-remote `cargo xtask ci` | **Passed:** conformance, formatting, Clippy, workspace/system tests, documentation/traceability, website typecheck and production build on the fix candidate. |
| Rust suites in that full gate | **Passed:** library 531, system 75, xtask 40, plus the other executable suites. The helper worker is not a second independent identity scenario; focused reruns are not additional coverage. |
| Windows focused system suite | **Passed: 23** shell-loader cases on the fix worktree build. |
| Windows Clippy | **Passed:** workspace/all-target/all-feature checks, warnings denied. |
| Scoop 0.5.3 isolated acceptance | **Passed:** selection/pinning/progression, separate installations, hash refusal, native shim entry, explicit reset, real Hook update/cleanup/uninstall and data sentinels. |
| Homebrew 7.0.6 isolated acceptance | **Passed:** selection/pinning/progression, separate installations, hash refusal, native symlink entry, explicit unlink/link selection, real Hook update/cleanup/uninstall and data sentinels. |
| Existing capacity/RSS ignores | **Not run:** the four pre-existing explicit ignores; none was added and no assertion, product timeout or coverage was weakened. |

Scoop checks normal active-process deferral rather than claiming an update has
completed while it is skipped. After the real operation ends, update selects
the new test package, cleanup removes the old executable and uninstall removes
the selected program. Busy cleanup can return a failure while preserving the
active executable; the successful follow-up is verified separately. The native
shim is exercised, not only an unpacked binary path.

Homebrew's default automatic cleanup stays enabled in the default-update run.
The old executable is actually removed while the Hook is active, yet its helper
and admitted Run complete successfully. A separate manual-cleanup setup disables
automatic cleanup only to retain an old version long enough to start a second
operation, then explicitly deletes that version while it runs. Active uninstall
also succeeds without breaking the Hook. Changing links between retained exact
packages preserves the active old process; atomic multi-command relinking and
all possible concurrent-launch schedules are not claimed.

Package manifests are local fixtures, with publisher-selected versions; this is
not a proof of GitHub discovery or a complete release-selection service. Real
package labels `0.1.0-probe.1` and `.2` contain identical fix-candidate bytes within
each run. They isolate installed-file lifetime, not data-schema downgrade or a
real old/new formal-product transition. The normal product version is unchanged.
The path-dependent version-selection fixture is not presented as having Pactrun's
fix: active-lifetime acceptance uses the actual candidate executable. Original
prototype results are not substituted for these fresh branch/platform checks.

## Source-qualified evidence

The full-gate input manifest contains 416 selected source/build/Spec/site inputs.
They match before and after the gate; the package-tested Linux executable hash
also matches after full CI. Builds identify this fix's isolated source/worktree,
not a reused executable from the prototype. Test optimization is 1, debug info 0,
debug assertions and overflow checks enabled, and one test thread. Data fixtures
stay on the configured persistent filesystem; only IPC temporary paths are short.

| Input/artifact | SHA-256 |
| --- | --- |
| Full-gate source manifest | d166aa91fe53674ac219874a6cfdffc28807dde5f8891702b424e502ac6991d2 |
| Windows candidate executable | 825475bc156d40b82c5dccfd588205c8ddd5b447d8e9b58c6c82564e5337578e |
| Linux candidate executable | f2281c14d5a36860fda1b237a72ef8af1e885be9e81ce45df1a6e97cfa716f6a |

Evidence is retained under local `target/executable-fix/evidence/`, with command
logs, return codes, source manifests, artifact hashes and per-run assertions.
Actual manager commands execute in isolated roots under `target/pm-poc/` locally
and the dedicated persistent remote test workspace. Manager source is pinned to
Scoop b588a06e41d920d2123ec70aee682bae14935939 and Homebrew
570982948a8a194f0f42f43f4a5bce2d1c9f64cb. Homebrew uses a custom test prefix; this
does not certify its default prefix, minimum OS, every filesystem or future
manager releases. Successful file-access samples are not a blanket production
service certification.

The full gate precedes the final update to this informative result record.
Runtime, tests, fixtures, dependencies and toolchain inputs remain unchanged;
documentation closeout is rechecked separately. Local integration must preserve
the original dirty-file hashes and match the reviewed fix tree. An identical
merge does not turn reused runtime evidence into a second fresh full-CI run.
This is self-review plus recorded automated/platform evidence, not an independent
review claim. Exact implementation and merge commits are available in Git history
and the final task handoff, rather than a self-referential commit hash in this file.

## Boundaries

Platform tests use local-only fixtures and test Git repositories, not public
Releases, bucket/tap publication, users' existing installations or real service
resources. Scoop's temporary user-PATH entry requires explicit tool approval and
is removed after testing, preserving unrelated entries and notifying Windows.
Original design work is not staged into this fix. Product versioning, full
installer/default-selection UX, data downgrade and release qualification remain
separate from this narrowly scoped executable-lifetime correction.
