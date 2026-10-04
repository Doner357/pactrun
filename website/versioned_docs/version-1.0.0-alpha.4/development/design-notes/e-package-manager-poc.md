---
title: E Package Manager Isolated Proof of Concept
---

# E package manager isolated proof of concept

**Status: Executed on 2026-09-26. The unmodified Scoop/Homebrew pair is NOT
accepted as a complete installer replacement. Default Homebrew package removal
reproducibly breaks an already-running Pactrun Hook in the tested packaging.**

> **Prototype follow-up:** A separately authorized
> [running-executable lifetime prototype](./e-executable-lifetime-prototype.md)
> passes these specific failure reproductions without retaining an extra disk
> copy or disabling default cleanup. That isolated result does not change the
> original unmodified-baseline outcomes below or constitute product integration.

This follows the operator-authorized verification of the
[package-manager research](./e-package-manager-research.md). It is a bounded
packaging experiment, not approval of E runtime, a custom installer, publication,
or a changed service/compatibility contract. Product source and active Spec
requirements were not changed to make the experiment pass.

## Decision-quality result

- **Retain Scoop as the provisional Windows candidate.** The tested operations
  protected the active executable by skipping update/reset/uninstall or, for
  cleanup, encountering Windows file-use protection. This is not proof of all
  launch races or complete filesystem safety.
- **Do not accept an ordinary Homebrew formula as a drop-in solution yet.**
  Installation, version advancement, pinning, coexistence, hashes and explicit
  selection worked, but automatic cleanup and uninstall removed an executable
  needed by an active operation. A real Pactrun shell-loader reproduction failed.
- **Disabling automatic cleanup is not a complete repair.** It let the real Hook
  survive an update, but a later explicit cleanup reproduced the same failure.
- **This does not establish that Homebrew can never be supported.** A safe
  installed-executable lifetime design could change the result. That engineering
  design must cover update, cleanup, uninstall and concurrent starts; it is not
  implemented or selected here. Do not immediately replace the package manager
  with a newly invented installer, or weaken the contract to fit its defaults.

## Isolation, versions and provenance

| Item | Actual tested input |
| --- | --- |
| Windows manager | Scoop v0.5.3, commit b588a06e41d920d2123ec70aee682bae14935939; archived official source plus local fetch of that exact commit into each test root |
| Linux manager | Homebrew 7.0.6, commit 570982948a8a194f0f42f43f4a5bce2d1c9f64cb; isolated Git checkout and its downloaded portable Ruby 4.0.7 |
| Pactrun source | Current development baseline at HEAD 0117987786fab1a9b31f3472dd821e035a30321d, with existing documentation-only dirty work; no E implementation |
| Product builds | Fresh local Windows and configured-remote Linux `cargo build --offline --locked --bin pactrun`; 251 product source/Spec/dependency inputs matched across local and executed remote source |
| Windows executable SHA-256 | 7f875c5423d8fa2f9605ed4f69dfef028faed3e645b920d6d58288b6eea97f3c |
| Linux executable SHA-256 | f21fc35b6ec74372353c26185a03d2421017c4f878997dd43df2fa8485ebd279 |
| Initial matrix driver SHA-256 | b97097364f752e69c4c5efd538dd3e05c643942ee397cbee984c5f0303ab21aa; identical local and remote copies preserved |
| Downloads | Generated ZIP/tar.gz artifacts served on loopback only, SHA-256 checked by the actual managers; no GitHub Releases or public bucket/tap published |
| Manager update policy in the experiment | Manager self-refresh was disabled to keep its source fixed. Product package updates remained real. Default Homebrew product cleanup stayed enabled except in the explicitly labelled control |

All Scoop apps, buckets, cache and config lived under workspace `target/pm-poc/`.
No original Scoop installation or config was updated. Stock Scoop shim creation
does write the user PATH, so the operator was explicitly asked through tool
approval before those runs. Each finalizer removed only that run's temporary
shim entry and compared original user-PATH/config fingerprints. Restoration
passed, including when a harness prerequisite failed.

Linux used the configured persistent Pactrun test workspace, with isolated
Homebrew prefix, HOME, configuration, cache and logs; no system prefix, package
installation script, sudo, shell profile or host-wide PATH change was used.
Portable Ruby is retained inside those test prefixes. Short `/tmp/pmipc-*`
directories served IPC/build temporary needs; management/data fixtures stayed
on the persistent filesystem. This custom long prefix is a Homebrew Tier 3
configuration. Results are evidence for this precise setup, not minimum-OS,
official-prefix, bottle or production-release qualification.

