---
title: Author Snapshot capabilities
---

# Author Snapshot capabilities

This is an optional tutorial after the [first Hook](../fundamentals/authoring-model.md)
and [Hook integration](./hooks-recovery-and-cleanup.md). It does not require a
Migration example. Use synthetic data and a fresh authoring environment.

Define what Capture includes, how service consistency is established, and which
exact Revision can Restore the result. Provide operator-facing prerequisites,
capacity expectations, and warnings about replaced data.

## Capture

Declare the Capture parameters, access, and Hook. In a Shell Loader script,
`hook candidate` identifies the candidate area. Produce the intended bytes and
register explicit descriptors with `hook capture-register --file <descriptor>`.
The descriptor uses the exact role/path/candidate-path contract; copying arbitrary
Workspace files does not capture them automatically.

Coordinate service-owned live state with the service's supported backup mechanism.
A filesystem copy taken during a write is not necessarily application-consistent.
Preserve Input protection and accurately declare the captured roles.

## Restore

Declare Restore explicitly. `hook snapshot-content` provides the granted content
root and exact descriptor JSON inside the admitted Loader Session. Validate your
application-level preconditions before replacement work. Use granted resources;
do not search internal Pactrun storage for files.

Explain whether the service must be stopped and which data is overwritten.
Snapshot verification checks stored content integrity; application-level validity
requires the Pack's own checks.

## Failure and security

Test missing content, insufficient capacity, refused compatibility, Hook failure,
and cancellation. Verify the Instance and service state after each failure. Do
not advertise automatic rollback unless the owning operation contract provides it.

Snapshot exports may contain Secret and live service data. Document protected
storage and transport requirements. Export authorization does not encrypt a bundle.

## Acceptance exercise

For a real service Pack, configure Inputs, create known service data and Capture
a consistent Snapshot in disposable storage. Export both the Snapshot and the
exact installed Revision. Import into a **new store where the Snapshot is absent**
and Restore into a compatible test Instance. Compare the restored state and Run
outcome. An `already_present` import is a duplicate-import check, not evidence of
fresh-store reconstruction. The worked example below gives this sequence for
fixed demo bytes and a separate incompatible-Revision refusal.

More detail: [Pack fields and values](../reference/pack-fields.md).

<details>
<summary>Maintainer sources (optional)</summary>

Contracts: [Snapshot capabilities](../../spec/contracts/snapshots-and-managed-data.md),
[integrity](../../spec/contracts/snapshot-integrity.md),
[bundle](../../spec/contracts/snapshot-bundle.md), and
[execution](../../spec/execution/m4-snapshot-lifecycle-approval-baseline.md).

</details>

## Worked example: capture and read one value

Use a new disposable storage root prepared by the [authoring setup](../setup.md).
Begin setup in a fresh parent directory, not inside a previous `pactrun-demo`.
This POSIX example needs `sh` and `python3` on the host. It captures fixed demo
bytes and verifies them in Restore; it does not back up or replace a real service.
Work from the `pactrun-demo` directory prepared by that tutorial, with its
original storage root at `./store`. Keep this working directory throughout the
example; changing `PACTRUN_STORAGE_ROOT` later selects another store without
changing where source and backup files are read. Create this source directory:

```text
snapshot-demo/
  pactrun.yaml
  snapshot.sh
```

Generate a Package ID and replace the example ID in `snapshot-demo/pactrun.yaml`:

```yaml
source_format: "1.0-alpha.1"
package_id: "00000000000000000000000000000067"
revision:
  snapshot:
    capture:
      parameters: []
      access: observe
      hook:
        protocol_version: "1.0-alpha.1"
        launch: {kind: shell_loader, shell: sh, command: sh, script: script}
        args: [capture]
        io: {terminal: output}
    restore:
      parameters: []
      hook:
        protocol_version: "1.0-alpha.1"
        launch: {kind: shell_loader, shell: sh, command: sh, script: script}
        args: [restore]
        io: {terminal: output}
runtime_content:
  files:
    - {id: script, source: snapshot.sh, path: snapshot.sh, executable: false}
```

Create `snapshot-demo/snapshot.sh`:

```sh
if [ "$1" = capture ]; then
  root=$("$PACTRUN_EXECUTABLE" hook candidate) || exit 1
  scratch=$("$PACTRUN_EXECUTABLE" hook workspace) || exit 1
  printf 'demo-state\n' > "$root/value" || exit 1
  printf '%s' '{"role":"state","path":"demo/value","candidate_path":"value"}' > "$scratch/descriptor.json" || exit 1
  "$PACTRUN_EXECUTABLE" hook capture-register --file "$scratch/descriptor.json"
  exit $?
fi
scratch=$("$PACTRUN_EXECUTABLE" hook workspace) || exit 1
"$PACTRUN_EXECUTABLE" hook snapshot-content --output "$scratch/content.json" || exit 1
python3 - "$scratch/content.json" <<'PY'
import json, pathlib, sys
content = json.loads(pathlib.Path(sys.argv[1]).read_text())
item = content['logical_descriptors'][0]
value = (pathlib.Path(content['readonly_root_path']) / item['materialized_path']).read_bytes()
assert value == b'demo-state\n'
print('Verified snapshot bytes')
PY
```

