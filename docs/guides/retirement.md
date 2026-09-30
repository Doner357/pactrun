---
title: Retire Instances and reclaim storage
---

# Retire Instances and reclaim storage

Choose retirement according to what should happen to service data. Review the
Pack's Cleanup behavior and maintain independent backups before deletion.
This is a deliberate lifecycle task, not a required follow-up to installation
or a successful Action. Identify the live Instance or retained deletion attempt
first. If you only want to inspect an Instance, use [inspection and operation guidance](./run-and-diagnose.md).

## Delete with Cleanup

```text
pactrun instance delete demo --plan
pactrun instance delete demo
```

Inspect the plan before execution. Use `--if-version <token>` when acting on a
specific observed state. Deletion follows the authored Cleanup contract and
retained deletion obligations. If Cleanup fails, inspect the attempt rather
than assuming the Instance or service data is gone.

```text
pactrun instance deletion list
pactrun instance deletion show <instance-id>
pactrun run show <run-id>
```

Manual completion confirmation is an explicit assertion of completed Cleanup.
Use it only after verifying the required external work, with the matching
attempt and state token shown by inspection.

## Abandon and retain service data

```text
pactrun instance abandon demo --plan
pactrun instance abandon demo
pactrun service-storage detached list --no-trunc
```

Abandonment ends Pactrun management and preserves remaining service data.
It is not a service shutdown or a backup. Detached allocations remain separate
from ordinary garbage collection.

`service-storage detached discard <allocation-id> --confirm-discard` is
irreversible disposal of the selected allocation. Inspect it, stop dependent
service use, and verify your backups before invoking it.

## Remove retained objects deliberately

Snapshot, Run, Artifact, and Revision deletion have separate commands and
retention constraints. Export anything you need before deleting it. Artifact
export requires `--authorize-sensitive-export`; choose a protected destination.

```text
pactrun storage gc --plan
pactrun storage gc
```

Garbage collection is store-wide: the plan and execution cover eligible
unreferenced managed content throughout the selected store, not only the Instance
used in the examples above. Review the selected store and the complete plan. It does not
replace Instance retirement or authorize discarding detached service data.

More detail: [User reference](../pactrun-users/reference/index.md).

<details>
<summary>Maintainer sources (optional)</summary>

Contracts: [managed-object lifecycle](../spec/behavior/managed-object-lifecycle.md)
and [Instance retirement](../spec/execution/m7-instance-retirement.md).

</details>
