---
title: Author Migration paths
---

# Author Migration paths

A target Revision declares its inbound Migration edges from exact source
Revisions in the same Package lineage. Design each edge around explicit Input
transitions and service-resource requirements.
This is an optional tutorial after the [first Hook](../fundamentals/authoring-model.md)
and [Hook integration](./hooks-recovery-and-cleanup.md). It does not require the
Snapshot tutorial or reuse that tutorial's Instance.

## Design an edge

1. Identify the exact source Revision digest.
2. Describe target Inputs and transitions from source values.
3. Declare required source/target Inputs and produced target values.
4. Use a declarative edge when the supported transitions suffice.
5. Add a Hook only for the external work or transformation the edge requires.

Follow the closed field spellings in [Service and Migration fields](../reference/service-fields.md)
and [canonical Revision model](../reference/revision-format.md).
Do not infer a path from version labels or Package names.

## Protect data and authority

Distinguish source and target views and active/retained roles. Retained service
storage is not an implicit new active allocation. Declare reattachment or other
supported transitions explicitly. A declassification needs both the authored
transition and the operator's authorization.

Use detached operator-supplied Input bytes for the selected path. A later edge
must not depend on a host file remaining unchanged throughout execution.

## Hook-backed transformation

Follow the transform Session lifecycle, including acknowledged risk handling,
explicit target-ready publication, receipt, and successful process termination.
Ordinary Action completion is not a replacement for the Migration handshake.
The [Shell Loader](../reference/shell-loader.md) supplies `hook target-ready`
for the applicable Session; it does not infer target readiness from files.

## Validate the whole path

Install source and target Revisions in disposable storage. Discover paths, inspect
a plan, supply required per-target files, and execute the selected path. Check
the resulting Revision, bindings, resources, readiness, and Run record.

Test each edge's failure and a failure after an earlier committed edge. Explain
the last committed state in operator documentation. Do not promise whole-path
rollback. Keep source data and recovery instructions available until validation
is complete.

