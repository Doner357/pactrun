---
title: Package runtime files
---

# Package runtime files

Declare each runtime file with a content ID, source-relative path, installed
logical path, and executable flag. The author tutorial shows a script entry.
Start from the [working first Pack](./authoring-model.md). This page explains how
its file declaration controls acquisition and launch, then how to validate export.

For the POSIX starter, this root-level fragment places the same owned script at
a different installed logical path; the Hook still names Content ID `script`:

```yaml
runtime_content:
  files:
    - {id: script, source: inspect.sh, path: scripts/inspect.sh, executable: false}
```

For the Windows variant, use `source: inspect.ps1` and `path: scripts/inspect.ps1`.
Keep the source file
beside the manifest. Reinstall after changing this declaration and test the new
Revision; changing the file path does not rebind an existing Instance.

## Keep paths portable

Place source files beneath the selected Pack root. Use exact spelling and forward
slashes in manifest paths. Avoid symlinks, traversal segments, reserved names,
and filesystem aliases. Installation refuses unsafe paths rather than following
them to undeclared host content.

Installed content is acquired and identified independently of later source edits.
A Hook should use its declared launch and granted Session resources. It must not
assume the original source directory still exists.

## Choose a launch model

- **Shell Loader:** use the built-in Session lifecycle with a supported host shell.
- **Direct:** package a compatible executable that implements the Hook protocol.
- **Interpreter:** explicitly select a host program and declared script according
  to the launch contract; implement the required Hook integration.

Host programs are prerequisites. Document required tools and supported platforms.
Trusted Hooks have host access; runtime packaging is not a security sandbox.

## Distribution

After installing and validating a Revision, export it with `revision export`.
Test importing the exported Pack into isolated storage before distributing it.
Review portable metadata and avoid including secrets in runtime files.

**Done:** the exported Pack installs and its declared operation works without the
original source directory. Continue with [Hook integration](../managed-capabilities/hooks-recovery-and-cleanup.md)
for output, risk, and Cleanup behavior. Capability tutorials are optional after that.

Recipes are not a supported authoring feature. Existing links to this page are
retained for continuity; use explicit Pack declarations and Hooks.

More detail: [Pack fields and values](../reference/pack-fields.md).

<details>
<summary>Maintainer sources (optional)</summary>

Contracts: [source acquisition](../../spec/contracts/pack-source.md),
[runtime content](../../spec/contracts/recipes-and-runtime-content.md), and
[distribution](../../spec/contracts/pack-distribution.md).

</details>
