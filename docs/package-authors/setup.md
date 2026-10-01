---
title: Prepare an authoring workspace
---

# Prepare an authoring workspace

Install the public Preview through the Scoop or Homebrew instructions in
[shared installation](../guides/installation.md). The current published version
is `1.0.0-alpha.2`, not a formal stable release. The executable and managed storage
are separate; do not use a real service store for experiments.
Run `pactrun --version` and `pactrun --help` to verify the selected executable.
If those commands are unavailable or select the wrong installation, complete
[shared installation](../guides/installation.md) before creating this workspace.

The examples need `sh` on POSIX or PowerShell 7 (`pwsh.exe`) on Windows. Pactrun
does not install an interpreter or override its execution policy. Author for the
actual host; the POSIX and Windows manifests are distinct Revisions.

## Create a fresh workspace

Use the terminal with your verified executable, starting in a fresh parent
directory. Do not rerun setup from inside an existing `pactrun-demo` directory.
Run one platform variant. An existing `pactrun-demo`
destination is refused; choose another new directory to repeat the exercise.
Stop if setup reports an error. Never repair an old store by adding internal files.

PowerShell:

```powershell
& {
    $ErrorActionPreference = 'Stop'
    if (Test-Path -LiteralPath pactrun-demo) {
        throw 'pactrun-demo already exists. Choose a new tutorial directory.'
    }
    New-Item -ItemType Directory pactrun-demo | Out-Null
    New-Item -ItemType Directory pactrun-demo/pack, pactrun-demo/store/database, pactrun-demo/store/runtime-content, pactrun-demo/store/staging | Out-Null
    Set-Location -LiteralPath pactrun-demo
    $env:PACTRUN_STORAGE_ROOT = Join-Path (Get-Location) 'store'
}
```

POSIX shell:

```sh
mkdir pactrun-demo &&
mkdir -p pactrun-demo/pack pactrun-demo/store/database pactrun-demo/store/runtime-content pactrun-demo/store/staging &&
cd pactrun-demo &&
export PACTRUN_STORAGE_ROOT="$PWD/store"
```

The empty `database`, `runtime-content`, and `staging` directories are required
by this candidate. The environment selection affects later commands in this
terminal. Restore its previous value or close the terminal after the exercise.

## Continue

The [first Pack tutorial](./fundamentals/authoring-model.md) supplies a complete
manifest and script for each platform, then installs and runs the Pack. Use
[Pack fields](./reference/pack-fields.md) when changing declarations. Keep only
synthetic data in this workspace and inspect a retirement plan before cleanup.

For container-backed services, review the
[first Preview's permission limitation](../guides/retirement.md#container-permissions-in-the-first-preview).
Test retirement after the service has initialized real data: a successful Cleanup
Hook does not prove that Pactrun can reclaim foreign-owned service directories.

**Done:** `pactrun-demo/pack` exists, the selected store is isolated, and this
terminal can run the verified executable. Creating the directories has not yet
installed a Pack or created an Instance.
