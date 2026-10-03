---
title: Install a standalone archive
---

# Install a standalone archive

This is the advanced offline/engineering fallback. Prefer the
[Scoop or Homebrew installation](./installation.md) for ordinary use.

This shared setup is for users and Pack authors. It ends with a verified
`pactrun` command in the current terminal. It does not create an Instance or
choose a store for real service data.

## Before you start

You need the standalone
archive for your platform and its SHA-256 checksum from a trusted delivery source.
If you do not have them, obtain that delivery first. A checksum verifies the
supplied bytes, not an unknown publisher's trustworthiness.

| Platform | Archive used by this procedure | Tools |
| --- | --- | --- |
| Windows x86-64 | `pactrun-1.0.0-alpha.3-windows-x86_64-standalone.zip` | PowerShell with `Get-FileHash` and `Expand-Archive` |
| Linux x86-64 | `pactrun-1.0.0-alpha.3-linux-x86_64-standalone.tar.gz` | POSIX shell, `sha256sum`, `tar` |

Use the actual supplied filename if your delivery differs. These standalone
archives contain the executable at the archive root. The normal/test native
package archives have a different layout; do not substitute one here.

If Pactrun is already installed by an approved Scoop or Homebrew source, keep
using that package's launcher and proceed to [verify the selected command](#verify-the-selected-command).
Return to the primary installation guide for the maintained package sources.

## Windows: install a supplied standalone archive

Open a terminal in a working directory containing the archive. Replace
`PASTE_TRUSTED_SHA256` with its complete 64-digit checksum, then run the block.
It refuses an existing `pactrun-bin` directory and only changes this terminal's PATH.

```powershell
& {
    $ErrorActionPreference = 'Stop'
    $archive = (Resolve-Path -LiteralPath './pactrun-1.0.0-alpha.3-windows-x86_64-standalone.zip').Path
    $expected = 'PASTE_TRUSTED_SHA256'
    if ($expected -notmatch '^[0-9a-fA-F]{64}$') { throw 'Supply the trusted SHA-256 checksum first.' }
    if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $expected) { throw 'Checksum mismatch; do not extract or run this archive.' }
    $installDir = Join-Path (Get-Location) 'pactrun-bin'
    if (Test-Path -LiteralPath $installDir) { throw 'pactrun-bin already exists; choose a new installation directory.' }
    New-Item -ItemType Directory -Path $installDir | Out-Null
    Expand-Archive -LiteralPath $archive -DestinationPath $installDir
    $program = Join-Path $installDir 'pactrun.exe'
    & $program --version
    if ($LASTEXITCODE -ne 0) { throw 'The supplied executable did not start successfully.' }
    & $program --help
    if ($LASTEXITCODE -ne 0) { throw 'The supplied executable did not report help successfully.' }
    $env:Path = $installDir + [IO.Path]::PathSeparator + $env:Path
    $selected = Get-Command pactrun -ErrorAction Stop
    if ($selected.CommandType -ne 'Application' -or $selected.Source -ne $program) { throw 'Another command or alias shadows pactrun; resolve it before continuing.' }
    Write-Output "Pactrun is ready in this terminal: $program"
}
```

## Linux: install a supplied standalone archive

Use the directory containing the archive, replace the checksum placeholder, and
run this block. It refuses an existing destination and leaves your working
directory unchanged.

```sh
pactrun_install() {
    archive="$PWD/pactrun-1.0.0-alpha.3-linux-x86_64-standalone.tar.gz"
    expected='PASTE_TRUSTED_SHA256'
    case "$expected" in ''|*[!0-9a-fA-F]*) printf '%s\n' 'Supply the trusted SHA-256 checksum first.' >&2; return 1 ;; esac
    [ "${#expected}" -eq 64 ] || { printf '%s\n' 'The checksum must contain 64 hexadecimal digits.' >&2; return 1; }
    actual=$(sha256sum "$archive") || return 1
    actual=${actual%% *}
    expected=$(printf '%s' "$expected" | tr 'A-F' 'a-f')
    [ "$actual" = "$expected" ] || { printf '%s\n' 'Checksum mismatch; do not extract or run this archive.' >&2; return 1; }
    install_dir="$PWD/pactrun-bin"
    if [ -e "$install_dir" ] || [ -L "$install_dir" ]; then printf '%s\n' 'pactrun-bin already exists; choose a new installation directory.' >&2; return 1; fi
    mkdir "$install_dir" || return 1
    tar -xzf "$archive" -C "$install_dir" || return 1
    "$install_dir/pactrun" --version || return 1
    "$install_dir/pactrun" --help || return 1
    PATH="$install_dir:$PATH"
    export PATH
    hash -r
    [ "$(command -v pactrun)" = "$install_dir/pactrun" ] || { printf '%s\n' 'Another command or alias shadows pactrun; resolve it before continuing.' >&2; return 1; }
    printf 'Pactrun is ready in this terminal: %s\n' "$install_dir/pactrun"
}
pactrun_install
```

## Verify the selected command

Check `Get-Command pactrun` on PowerShell or `command -v pactrun` on a POSIX shell.
It must identify the installation you intend to use. Then run:

```text
pactrun --version
pactrun --help
```

Expect the version supplied with your delivery and the command overview, with
successful exit status. These checks do not need a managed store. If any step
fails, stop before the next tutorial: inspect the checksum, archive type, platform,
selected path, and any command alias. Do not overwrite an existing installation
or change service data to make verification pass.

The PATH change above lasts only for this terminal. Keep it open for the next
tutorial and record the printed absolute installation directory. In a new
terminal, add that verified directory to the session PATH and verify command
resolution again; do not re-extract over the existing directory. No global PATH,
package repository, or shell profile was changed by these steps.

## Continue with your task

- **New user:** [Create your first Instance](../introduction.md). It creates its
  own isolated test store; you do not need to study internal storage first.
- **Pack author:** [Prepare an authoring workspace](../package-authors/setup.md).
- **Operating real service data:** [Choose a data location](./data-location.md),
  then [install a supplied Pack and create an Instance](./use-pack.md).

An executable update is separate from managed-data compatibility. Keep backups;
unsupported stores are refused rather than upgraded or deleted automatically.

<details>
<summary>Maintainer sources (optional)</summary>

[Delivery evidence](../development/e-implementation-status.md),
[release gates](../development/release-readiness.md),
[versioning](../spec/foundations/product-versioning-and-compatibility.md), and
[persistence](../spec/persistence/persistence-baseline.md).

</details>
