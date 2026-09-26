---
title: E Package Manager Research - Scoop and Homebrew
---

# E package manager research: Scoop and Homebrew

**Status: Research and conditional recommendation, 2026-09-26. Not an approved
packaging contract, implementation, or publication authorization.**

> **Executed follow-up:** The [isolated proof of concept](./e-package-manager-poc.md)
> now records actual manager operations and a real Pactrun Hook reproduction.
> Default Homebrew cleanup broke an active Hook; disabling automatic cleanup did
> not protect explicit cleanup. Read that measured result before treating the
> conditional recommendation below as a completed installer selection.

The operator provisionally selected Scoop on native Windows and asked for a
Linux counterpart with a similar experience. This narrows the inquiry to
Homebrew with an upstream-maintained tap, rather than choosing a separate
version-manager workflow for Linux. Similar operator concepts matter more than
identical command spelling. This is not a claim that no other Windows manager
exists, nor an instruction to use WSL for the Windows product.

## Result

**Homebrew remains the first Linux candidate for ordinary package installation
and updates. Neither stock manager, with a simple manifest alone, has yet been
shown to satisfy the entire Pactrun version-selection and continuity contract.**

Do not start a custom Rust installer because these gaps exist. First prototype
one minimal bucket/tap pair and verify the gaps below. Also do not call the
pair a complete solution merely because install/update/uninstall commands map.
The custom installer in the [S0 plan](./e-versioning-s0-plan.md) is under
reassessment; this research does not approve a replacement or discard product
safety/compatibility decisions.

## Evidence scope and source versions

Official release API results checked on 2026-09-26 identified Scoop v0.5.3
(published 2025-08-12) and Homebrew 7.0.6 (published 2026-09-21). The source audit
uses these exact commits, not a claim about every installed manager version:

- Scoop: `b588a06e41d920d2123ec70aee682bae14935939`.
- Homebrew: `570982948a8a194f0f42f43f4a5bce2d1c9f64cb`.

Homebrew main was initially inspected at
`ca6e46bf058925552d19a6530c1bd2e45864079d`, then the relevant files were downloaded
again from the formal release before drawing conclusions. The old Homebrew
master branch is a legacy bootstrap branch, not the current source audit target.

Downloaded official files, URLs, hashes, release responses and the bounded
comparison probe are retained locally under `target/e-installer-research/`.
No downloaded package-manager entrypoint, installer, update, uninstall or
cleanup command was executed. No package manager was installed or reconfigured.

## Capability comparison

The following findings come from the source register below. "Needs packaging"
is a design inference, not an already implemented Pactrun capability.

| Requirement | Scoop evidence | Homebrew evidence | Pactrun disposition |
| --- | --- | --- | --- |
| Project-owned source | Bucket manifests determine the package and target download | A custom tap is a project-owned Git source with formulae [H1] | Good conceptual match; retain GitHub Releases as artifact origin |
| Routine management | Install, update, uninstall, hold/unhold, cleanup and reset entrypoints [S1-S3] | Install, upgrade, uninstall, pin/unpin and cleanup [H2-H4] | Similar concepts, not identical commands or transactional behavior |
| Major restriction | The installed bucket/manifest supplies the update candidate [S1] | The selected formula supplies its candidate [H2] | Needs Major-specific package definitions; an unversioned rolling latest package would not encode this policy |
| Prerelease eligibility | A manifest supplies an explicit version, not Pactrun's release-set policy [S1] | Core's policy rejects unstable versions; that admission policy does not prohibit project-owned taps [H1,H5] | Our own bucket/tap can publish the selected prerelease; publisher must compute the eligible set correctly |
| Exact acquisition | install app@version may generate a manifest from current autoupdate data; it is not necessarily replay of historical package metadata [S2] | version-install extracts an older definition into a personal tap; existing versioned formulae are another route [H5,H6] | Prefer immutable, publisher-provided definitions and trusted hashes; do not require users to maintain extracted formulae |
| Exact retention | hold protects an installed package from normal update, not arbitrary historical availability [S1,S2] | pin protects an installed formula from normal upgrade, not arbitrary historical availability [H5] | Do not equate a held install with an exact-version acquisition service |
| Coexistence/default | Separate app names and explicit shims are available; same-name reset changes the active version [S2,S4] | Separate names plus non-conflicting binaries or keg-only formulae [H1,H5] | Needs explicit command/root association and tests; test installation must not take over the normal command |
| Running operations | update/reset/uninstall consult a process-path scan in reviewed paths; override exists [S1,S3,S4] | Upgrade has cleanup paths; keg removal recursively removes its directory [H3,H4] | No complete Pactrun liveness/safety guarantee established on either platform |
| Data removal | uninstall --purge removes Scoop-managed persistent data [S3] | Keg removal deletes package files; custom packaging can add behavior [H4] | Keep management roots and service data outside either manager's package/persistence trees and removal scripts |
| Setup cost | Windows package-manager bootstrap is separate from Pactrun installation | Linux Homebrew requires its own environment, writable prefix and development prerequisites [H7] | Similar daily operations do not imply identical first-time bootstrap cost |

