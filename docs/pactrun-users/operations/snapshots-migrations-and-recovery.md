---
title: Capture, restore, and migrate
---

# Capture, restore, and migrate

Snapshot and Migration capabilities are authored by each Pack. Install the
required Revisions and inspect the operation before executing a write.
Start with a [live Instance](../../guides/use-pack.md), the author's service
prerequisites, and any required Inputs. Capture/Restore requires the corresponding
declared capability; Migration requires a declared target-owned path. These are
independent procedures. You do not need to run all sections, and the empty first
Instance tutorial supplies none of these capabilities.

## Capture a Snapshot

```text
pactrun snapshot capture demo --plan
pactrun snapshot capture demo
pactrun snapshot list --instance demo --no-trunc
pactrun snapshot show <snapshot-id>
pactrun snapshot verify <snapshot-id>
```

A Snapshot contains the declared captured data. It does not imply a complete
backup of every host resource. Capture can require a running service or other
Pack-specific coordination; follow the author's prerequisites.

## Export and import

```text
pactrun snapshot export <snapshot-id> --output ./backup --authorize-sensitive-export
pactrun snapshot import ./backup.snapshot
```

Export always appends `.snapshot`. Choose a new destination and protect it;
bundles can contain Secret and service data. Import/export use filesystem paths.
Verification checks integrity; it does not prove that application data is
semantically correct or that an arbitrary Revision can restore it.

An `already_present` import is a duplicate-import result. For a recovery
rehearsal, use a new store where the Snapshot is absent, install the exact
Revision, then import, verify and Restore. The
[fresh-store worked example](../../package-authors/managed-capabilities/snapshots-and-managed-data.md#export-the-recovery-files)
includes both platform setups and cleanup; Snapshot bundles do not carry the
Pack's executable runtime content.

## Restore an existing Instance

```text
pactrun snapshot restore demo <snapshot-id> --plan
pactrun snapshot restore demo <snapshot-id>
```

Restore can replace managed and service data according to the authored contract.
Check exact compatibility and the planned effects. A recovery override bypasses
only its specified trust guard, not identity, readiness, capacity, or conflicts.

To create a new Instance from a compatible Snapshot:

```text
pactrun instance create restored --revision <reference> --restore-from <snapshot-id>
```

Create-and-restore has no `--plan` and cannot be combined with initial Input
options. Inspect the Snapshot and Revision first.

## Migrate to a new Revision

```text
pactrun instance migration-paths demo --to <target-reference>
pactrun instance migrate demo --to <target-reference> --path <path-id> --plan
pactrun instance migrate demo --to <target-reference> --path <path-id>
```

Use a complete path ID from discovery. If more candidates exist, follow the
reported pagination. Supply required per-target Input files with the exact
`<target-digest>/<input-id>=<path>` selector. Review any declassification separately;
its authorization is not a general permission to bypass protection.

A multi-edge failure may leave an earlier edge committed. Inspect the Run and
current Instance before retrying; do not assume automatic rollback to the source.

## After failure

Inspect `run show` and the Instance's current Revision, readiness, and recovery
state. Repair external service coherence using the Pack's procedure before
acknowledging manual recovery. Preserve diagnostics and remaining Snapshot data.

More detail: [User reference](../reference/index.md).

<details>
<summary>Maintainer sources (optional)</summary>

Contracts: [Snapshot operations](../../spec/behavior/m4-snapshot-command-reference.md),
[Migration commands](../../spec/behavior/m5-migration-command-reference.md), and
[recovery](../../spec/execution/recovery-and-reconciliation.md).

</details>
