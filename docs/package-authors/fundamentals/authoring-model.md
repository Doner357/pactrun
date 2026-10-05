---
title: Author your first Pack
---

# Author your first Pack

This tutorial adds an observable Shell Hook to a minimal Pack. Use a disposable
storage root as shown in [authoring setup](../setup.md). You need
Pactrun and a supported host interpreter. The POSIX example uses `sh`.

## Create the files

Work from the disposable `pactrun-demo` directory established by the
authoring setup. Put the manifest and script in its `pack` subdirectory. If
you previously used the user tutorial's empty Pack, replace that manifest; keep the isolated
storage-root environment selection.

```text
pactrun-demo/
  pack/
    pactrun.yaml
    inspect.sh       # inspect.ps1 for the Windows variant
  store/
```

Generate a Package ID with `pactrun pack generate-id`. Create `pack/pactrun.yaml`
and replace the example ID with that complete value:

```yaml
source_format: "1.0-alpha.1"
package_id: "00000000000000000000000000000066"
revision:
  actions:
    - id: inspect
      access: observe
      parameters: []
      outputs: []
      hook:
        protocol_version: "1.0-alpha.1"
        launch: {kind: shell_loader, shell: sh, command: sh, script: script}
        args: []
        io: {terminal: output}
runtime_content:
  files:
    - {id: script, source: inspect.sh, path: inspect.sh, executable: false}
```

Create `pack/inspect.sh` beside the manifest:

```sh
printf 'Hello from the Pack\n'
"$PACTRUN_EXECUTABLE" hook workspace
```

The Loader establishes the Session before running the script. The helper returns
the granted Workspace. Normal zero exit requests success; Pactrun still completes
its protocol and durable outcome checks.

### Windows variant

The host must have PowerShell 7 available. On Windows, use this complete
`pack/pactrun.yaml` instead of the POSIX manifest. Replace its example Package
ID with your generated value; do not append a second YAML document.

```yaml
source_format: "1.0-alpha.1"
package_id: "00000000000000000000000000000066"
revision:
  actions:
    - id: inspect
      access: observe
      parameters: []
      outputs: []
      hook:
        protocol_version: "1.0-alpha.1"
        launch: {kind: shell_loader, shell: powershell_7, command: pwsh.exe, script: script}
        args: []
        io: {terminal: output}
runtime_content:
  files:
    - {id: script, source: inspect.ps1, path: inspect.ps1, executable: false}
```

Save the script as `pack/inspect.ps1`:

```powershell
Write-Output 'Hello from the Pack'
& $env:PACTRUN_EXECUTABLE hook workspace
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
```

The Loader does not override execution policy or shell error preferences. Handle
script failures explicitly. POSIX and Windows shell alternatives are distinct
Revision content; they do not automatically fall back to each other.

## Install, inspect, and run

```text
pactrun pack install ./pack
pactrun revision list --no-trunc
pactrun instance create hook-demo --revision <reference>
pactrun action show hook-demo inspect
pactrun invoke hook-demo inspect --plan
pactrun invoke hook-demo inspect
pactrun run list hook-demo
pactrun run show <run-id>
```

Copy the full line beginning with `exact:` from `pack install` as `<reference>`;
the list separates its components into columns. Use the actual source directory.
Copy the value after `run:` from the invocation output as `<run-id>`. Expect the greeting,
a Workspace path, and a successful Run. The Workspace is scratch; do not keep its
path as durable service state. If admission fails, check host interpreter availability,
source paths, and declared shell kind before retrying.

## Continue authoring

- [Actions, Inputs, and parameters](./actions-inputs-and-parameters.md)
- [Runtime files](./recipes-and-runtime-content.md)
- [Hooks, recovery, and Cleanup](../managed-capabilities/hooks-recovery-and-cleanup.md)
- [Snapshots and managed data](../managed-capabilities/snapshots-and-managed-data.md)
- [Migrations](../managed-capabilities/migrations.md)

Keep the Package ID stable for revisions of one lineage. Install a changed Pack
to obtain a new Revision; an existing Instance stays bound until an allowed transition.
Retire the disposable Instance with `instance delete` after reviewing its plan.

More detail: [Pack fields and values](../reference/pack-fields.md).

<details>
<summary>Maintainer sources (optional)</summary>

Contracts: [Pack source](../../spec/packages/source-format.md),
[Revision model](../../spec/packages/revision-format.md),
[Shell Loader](../../spec/interfaces/shell-loader.md).

</details>