## Important findings

### 1. Publish the selection policy, not a vague latest package

Recommendation: one shared publisher-side selection function computes the
highest eligible product SemVer for a given Major and prerelease flag, then
generates both package definitions with the same product version and the
appropriate target artifact hash. This is release tooling, not a resident
updater or new channel service. The managers consume those definitions.

Possible package labels, only examples:

- `pactrun-1`: Major 1, formal-only.
- `pactrun-1-preview`: Major 1, formal AND prerelease eligible.

Preview must not mean "only alpha/beta/rc": it follows formal 1.0.0 if that is
highest, and may later follow 1.1.0-alpha.1. No separate alpha/beta/rc channels.
Before the first formal release, formal-only must report no eligible release,
not silently redirect to preview. Stale/out-of-order publishing must not regress
either definition. Manager-specific version revisions/rebuilds require their
own tests; a SemVer unit pass alone does not establish no-downgrade behavior.

These definitions implement eligibility, not the complete saved-selection UX.
Installing another eligibility package usually installs/selects a payload; it
does not automatically meet the approved "change eligibility without switching"
behavior. A final design must explicitly cover that behavior, default selection
and installation/root associations, or request a product decision about a
concrete mismatch. Do not silently redefine it as "uninstall and reinstall".

### 2. Exact prerelease versions need more than a generic convenience command

Scoop's generate_user_manifest first tries current/cached metadata, otherwise
uses autoupdate to synthesize the requested version. That path needs verification
against our artifact naming and authoritative hashes; it is not a universal
guarantee of historical reproducibility [S2].

Homebrew 7.0.6 does have `brew version-install`; saying it simply cannot obtain
old versions would be inaccurate. But its implementation normalizes a requested
version into a formula name by replacing nonnumeric runs with dots and has an
early installed-name check. The reviewed expressions map all of
`1.0.0-alpha.1`, `1.0.0-beta.1`, and `1.0.0-rc.1` to `1.0.0.1` [H6].

This is a source-level naming-collision finding, supported by a local translation
of those expressions, **not a reproduced end-to-end Homebrew installation bug**.
Do not make that convenience command the Pactrun prerelease-selection contract.
Publisher-generated, distinct immutable formula names are a candidate workaround;
verify their class naming, installation, linking and duplicate behavior rather
than assuming they solve everything. Do not require users to edit Ruby formulae.

### 3. Cleanup and in-flight execution are the main safety gap

Scoop's process check scans executable paths under the app directory. It is a
snapshot, can be disabled by IGNORE_RUNNING_PROCESSES, and does not hold a
Pactrun ownership/launch lease. The reviewed cleanup path removes retained
version directories without that same process check. These observations do not
prove Scoop kills a process; they mean the scan is not a complete coordination
proof [S1,S3,S4].

Homebrew has cleanup after install/upgrade and explicit cleanup. Its formula
locks coordinate package management, not participation by a running Pactrun
process. Keg.uninstall calls FileUtils.rm_r on the version directory [H3,H4].
The NO_INSTALL_CLEANUP and NO_CLEANUP_FORMULAE controls are useful prototype
controls, not a product safety mechanism: an unset environment variable or
explicit uninstall must not become an undocumented data/operation hazard.

There is a concrete local consumer: src/hook/shell_loader.rs supplies
std::env::current_exe() to the Hook environment; src/hook/platform.rs also
launches the current executable for adapter work. Retaining an already-running
process is therefore insufficient evidence that later required launches still
work after its installed path is removed or switched. This is a risk inference
from the inspected code, not a new failure reproduced in Pactrun by this task.

Any accepted package arrangement must protect the whole interval from active
operation admission to its last dependent launch, including races with cleanup
and uninstall. Waiting for an operation to finish naturally may be an option;
requiring service interruption or killing an operation is not. Do not add a
temporary scan or a user warning and call the gap closed. If stock package
operations cannot meet the promise, report that conflict before accepting this
distribution path as the complete installer replacement.

### 4. Do not put product data in package-manager persistence

Packaging should own program artifacts only. No Scoop persist entry for a
Pactrun management root; no Homebrew Cellar/opt location as the management root;
no formula or uninstall script that creates, migrates, moves, rebinds or deletes
service data. Normal and preview roots remain distinct by explicit product
configuration. Changing a symlink/shim must not implicitly choose a different
root. This is a proposed packaging restriction, not something either manager
automatically enforces for us.

