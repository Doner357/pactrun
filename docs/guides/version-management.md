---
title: Manage installed versions
---

# Manage installed versions

Scoop/Homebrew manage programs; Pactrun manages its data. Updating or removing a
program package must not remove management roots, Instances, Inputs, Secrets,
Snapshots or service resources. When next opened, a supported older Store is
upgraded automatically before the requested operation. An unsupported or unsafe
conversion is refused without resetting data. Reinstalling an older program is
not a data rollback or conversion procedure.

## Ordinary Preview updates

```powershell
scoop update
scoop update pactrun-preview
pactrun --version
```

```sh
brew update
brew upgrade doner357/pactrun/pactrun-preview
pactrun --version
```

The Major 1 Preview entry accepts formal and prerelease versions by product-version
precedence. It does not become Major 2 automatically. Future Majors require separate
entries and an explicit installation decision. There is no stable fallback to alpha.

## Freeze the version already installed

```powershell
scoop hold pactrun-preview
```

```sh
brew pin doner357/pactrun/pactrun-preview
```

Hold/pin changes update eligibility without replacing the executable. Inspect the
manager's hold/pin state. To resume updates, use `scoop unhold pactrun-preview` or
`brew unpin doner357/pactrun/pactrun-preview`, then run the update separately.

## Install an exact published version

Exact entries do not move when a new Preview is published. For alpha.7:

```powershell
scoop uninstall pactrun-preview
scoop install pactrun/pactrun-exact-1-0-0-alpha-7
pactrun --version
```

```sh
brew uninstall doner357/pactrun/pactrun-preview
brew install doner357/pactrun/pactrun-exact-1-0-0-alpha-7
pactrun --version
```

The alpha.1 through alpha.6 exact entries remain available as
`pactrun-exact-1-0-0-alpha-1`, `pactrun-exact-1-0-0-alpha-2` and
`pactrun-exact-1-0-0-alpha-3`, `pactrun-exact-1-0-0-alpha-4` and
`pactrun-exact-1-0-0-alpha-5`, and `pactrun-exact-1-0-0-alpha-6`; publishing alpha.7
does not rewrite their assets. The alpha.1 through alpha.3 Linux builds retain
their glibc 2.39 requirement; selecting one of those older
exact version is not a workaround for Debian 12 compatibility.

Before switching, record the installed package/version, confirm no Pactrun operation
is in flight, check target data compatibility and preserve needed backups. Do not
stop a service or force an unlock merely to make a package update proceed. An
ordinary native update cannot select an older version; the commands above are an
explicit switch. If the old executable cannot read current data, stop rather than
trying to remove or relabel that data.

## Updating from alpha.4

Opening an alpha.4 Store with alpha.5 or alpha.6 upgrades its management schema to
Persistence `1.0-alpha.4`, preserving existing Revision and Instance identities.
Installing the executable alone does not upgrade the Store. An older executable
cannot reopen the upgraded Store; reinstalling it is not a data rollback.

Old Revision aliases convert to local names only without information loss.
Multiple aliases on one Revision or names outside the supported grammar block
the upgrade and must be corrected with the older version first.

## Updating to alpha.7

Opening a supported older Store with alpha.7 upgrades it to Persistence
`1.0-alpha.5`. Existing Revision identities, canonical bytes, bindings and
retained Run artifacts are preserved. The upgrade adds storage for artifacts
produced by Capture, Restore, Migration and Cleanup. Older executables cannot
reopen this Store; preserve a backup before upgrading if rollback is needed.

Existing Packs retain their original Input and Hook behavior. New operation-level
Input declarations use Source `1.0-alpha.3`, Core `1.0-alpha.2` and Hook Protocol
`1.0-alpha.2`; changing a Pack to this model creates a new Revision rather than
relabeling the installed one. Restore still requires the Snapshot's exact
producer Revision.

The default machine interface is now `1.0-alpha.4`. Existing automation may select
`--format json --machine-version 1.0-alpha.3` before the command for supported
legacy views. Operations or results that cannot be represented by that interface
are refused rather than silently dropping information.

## Stable and Preview switching

Once a formal Major 1 version exists, the Stable package is `pactrun`. Switching
uses the native manager: uninstall the currently installed program package, then
install the chosen entry. Stable and Preview both expose `pactrun`; they cannot
own the same command at the same time. No source branch checkout is involved.

For example, after Stable becomes available, switch from Preview with
`scoop uninstall pactrun-preview` followed by `scoop install pactrun/pactrun`, or
`brew uninstall doner357/pactrun/pactrun-preview` followed by
`brew install doner357/pactrun/pactrun`. Reverse the package names to opt into Preview.
These Stable commands are not available during the alpha-only delivery.

There is a short interval without the command between uninstall and installation.
If installation fails, retain the error and reinstall the original exact package;
do not clear product data, change its version markers or bypass checksum checks.
Restoring the old program does not guarantee it understands data written by a newer
program. This is why target compatibility and backups are checked before switching.

## Isolated evaluation

`pactrun-test-preview` exposes `pactrun-test`, with a separate default data root.
Normal and test installations can coexist. Exact test entries use the corresponding
`pactrun-test-exact-...` name. An explicit `PACTRUN_STORAGE_ROOT` overrides either
default, so isolate service resources as well as management data before testing.

## Source relocation

Keep the bucket/tap name and installed package identity when moving its repository
URL. Update the native source clone's origin, then refresh through the manager;
do not reinstall the program merely to change a URL. Hold/pin and exact package
choice must remain unchanged. This is an operator source-setting operation, not
a Pactrun data migration. Follow the project's announced migration instructions
if its official repository ever moves.

Return to [installation](./installation.md) or the [task guide](./index.md).
