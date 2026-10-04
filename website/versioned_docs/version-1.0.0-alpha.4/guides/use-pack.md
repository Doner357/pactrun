---
title: Install a Pack and create an Instance
---

# Install a Pack and create an Instance

Use this after [Pactrun is installed and verified](./installation.md). You need
a trusted Pack supplied by its author and a deliberately selected, prepared
[store](./data-location.md). Installing Pactrun and installing a Pack are different
operations. You do not need to write a manifest to use a supplied Pack.

The [first Instance tutorial](../introduction.md) uses an empty synthetic Pack.
It proves the basic command flow, not real service functionality. This procedure
uses a Pack with the capabilities you actually want to operate.

## 1. Install the supplied Pack

Replace `./supplied.pack` with its real path. A supported source or distribution
directory can also be used. Verify the delivery according to the author's instructions.

```text
pactrun pack install ./supplied.pack
```

Record the complete output line beginning with `exact:` as `<reference>`. It
identifies this immutable installed Revision. Installation acquires files and
metadata; it does not create an Instance or start a service.

## 2. Inspect before creating an Instance

```text
pactrun revision show <reference>
pactrun instance list
```

Read the available Actions, Inputs and capabilities, and the author's host-program
and service requirements. Choose an unused Instance name. The examples use `demo`;
if that name already exists, inspect it and choose another name rather than deleting
it to make the example fit.

## 3. Create and inspect

```text
pactrun instance create demo --revision <reference>
pactrun instance show demo
pactrun input list demo
pactrun action list demo
```

Creation may allocate declared service storage, but it does not automatically
start the service or create its live files. Missing required Inputs can leave the
new Instance incomplete. An empty Action list means this Pack has no ordinary
Actions; do not assume it contains one named `inspect`.

## Choose the next operation

- Required configuration is missing: [configure Inputs](./configure-inputs.md).
- A declared Action is ready: [plan and run it](./run-and-diagnose.md), following
  the author's service-specific instructions.
- You need Snapshot or Migration operations: use the
  [capability-specific procedures](../pactrun-users/operations/snapshots-migrations-and-recovery.md).
- You only want to export the Revision or manage an alias: use
  [Pack management](./pack-management.md).

**Done:** you have a known live Instance and have inspected its actual capabilities.
The other task pages use example names; replace them with this Instance's name and
declared IDs. They are choices, not commands to run one after another. If any step
fails, inspect the message and [recovery guidance](./recovery.md) before retrying.
