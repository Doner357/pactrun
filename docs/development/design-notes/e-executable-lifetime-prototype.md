---
title: E Running Executable Lifetime Prototype
---

# E running executable lifetime prototype

**Status: Isolated prototype authorized and exercised on 2026-09-26; not merged
into product runtime, not an E milestone approval or a release qualification.**

This is the bounded follow-up to the
[package-manager counterexample](./e-package-manager-poc.md). The operator asked
to try a minimal repair. All experimental Rust changes are in the local ignored
source copy `target/ep/source/`, not the project's `src/` or `tests/` directories.
The original failing evidence is retained; it is not retrospectively marked as
passing. No custom installer, manager fork, updater, data conversion, new format
or historical-reader support was added.

## Result

The prototype passes the previously failing real Homebrew update/cleanup cases
and a real active-uninstall case. It does not need to disable normal automatic
cleanup or keep an extra on-disk executable cache. Windows retains its previous
lookup behavior and its 23 focused shell-loader system tests pass.

This closes the reproduced executable-path problem at prototype scope. It does
not by itself establish all version-selection, packaging, default-installation,
platform-floor or distribution requirements. The package-manager direction can
continue to be evaluated without treating this failure as proof that a custom
installer or different Linux platform is required.

## Small implementation boundary

The existing shell-loader contract says PACTRUN_EXECUTABLE names the running
executable; it does not grant a durable installation-path identity. The prototype
preserves that meaning and the existing Session/authority/completion contract.

On Linux, a new crate-private Hook helper returns `/proc/<caller-pid>/exe` and
checks that the locator is accessible. It deliberately does **not** canonicalize
that path into the old, removable installation filename. The calling process
remains alive for the child/Hook use covered by its execution ownership.

Use that helper consistently for:

- Launching the built-in shell-loader from the execution materializer.
- Recognizing that private self-launch when setting up its startup/IPC boundary.
- Launching the interactive adapter.
- Supplying the loader's own running-image locator to its script's helper calls.

On Windows and other non-Linux targets, the helper retains std::env::current_exe.
Only Windows/Linux were checked; no new platform promise is made.

The locator refers to a live process image, not whichever package version is now
selected in a shim/symlink or has been written to the old pathname. The prototype
does not copy or migrate managed objects, rebind an Instance, change ownership,
transfer a Session, restart a service or replay a helper request. The kernel's
existing process-image lifetime, rather than a new history directory or garbage
collector, keeps the executable available during the active process lifetime.

Linux requires the relevant procfs locator to be readable/executable within the
normal process namespace and permissions. There is no fallback to a newly
installed executable when lookup fails. The locator is not durable discovery,
an authentication token or safe for caching outside its execution context.
PID reuse and foreign namespaces must not be turned into a new version-selection
or Session-recovery mechanism. Existing loss/cancellation handling remains the
authority for whether an operation can complete.

Only these six paths differ in the isolated source copy:

```text
src/hook.rs
src/hook/executable.rs
src/hook/materialize.rs
src/hook/platform.rs
src/hook/shell_loader.rs
tests/system/shell_loader.rs
```

The new production helper is small; most of the patch is regression coverage.
No dependency, product version, canonical bytes, database schema, Hook wire
shape, public command or unsafe-code policy changed. The remaining production
self-launch locations were inspected; unchanged current_exe uses elsewhere in
the inspected modules are test workers, not an alternative production selector.

## Regression evidence

