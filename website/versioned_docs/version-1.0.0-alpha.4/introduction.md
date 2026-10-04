---
title: Your first Instance
---

# Your first Instance

This tutorial creates an empty Pack and an Instance in an isolated storage root.
It does not launch a Hook or manage a service. Allow about one short terminal
session. First complete [installation and command verification](./guides/installation.md).
Keep that terminal open so the verified command remains available. This tutorial
supplies its own empty Pack; you do not need a real service or prior YAML knowledge.

- A **Package** is a stable authored lineage.
- A **Revision** is one exact immutable definition and its runtime files.
- An **Instance** is the managed object you create from a Revision.

## 1. Choose an isolated directory

Create a new working directory and run all steps there. Keep it separate from
real service data. Select a storage directory that does not already contain a
Pactrun installation.

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

If setup reports an error, stop before running the next step. Both variants
refuse an existing `pactrun-demo` directory and select the storage root only
after directory creation succeeds. Use a new directory to repeat the tutorial.

The candidate requires the empty storage directories shown above. Create them
only in this new tutorial root; do not modify an existing store to bypass a
compatibility refusal. Pactrun creates its database during the first write.

The environment variable affects subsequent commands in this terminal. Restore
its previous value when finished, or close this dedicated terminal.

## 2. Create the Pack source

Run `pactrun pack generate-id` and copy the complete identifier. Create
`pack/pactrun.yaml` as UTF-8 text with the following content. Replace the
example package ID with the identifier you generated.

```yaml
source_format: "1.0-alpha.1"
package_id: "00000000000000000000000000000065"
revision: {}
runtime_content: {}
```

Empty Inputs, Actions, Migrations, and runtime files use the source format's
empty defaults. This Pack has no Snapshot or Cleanup capability.

## 3. Install and create an Instance

```text
pactrun pack install ./pack
```

### Copy a Revision reference

Copy the complete line beginning with `exact:` from the **pack install output**.
Keep the Package ID, slash, `sha256:` prefix and complete digest together. Its
shape is `exact:<package-id>/sha256:<digest>`; the angle-bracket parts here are
placeholders, not values to type.

You can inspect installed Revisions with:

```text
pactrun revision list --no-trunc
```

That list displays the Package ID and digest in separate columns. A row is not
a ready-to-paste reference; use the full install-output line saved above.
In the following command, replace `<reference>`, including the angle brackets:

```text
pactrun instance create demo --revision <reference>
pactrun instance show demo
pactrun input list demo
pactrun action list demo
```

The Instance is bound to the installed Revision. Input and Action lists are
empty. Readiness describes declared configuration; this example runs no service.

## 4. Retire the example

```text
pactrun instance delete demo --plan
pactrun instance delete demo
pactrun instance history list
```

This empty Pack needs no Cleanup Hook. Deletion ends the live Instance while
retained history remains discoverable. Do not generalize this result to a Pack
with service resources; read [retirement](./guides/retirement.md) first.

## If a step fails

Check the selected storage root, source spelling, and complete Revision reference.
Unsupported old storage is refused; do not delete a real store to bypass that
refusal. A reused Instance name may already exist: inspect it before choosing a
new name. Preserve diagnostics rather than repeatedly retrying a write.

## What you have completed

You have installed a synthetic Revision, created and inspected an Instance, and
retired that empty example. The store and retained history remain; `demo` is no
longer a live Instance. This Pack supplies no Inputs, Actions, Snapshot, or Cleanup
Hook, so it is not the prerequisite Instance for later service-operation examples.

**Continue as a user:** [install a supplied Pack and create an Instance](./guides/use-pack.md),
then choose a task based on its actual capabilities. **Choose authoring instead:**
open the [Pack author guide](./package-authors/index.md). Authoring is an optional
role change, not the next required step for operating a Pack.