The host operator can deliberately delete arbitrary files; that is distinct
from promising that supported normal update/uninstall/cleanup workflows preserve
Pactrun's contracts. Do not excuse a normal-workflow safety gap by pointing to
the operator's broader OS authority.

## Recommended bounded proof before selection is final

Use isolated test roots, harmless versioned executable fixtures and the existing
Pactrun baseline. Do not publish repositories/releases or modify a user's real
Scoop/Homebrew setup for this proof. Windows manager setup outside the workspace
still needs separate authorization; use only the configured remote on Linux.

1. Generate a local bucket/tap pair from identical fixture releases: unsorted
   versions, alpha.9/alpha.10, beta/rc, formal, a later Minor prerelease and Major 2.
   Verify formal-only/no-formal failure, inclusive preview, no cross-Major and no
   metadata regression. Include stale repository metadata and exact pins.
2. Install normal and preview concurrently with non-conflicting entrypoints and
   sentinel management/service directories. Prove explicit defaults and stable
   root selection without implicit initialization or take-over.
3. Acquire two exact prereleases with equal numeric suffixes, reinstall offline
   from available artifacts, reject incorrect hashes, then select an older
   executable. No data-downgrade claim follows from executable selection.
4. Update/reset/relink during a real Hook and delayed shell-loader helper launch.
   Race cleanup, direct executable launch and uninstall using deterministic
   barriers. Check exit status, handles, paths, service access and sentinel data.
5. Inject interrupted download/publication and partial-install recovery. Compare
   data trees and all active command targets, not just command exit status.
6. Reject the complete-replacement claim if eligibility-only changes, active
   operation safety, or non-destructive uninstall requires a broad new manager
   that defeats the low-maintenance objective. Present the exact tradeoff rather
   than building a hidden updater or weakening the contract.

This proof is not executed by the current research task. It is the specific next
engineering gate, not a request for the operator to decide module/lock details.

## Validation status

- **Passed:** official documentation/source audit for the recorded versions;
  16 pure Scoop Compare-Version cases covering the relevant alpha/beta/rc,
  numeric ordering, formal transitions, equal versions and downgrade direction.
  Only the reviewed Compare-Version/SplitVersion definitions were evaluated.
- **Source finding only:** Homebrew version-install name normalization; a local
  .NET regex translation is recorded separately, not labelled Ruby/brew execution.
- **Not run:** manager install/update/switch/uninstall/cleanup E2E, package
  publication, active-operation races, minimum-platform qualification, Rust CI.
- **Environment:** local Windows plus a read-only Ruby availability query on the
  configured remote. Ruby was absent; no installation was attempted. No new
  service was started. No host PATH, shell profile or package-manager config changed.
- **Documentation impact:** this informative report and an S0 reassessment note
  only. Existing decisions and active Spec requirements are not rewritten.

## Primary source register

Read repository paths below at the exact commits in the evidence scope. Local
sources.json records their original URLs and SHA-256. Official document URLs
are discovery references; online pages can change after this snapshot.

| Key | Official source and audited paths |
| --- | --- |
| S1 | Scoop libexec/scoop-update.ps1: update, app_status-driven eligibility and running-process precheck |
| S2 | Scoop libexec/scoop-install.ps1, libexec/scoop-reset.ps1, libexec/scoop-hold.ps1; lib/manifest.ps1: generate_user_manifest; lib/versions.ps1 |
| S3 | Scoop libexec/scoop-uninstall.ps1 and libexec/scoop-cleanup.ps1 |
| S4 | Scoop lib/install.ps1: test_running_process, create_shims, link_current, persist_data |
| H1 | Homebrew docs/How-to-Create-and-Maintain-a-Tap.md |
| H2 | Homebrew Library/Homebrew/upgrade.rb and formula_installer.rb |
| H3 | Homebrew Library/Homebrew/cleanup.rb and env_config.rb |
| H4 | Homebrew Library/Homebrew/uninstall.rb and keg.rb |
| H5 | Homebrew docs/Versions.md; Library/Homebrew/cmd/pin.rb and cmd/link.rb |
| H6 | Homebrew Library/Homebrew/cmd/version-install.rb and dev-cmd/extract.rb |
| H7 | Homebrew docs/Homebrew-on-Linux.md, Installation.md and Support-Tiers.md |

```text
https://github.com/ScoopInstaller/Scoop/tree/b588a06e41d920d2123ec70aee682bae14935939
https://github.com/Homebrew/brew/tree/570982948a8a194f0f42f43f4a5bce2d1c9f64cb
https://github.com/ScoopInstaller/Scoop/wiki/FAQ
https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap
https://docs.brew.sh/Versions
https://docs.brew.sh/Manpage
https://docs.brew.sh/Homebrew-on-Linux
```