| Check | Result and actual scope |
| --- | --- |
| Removed and replaced original filename | **Passed:** a copied test executable deletes its own pathname, puts a different executable there, and still re-executes the original running image. The replacement's control invocation returns a distinct failure code. |
| Real CLI + delayed helper after pathname replacement | **Passed:** one system regression exercises none/output/interactive terminal modes. The original program file is removed and replaced while the real Hook waits; helper and Run complete successfully. |
| Locator scope | **Passed in that regression:** the supplied loader locator is no longer available after normal operation/loader completion; it is not used as persistent state. This is not a universal PID-reuse proof. |
| Linux focused shell-loader suite | **Passed: 24 tests**, including the new three-mode system regression. |
| Windows focused shell-loader suite | **Passed: 23 tests**, without changing its native path behavior or host configuration. |
| Formatting and Clippy | **Passed:** prototype formatting plus Windows and Linux workspace/all-target/all-feature Clippy with warnings denied. |
| Real Homebrew ordinary update with default automatic cleanup | **Passed:** old executable path removed, helper exit 0, admitted Run succeeds. |
| Real Homebrew explicit cleanup | **Passed:** old executable path removed during a new active Hook; helper exit 0 and Run succeeds. Automatic cleanup was disabled only to arrange a retained old version for this specific test. |
| Real Homebrew active uninstall | **Passed in both package runs:** selected package files removed, existing Hook/Run still completes, and its data remains inspectable with a separate same-build control executable. New invocations through an uninstalled package are not promised. |
| Data-access witness | **Passed:** 253/358 sampled existing-handle and fresh-path reads around the two updates, with unchanged sentinels. This is a bounded witness, not production-service deployment certification. |
| Complete configured-remote cargo xtask ci | **Passed, fresh run on the isolated prototype:** conformance, formatting, Clippy, workspace/system tests, document/traceability checks and site typecheck/build. All 419 selected inputs matched before/after; the package-tested executable hash remained unchanged. |

The unit test runner also executes a subprocess-worker test; do not count that
as a second independent identity scenario. Focused runs and the same cases in a
full CI run are not separate coverage. The prototype adds no ignored tests and
does not alter existing timeouts, assertions or capacity-test scope.
The four pre-existing explicit capacity/RSS ignores remain Not run; no additional
ignore or weakened assertion was introduced. The full gate's 175-document site
snapshot predates this informative report; current-checkout documentation
closeout is checked separately without rerunning unchanged prototype runtime.

## Source-qualified inputs

The source copy contains 419 selected files, derived from the working tree at
HEAD `0117987786fab1a9b31f3472dd821e035a30321d` plus the six-path prototype.
Existing unrelated dirty work and archives remain untouched. Input hashes were
checked before and after focused execution; the complete gate checks its own
before/after manifest as well.

- Prototype source-manifest SHA-256:
  `61383ac6d902b5641625f5d8f8e69abb8048d808f9be71a6ef928998616d268b`.
- Review patch SHA-256:
  `28887ad7c4bf157fba31168ebd45b1227ed040d655b75467afa2b64b3d06c1fc`.
- Linux prototype executable after focused build:
  `00c06c8fac4c4b99b7816b868463c23d1b5497ccfde396bbaf06eb78659d6a71`.

The actual build paths identify the isolated prototype source, not the original
checkout. The Homebrew drivers record the exact input executable hash and their
own source. Their package labels still contain identical product binaries within
each run, isolating file-lifetime behavior from a product/data-version transition.
The tested product version remains 0.1.0, not a published E alpha.

Provenance correction: the two raw package-run real-product.json files inherited
an "Unmodified current development baseline" note from the earlier driver. That
note is inaccurate for this prototype; their executable hash and build path
identify the modified isolated image above. Raw evidence is preserved, this
correction is explicit, and the reusable driver note has been corrected. No test
outcome or executed binary is changed by correcting that descriptive note.

The Linux gate uses the persistent supported filesystem, optimization level 1,
debug info 0, debug assertions and overflow checks enabled, and one test thread.
Short temporary IPC paths do not relocate filesystem-semantic fixtures to tmpfs.
Manager source remains Homebrew 7.0.6, and the reproduced counterexample's
normal cleanup behavior stays enabled in the default-update run.

## Evidence locations and adoption boundary

Local prototype/evidence root: `target/ep/`.

- `source/`: the isolated code and tests.
- `evidence/prototype.patch`: reviewable patch; not applied to the project runtime.
- `evidence/windows-shell-loader.log` and `windows-clippy.log`: local checks.
- Remote gate and package evidence is copied under `evidence/remote/` at closeout.
- Original failure evidence remains under `target/pm-poc/` and its prior report.

The operator's request authorizes this experiment, not the whole E implementation.
Before product integration, apply the reviewed change through the approved E
sequence, attach stable test traceability where appropriate, retain the new
regressions and qualify intended platforms/artifacts. Do not present this as
independent review or a blanket guarantee for every namespace, library layout,
OS, data downgrade or future package-manager version.

This task does not change Scoop/host PATH, because Windows verification uses the
isolated product tests rather than reinstalling through Scoop. No user/service
data cleanup, Git integration, public repository/Release creation or deployment
is performed. Isolated builds, test installations and evidence remain available
for inspection; no persistent updater or new preview service is introduced.