The baseline executable still identifies as product 0.1.0. Real-product package
labels `0.1.0-probe.1` and `0.1.0-probe.2` intentionally contain the **same
binary bytes**. They only force manager replacement/removal paths, isolating
program-lifetime effects from schema, data compatibility or code differences.
They are not Pactrun prereleases and were never published.

## Executed matrix

| Behavior | Scoop | Homebrew | Meaning and limits |
| --- | --- | --- | --- |
| Install a prerelease artifact | Passed | Passed | Actual manager downloads, verifies and exposes the installed executable |
| hold/pin blocks normal update | Passed | Passed | Installed-version retention, not historical acquisition policy |
| alpha.9 to alpha.10; beta; rc; formal; later-Minor alpha | Passed | Passed | Explicit publisher-selected definitions advance in the intended sequence |
| Stale lower definition does not downgrade | Passed | Passed | Default update behavior in these cases; force overrides not exercised |
| Major-specific packages and normal/preview coexistence | Passed | Passed | Separate package definitions and non-conflicting names; not automatic SemVer release discovery |
| Distinct exact alpha.1/beta.1 definitions | Passed | Passed | Publisher-generated names, not generic version-install normalization |
| Incorrect hash does not expose a usable new command | Passed | Passed | Error status also verified through the proper manager entrypoint |
| Update with active old executable | Passed by deferral | Failed continuity | Scoop leaves the old version selected; Homebrew selects the new package and removes the old executable path |
| Cleanup with active old executable | Passed for delayed child, cleanup errors | Separate case skipped in initial matrix | Windows denied removal of the in-use executable. Linux ordinary update had already removed the old version |
| Uninstall with active executable | Passed by deferral | Failed continuity | Homebrew removal succeeded but the active fixture could no longer launch its same-path child |
| Ordinary uninstall after completion | Passed | Passed | Selected command removed; other installation/data sentinels retained |
| Real shell-loader control before manager mutation | Passed | Passed | Confirms the installed real binary, Pack, Hook transport and delayed helper work |
| Real shell-loader helper after ordinary update | Passed by deferral | Failed | Hook helper exited 127 and the admitted Run failed on Linux |
| Existing Instance after update | Passed | Passed | Inspectable without rebind, recreation or schema change; does not excuse the failed active Run |
| Management/service sentinel bytes | Passed | Passed | No fixture data deletion observed; not a media-erasure or production-service test |

The initial final Windows matrix records 27 successful assertions. The initial
Linux matrix records 22 successful, 3 failed and 1 skipped assertions. These are
harness observations, not normative requirement counts. Its Windows PATH/config
comparison is meaningful; the analogous Linux row is an empty Windows-specific
fingerprint comparison, **not a Linux host-wide filesystem audit**. Linux
isolation evidence is the explicit prefix/environment and executed commands.

### Explicit switching supplement

- Scoop `reset` selected an older retained version and its native shim executed
  that version. While a process was active, another reset left selection unchanged
  and the old process could still launch its child. This is deferred switching,
  not immediate hot switching during a Run.
- Homebrew installed a second exact-version package with `--skip-link` without
  taking over the shared command. Explicit unlink/link selected the other version
  and then the older one. Relinking without removing the old payload preserved
  an active child's later launch.
- The Homebrew supplement is a two-command switch between distinct exact
  formulae. It does not prove atomic default-command replacement, no transient
  command absence, or the approved eligibility-only selection operation.

## Reproduced real failure

1. Install the unmodified baseline through a real manager package.
2. Install a small Source V3 Pack and create an Instance in an isolated persistent
   management root. Its Observe action uses the existing shell-loader protocol.
3. Run a control action that calls `PACTRUN_EXECUTABLE hook session`; it succeeds.
4. Start a second action, wait for a file barrier proving its script is running,
   then update the program package before allowing the script to call its helper.
5. Homebrew reports successful upgrade and removal of the old package directory.
   Release the barrier. The helper's old executable path no longer exists.

Observed Linux result:

```text
manager update exit: 0
old executable exists: false
Hook helper exit: 127
Pactrun invoke exit: 1
Run outcome: failed
boundary: admitted
terminal_risk: clear
current_recovery_guard: none
```

The same Windows test skipped the update, retained the old executable and
completed the Hook/Run successfully. A sampled file-access witness used an
existing open handle and fresh path opens during each update: 84 Windows and
349 Linux samples completed without mismatch/read errors in the initial matrix.
These are bounded file-access observations, not proof for every live service or
all schedules. Instance inspection and sentinel hashes also passed afterward.

Thus this failure is **interference with an active management operation**, not
observed service-data corruption or a schema/version incompatibility. The product
contract prohibits that interference even if services keep reading their data.