### Windows Snapshot variant

PowerShell 7 is a host prerequisite. Use this complete
`snapshot-demo/pactrun.yaml` instead of the POSIX manifest, with your generated
Package ID. Both launch declarations and the runtime file are already adjusted.

```yaml
source_format: "1.0-alpha.1"
package_id: "00000000000000000000000000000067"
revision:
  snapshot:
    capture:
      parameters: []
      access: observe
      hook:
        protocol_version: "1.0-alpha.1"
        launch: {kind: shell_loader, shell: powershell_7, command: pwsh.exe, script: script}
        args: [capture]
        io: {terminal: output}
    restore:
      parameters: []
      hook:
        protocol_version: "1.0-alpha.1"
        launch: {kind: shell_loader, shell: powershell_7, command: pwsh.exe, script: script}
        args: [restore]
        io: {terminal: output}
runtime_content:
  files:
    - {id: script, source: snapshot.ps1, path: snapshot.ps1, executable: false}
```

Create `snapshot-demo/snapshot.ps1`:

```powershell
if ($args[0] -eq 'capture') {
  $root = & $env:PACTRUN_EXECUTABLE hook candidate
  if ($LASTEXITCODE -ne 0) { exit 1 }
  $scratch = & $env:PACTRUN_EXECUTABLE hook workspace
  if ($LASTEXITCODE -ne 0) { exit 1 }
  [IO.File]::WriteAllBytes((Join-Path $root 'value'), [Text.Encoding]::UTF8.GetBytes("demo-state" + [char]10))
  $descriptor = Join-Path $scratch 'descriptor.json'
  [IO.File]::WriteAllText($descriptor, '{"role":"state","path":"demo/value","candidate_path":"value"}', [Text.UTF8Encoding]::new($false))
  & $env:PACTRUN_EXECUTABLE hook capture-register --file $descriptor
  exit $LASTEXITCODE
}
$contentText = & $env:PACTRUN_EXECUTABLE hook snapshot-content
if ($LASTEXITCODE -ne 0) { exit 1 }
$content = $contentText | ConvertFrom-Json
$file = Join-Path $content.readonly_root_path $content.logical_descriptors[0].materialized_path
if ([IO.File]::ReadAllText($file) -cne ("demo-state" + [char]10)) { exit 1 }
Write-Output 'Verified snapshot bytes'
```

### Exercise Capture and Restore

Install the example Pack:

```text
pactrun pack install ./snapshot-demo
```

Save the full output line beginning with `exact:` as `<snapshot-reference>`.
Replace every angle-bracket placeholder below with the corresponding value,
including removing the brackets. Do not substitute the separate table columns
from `revision list` for that complete reference.

```text
pactrun instance create snapshot-demo --revision <snapshot-reference>
pactrun snapshot capture snapshot-demo --plan
pactrun snapshot capture snapshot-demo
pactrun snapshot list --instance snapshot-demo --no-trunc
pactrun snapshot verify <snapshot-id>
pactrun snapshot restore snapshot-demo <snapshot-id> --plan
pactrun snapshot restore snapshot-demo <snapshot-id>
```

Save the value after `snapshot:` in the Capture output as `<snapshot-id>`;
the separately printed `run:` value identifies the Run. Expect
`Verified snapshot bytes` and a successful Restore Run. This first Restore uses
the original store. It does not yet prove recovery from exported files.

### Export the recovery files

The original root is still `./store`. Export the exact installed Revision so
that recovery does not depend on recreating source files or their line endings.
A Snapshot bundle does not include the Pack's executable runtime content.

```text
pactrun revision export <snapshot-reference> --output ./snapshot-pack
pactrun snapshot export <snapshot-id> --output ./backup --authorize-sensitive-export
```

Expect `snapshot-pack.pack` and `backup.snapshot` in the working directory,
outside both stores. The commands append their suffixes automatically. Use new
output names if files already exist; do not overwrite an earlier backup.
Bundles are unencrypted and can contain sensitive bytes. These examples contain
only the fixed demo value.

### Prepare a fresh destination store

Stay in `pactrun-demo`. The following guard refuses an existing destination.
Stop on any error; do not fall through to import into the old store.

PowerShell:

```powershell
if (Test-Path -LiteralPath './recovery-store') { throw 'Use a fresh working directory; recovery-store already exists.' }
New-Item -ItemType Directory recovery-store/database, recovery-store/runtime-content, recovery-store/staging -ErrorAction Stop | Out-Null
$env:PACTRUN_STORAGE_ROOT = Join-Path (Get-Location) 'recovery-store'
$env:PACTRUN_STORAGE_ROOT
```

POSIX shell:

```sh
if [ -e ./recovery-store ] || [ -L ./recovery-store ]; then
  printf '%s\n' 'Use a fresh working directory; recovery-store already exists.' >&2
  exit 1
fi
mkdir -p recovery-store/database recovery-store/runtime-content recovery-store/staging || exit 1
export PACTRUN_STORAGE_ROOT="$PWD/recovery-store"
printf '%s\n' "$PACTRUN_STORAGE_ROOT"
```

The printed root must end in `recovery-store`. Keep the source store intact.
Install the exported Pack into the new store, then inspect its empty Snapshot list:

```text
pactrun pack install ./snapshot-pack.pack
pactrun snapshot list --no-trunc
```

The install output must equal the original `<snapshot-reference>`, and no
Snapshots should be listed. If either check fails, stop before importing.

### Import and restore in the destination

All commands in this section use `recovery-store`:

```text
pactrun snapshot import ./backup.snapshot
pactrun snapshot list --no-trunc
pactrun snapshot verify <snapshot-id>
pactrun instance create restored-demo --revision <snapshot-reference>
pactrun snapshot restore restored-demo <snapshot-id> --plan
pactrun snapshot restore restored-demo <snapshot-id>
```

The previously absent Snapshot must now appear with the original ID. An
`already_present` response means the destination was not fresh; do not count it
as this recovery exercise passing. Expect valid verification, the message `Verified snapshot bytes`, and a
successful Restore Run in the destination. A script failure must
remain non-success; do not remove the byte check to obtain a pass.

### Prepare an incompatible Revision

Keep using `recovery-store`. This creates a second source directory and changes
only the runtime script's success message. Keep the Package ID unchanged. It is
a deliberately different Revision within the same lineage, not a new Package.

PowerShell:

```powershell
if (Test-Path -LiteralPath './snapshot-other') { throw 'snapshot-other already exists.' }
Copy-Item -LiteralPath './snapshot-demo' -Destination './snapshot-other' -Recurse -ErrorAction Stop
$file = Join-Path (Get-Location) 'snapshot-other/snapshot.ps1'
$text = [IO.File]::ReadAllText($file)
[IO.File]::WriteAllText($file, $text.Replace('Verified snapshot bytes', 'Verified other revision bytes'), [Text.UTF8Encoding]::new($false))
```

POSIX shell (uses the same Python 3 prerequisite as the demo Hook):

```sh
python3 - <<'PY'
from pathlib import Path
import shutil
shutil.copytree('snapshot-demo', 'snapshot-other')
file = Path('snapshot-other/snapshot.sh')
file.write_text(file.read_text().replace('Verified snapshot bytes', 'Verified other revision bytes'))
PY
```

Install the second source:

```text
pactrun pack install ./snapshot-other
```

Save the new `exact:` line as `<other-reference>` and confirm it differs from
`<snapshot-reference>`. Do not overwrite the saved original reference.

### Check the incompatible Restore refusal

```text
pactrun instance create snapshot-other --revision <other-reference>
pactrun snapshot restore snapshot-other <snapshot-id>
```

The second command must fail with the exact-producer compatibility refusal,
exit 1, and no `Verified other revision bytes` output. This is an expected
negative result. Do not add a recovery override to bypass it.

### Retire destination test objects

Still in `recovery-store`, finish inspection before removing these test objects:

```text
pactrun instance delete restored-demo --plan
pactrun instance delete restored-demo
pactrun instance delete snapshot-other --plan
pactrun instance delete snapshot-other
pactrun snapshot delete <snapshot-id>
```

### Return to the source store

Stay in the same working directory. Select the original `./store` again.

PowerShell:

```powershell
$env:PACTRUN_STORAGE_ROOT = Join-Path (Get-Location) 'store'
$env:PACTRUN_STORAGE_ROOT
```

POSIX shell:

```sh
export PACTRUN_STORAGE_ROOT="$PWD/store"
printf '%s\n' "$PACTRUN_STORAGE_ROOT"
```

Inspect the original Snapshot, then retire the source example:

```text
pactrun snapshot list --instance snapshot-demo --no-trunc
pactrun instance delete snapshot-demo --plan
pactrun instance delete snapshot-demo
pactrun snapshot delete <snapshot-id>
```

The source Snapshot should still be listed before deletion; destination cleanup
does not delete it. The exported files remain in the working directory. Restore
the environment variable's previous value or close this dedicated tutorial
terminal when finished. Do not apply these cleanup commands to real service data.
