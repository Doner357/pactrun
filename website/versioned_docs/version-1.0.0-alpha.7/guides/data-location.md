---
title: Data locations and separate stores
---

# Data locations and separate stores

Use this page after [installing and verifying Pactrun](./installation.md), when
you need a persistent store or another isolated environment. The
[first Instance tutorial](../introduction.md) already prepares its own disposable
store; new readers can follow that example without this detour.

## What a store contains

A store holds Pactrun's management state and retained content. It is separate
from the executable installation. ServiceStorage allocations have their own
lifecycle; deleting or moving a store directory is not a supported way to retire
an Instance or dispose of service data.

`PACTRUN_STORAGE_ROOT` explicitly selects a store. Pactrun chooses a default only
when this variable is absent, using the actual executable's name:

| Entry point | Windows default | Linux default |
| --- | --- | --- |
| `pactrun` (package or standalone) | `%LOCALAPPDATA%\pactrun` | `$XDG_DATA_HOME/pactrun`, or `$HOME/.local/share/pactrun` when XDG_DATA_HOME is absent |
| `pactrun-test` | `%LOCALAPPDATA%\pactrun-test` | `$XDG_DATA_HOME/pactrun-test`, or `$HOME/.local/share/pactrun-test` |
| Renamed executable with another stem | Explicit `PACTRUN_STORAGE_ROOT` | Explicit `PACTRUN_STORAGE_ROOT` |

Platform data bases must be absolute paths. An explicitly empty override is an
invalid selection, not a request to fall back to the default. Normal and test
entries can be pointed at the same store by an explicit override; their names
alone do not protect valuable data after you override the location.

Pactrun prepares a missing Store automatically when an operation needs to write
to it. You do not need to create internal directories or run an initialization
command. Global collection queries return empty results before a Store exists.
An existing but incomplete or unsupported Store is not treated as a new one.

## Choose a different data location

The executable's default is sufficient for ordinary use. To select another
Store, choose a new `pactrun-data` path in the current directory. For real use, choose a
persistent location covered by your backup policy; do not put service data in a
disposable tutorial directory.

PowerShell:

```powershell
$env:PACTRUN_STORAGE_ROOT = Join-Path (Get-Location) 'pactrun-data'
```

POSIX shell:

```sh
export PACTRUN_STORAGE_ROOT="$PWD/pactrun-data"
```

Selecting a location does not create an Instance or service files. A missing
Store is prepared when first needed for writing. Continue with
[installing a supplied Pack](./use-pack.md).

## Select an existing compatible store

Set `PACTRUN_STORAGE_ROOT` to its known absolute path in the terminal where you
will run Pactrun. For example, set
`$env:PACTRUN_STORAGE_ROOT` on PowerShell, or `export PACTRUN_STORAGE_ROOT=...` on
a POSIX shell, using your actual path. Inspect `pactrun instance list` before
performing a write and confirm that the objects are the ones you expect.

Changing the selection neither copies data nor moves an Instance. Restore the
previous environment value, or close the dedicated terminal, when finished.
If no override is set, the outer shell may show no variable even though the
executable selects its default internally. Use the table above to identify that
location. Executed Hooks receive the selected root explicitly.

## If a store is refused

After a program update, opening a supported older Store upgrades its management
schema automatically. You do not need a separate upgrade command. An upgrade
that requires a user choice, encounters an active incompatible writer, or cannot
validate the source Store stops with an explanation and preserves the old data.
Reinstalling an older executable does not reverse a Store upgrade.

Preserve the data and inspect the binary, selected root, permissions, and
compatibility. Do not delete directories, rewrite version markers, or use a
retired development upgrade command. Executable uninstall does not authorize
managed-data deletion; use [retirement](./retirement.md) for supported lifecycle work.