## Causal control: disabling automatic cleanup

A second Linux real-product run set `HOMEBREW_NO_INSTALL_CLEANUP=1` only in its
isolated command environment. Updating then retained the old executable; the
delayed Hook helper returned 0 and the Run succeeded. The service-access witness
recorded 368 successful samples.

Next, a real Hook launched from that retained executable was held at the same
barrier. An explicit `brew cleanup pm-real` removed its old package directory
despite the automatic-cleanup setting. The cleanup command returned 0; the
helper returned 127 and the Run failed again.

This strengthens attribution to installed-file lifetime. Disabling automatic
cleanup can address that particular update path, but **does not close the
cleanup/uninstall/concurrent-start contract**. Do not promote an environment
variable or a warning into the safety design, and do not silently redefine
normal cleanup as operator misuse to avoid the obligation.

## Remaining scope and next engineering boundary

**Overall acceptance: Failed for the unmodified minimal Homebrew packaging.**
Scoop remains provisional rather than fully certified. No code change to
Pactrun or either manager was made to repair this result.

Before accepting a complete distribution/installer replacement, design and prove
the lifetime of the selected executable and any later helper launches under
manager update, explicit cleanup, uninstall and races with new operations.
Potential safe integration is still open; this experiment does not choose
manager patching, a wrapper, a custom installer, or a new product API.

The following remain **Not run / not established**, not hidden passes:

- Actual GitHub Release discovery, common publisher-side eligibility filtering,
  no-formal-candidate handling and a saved eligibility change without switching.
- Full installation/root/default association UX; atomic relinking and arbitrary
  named-install behavior. Separate package names are not proof of those contracts.
- Default-prefix Homebrew deployment, minimum OS/filesystem matrix, production
  optimized artifacts and public end-to-end distribution.
- Disk-full, power-loss, interrupted activation, cancellation, adversarial archive
  tests beyond digest rejection, and exhaustive launch/ownership races.
- Real data downgrade or prerelease-to-formal data acceptance. The product bytes
  used for the real replacement labels were identical.
- Full Rust CI. Only the unchanged product builds and these scoped experiments
  were needed for this packaging inquiry.

A concrete counterexample is sufficient to reject the complete-replacement
claim now; the unrun rows remain future acceptance work. It is not necessary to
implement the whole E milestone just to establish this failure.

## Evidence and preparation corrections

Local evidence root: `target/pm-poc/`. Main Windows run:
`win/run-20260926-150810/evidence`; switching supplement:
`win/run-20260926-151704/evidence`.

Linux evidence is copied beneath local `target/pm-poc/remote/`. Main run:
`linux/run-20260926-070836/evidence`; automatic-cleanup control:
`linux/run-20260926-071614/evidence`; switching supplement:
`linux/run-20260926-071826/evidence`. Run directory clocks differ because the
remote clock uses UTC; they are all the 2026-09-26 task.

Each final evidence set includes command arguments, return codes, output logs,
results, manager/source provenance and real binary hashes. In particular, the
main Linux `update-pm-real` log records removal, and
`real-hook-during-update.log` records the failed admitted Run. The control has
`real-hook-during-manual-cleanup.log`. Product input checks and fresh build logs
are retained separately. Fixture Git commits exist only inside isolated test
repositories; no project branch was staged, committed, merged or pushed.

Preparation attempts were not counted as final acceptance:

- An archive-only Scoop checkout let version discovery see the enclosing project
  repository. Later runs fetched the official commit into the isolated core's
  own Git repository and verified it before testing.
- Direct invocation of bin/scoop.ps1 did not preserve a nested hash-error exit
  status. Final runs used Scoop's standard PowerShell shim shape, including
  `exit $LASTEXITCODE`; hash rejection returned failure correctly. No Scoop
  integrity-bypass claim is made from the discarded harness invocation.
- The first Linux temporary directory exceeded native socket-path limits. Short
  IPC paths fixed this without moving management/data fixtures to tmpfs.
- Disabling Homebrew's formula API caused an unnecessary core-repository clone
  during preparation. Its owned processes were stopped after the timeout; normal
  API-backed initialization then succeeded. The final driver records timed-out
  output and terminates its own Linux command process group.
- The loopback server logged client-disconnected download probes; completed
  downloads still passed actual manager hash verification. These diagnostics
  were not treated as product corruption or suppressed into a false pass.

Test servers and owned execution processes have ended. Isolated program installs,
source clones, caches, data fixtures, logs and short IPC temporary directories are
retained for inspection; no production deployment or real service was created.
Document/link/traceability checks and remote site typecheck/build are separate
documentation evidence, not a substitute for the failed packaging contract.