See [operator commands](../../pactrun-users/operations/snapshots-migrations-and-recovery.md)
and the [transformation completion rules](../reference/service-fields.md#service-transformation-completion).

## Worked example: carry one Input to a target Revision

Use a new disposable storage root prepared by the [authoring setup](../setup.md).
Start setup from a fresh parent directory, keeping the verified executable in
your terminal. Do not create a nested workspace inside an earlier `pactrun-demo`.
Create sibling directories `migration-source` and `migration-target`; run the
commands below from their parent directory. The example uses one Package lineage
and a plain Migration Hook. It does not perform a service-resource transformation.

### Install the source

Generate a Package ID and use it in both manifests. Put this source manifest in
`migration-source/pactrun.yaml`:

```yaml
source_format: "1.0-alpha.2"
package_id: "00000000000000000000000000000068"
revision:
  inputs:
    - {id: config, required: true, protection: normal}
runtime_content: {}
```

Create `config.txt` with known test text. Install and configure the source Instance:

```text
pactrun pack install ./migration-source --package-name migration-example --revision-name source
```

Use the local reference `migration-example:source` for this installed Revision.

```text
pactrun instance create migration-demo --revision migration-example:source --input-file config=./config.txt
```

### Declare the target

Put the following in `migration-target/pactrun.yaml`. Replace `SOURCE_DIGEST`
with `result.revision.content_digest` from
`pactrun --format json revision show migration-example:source`. Copy the complete
`sha256:…` value, not a local name or Package ID. Keep the same Package ID as the source.

```yaml
source_format: "1.0-alpha.2"
package_id: "00000000000000000000000000000068"
revision:
  inputs:
    - {id: settings, required: true, protection: normal}
  migrations:
    - source_revision_digest: "SOURCE_DIGEST"
      transitions:
        - {kind: carry, source: {role: active, input_id: config}, target_input_id: settings}
      requires_source: []
      requires_target: []
      produces_target: []
      hook:
        protocol_version: "1.0-alpha.1"
        launch: {kind: shell_loader, shell: sh, command: sh, script: script}
        args: []
        io: {terminal: output}
runtime_content:
  files:
    - {id: script, source: migrate.sh, path: migrate.sh, executable: false}
```

Create `migration-target/migrate.sh` on a POSIX host with `sh`:

```sh
printf 'Migration hook completed\n'
```

### Windows target variant

With PowerShell 7, use this complete `migration-target/pactrun.yaml` instead
of the POSIX target manifest. Replace the example Package ID with the **same**
ID as the source, and replace `SOURCE_DIGEST` with its complete `sha256:…`
component. Do not generate another Package ID for this target.

```yaml
source_format: "1.0-alpha.2"
package_id: "00000000000000000000000000000068"
revision:
  inputs:
    - {id: settings, required: true, protection: normal}
  migrations:
    - source_revision_digest: "SOURCE_DIGEST"
      transitions:
        - {kind: carry, source: {role: active, input_id: config}, target_input_id: settings}
      requires_source: []
      requires_target: []
      produces_target: []
      hook:
        protocol_version: "1.0-alpha.1"
        launch: {kind: shell_loader, shell: powershell_7, command: pwsh.exe, script: script}
        args: []
        io: {terminal: output}
runtime_content:
  files:
    - {id: script, source: migrate.ps1, path: migrate.ps1, executable: false}
```

Create `migration-target/migrate.ps1`:

```powershell
Write-Output 'Migration hook completed'
```

The Hook reports completion; the declared carry transition supplies `settings`.
It does not need to register a target output. Omitting the entire Hook and runtime
file produces a declarative-only variant; use a newly installed target reference
if you change the manifest.

### Execute and verify

Install the target:

```text
pactrun pack install ./migration-target --package-name migration-example --revision-name target
```

Use `migration-example:target` as `<target-reference>` below. Keep source and
target references separate. Discover the path:

```text
pactrun instance migration-paths migration-demo --to <target-reference> --no-trunc
```

Copy the complete value after `path_id:`, including its `mp1-` prefix, as
`<path-id>`. Do not copy the `route:` line or its labels.

```text
pactrun instance migrate migration-demo --to <target-reference> --path <path-id> --plan
pactrun instance migrate migration-demo --to <target-reference> --path <path-id>
pactrun instance show migration-demo
pactrun input export migration-demo settings --output ./settings-copy.txt
```

Expect `Migration hook completed`, a successful Run, the target Revision, and
a present required `settings` binding. Use a new export destination; an existing
file must not be silently overwritten.

### Compare the exported Input

Compare these two small tutorial files without decoding text or normalizing
line endings. A successful comparison prints `Input bytes match.`. A mismatch
or read error is not a pass; inspect the result before continuing.

PowerShell (the error preference is local to this script block):

```powershell
& {
  $ErrorActionPreference = 'Stop'
  $original = [IO.File]::ReadAllBytes((Resolve-Path -LiteralPath './config.txt').Path)
  $copy = [IO.File]::ReadAllBytes((Resolve-Path -LiteralPath './settings-copy.txt').Path)
  if ($original.Length -ne $copy.Length) { throw 'Files have different lengths.' }
  for ($i = 0; $i -lt $original.Length; $i++) {
    if ($original[$i] -ne $copy[$i]) { throw "Files differ at byte $i." }
  }
  Write-Output 'Input bytes match.'
}
```

POSIX shell (uses `cmp`):

```sh
cmp ./config.txt ./settings-copy.txt && printf '%s\n' 'Input bytes match.'
```

### Prepare an unrelated Package

Keep the same working directory and storage root. Create a separate source
folder; do not overwrite the source or target Pack.

PowerShell:

```powershell
New-Item -ItemType Directory migration-unrelated -ErrorAction Stop | Out-Null
```

POSIX shell:

```sh
mkdir ./migration-unrelated
```

Stop if creation fails. Generate a **new** Package ID; do not reuse the ID in
`migration-source` or `migration-target`:

```text
pactrun pack generate-id
```

Save this as UTF-8 `migration-unrelated/pactrun.yaml`, replacing the example ID
with the new generated value:

```yaml
source_format: "1.0-alpha.2"
package_id: "00000000000000000000000000000069"
revision: {}
runtime_content: {}
```

Install it with a separate local name:

```text
pactrun pack install ./migration-unrelated --package-name unrelated --revision-name initial
```

### Check the cross-Package refusal

Use `unrelated:initial` as `<unrelated-reference>`:

```text
pactrun instance migrate migration-demo --to <unrelated-reference> --plan
```

Expect exit 1 and a refusal that Migration must stay within one Package lineage.
This is an intentional negative result. Do not use recovery override to bypass
identity checks. A successful plan here would fail the exercise.

### Retire the test Instance

After inspection, review the deletion plan and retire the Instance:

```text
pactrun instance delete migration-demo --plan
pactrun instance delete migration-demo
```

Source files, exported comparison files and retained history remain available
for review. This one-edge exercise does not prove multi-edge failure recovery or
service-resource transformation; those require separate Pack-specific tests.
