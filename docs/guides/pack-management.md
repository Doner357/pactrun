---
title: Export Packs and manage metadata
---

# Export Packs and manage metadata

Use this task for an already installed Revision. For first installation and
Instance creation, follow [install a supplied Pack](./use-pack.md). Export and
metadata management do not require a live Instance.

## Identify the installed Revision

```text
pactrun revision list --no-trunc
pactrun revision show <reference>
```

Use the complete `exact:` reference recorded at installation. A list row displays
identity components separately; do not copy the entire row as an operand.

## Export an installed Revision

```text
pactrun revision export <reference> --output ./release
```

The destination is `release.pack`. Pactrun always appends the suffix, so passing
`release.pack` produces `release.pack.pack`. Choose a new destination; do not
assume force-overwrite support. Use `--include-portable-metadata` when you want
the export to carry eligible portable metadata.

## Use local aliases, notes, and trust labels

```text
pactrun revision metadata show <reference>
pactrun revision alias set demo-current <exact-reference> --expect-absent
pactrun revision alias show demo-current
```

Metadata writes require explicit expected state. For updates, inspect the value
and pass its exact expectation; conflicts must be resolved by reinspection.
Local trust labels describe your assessment. They do not authenticate downloaded
code or sandbox a Hook.

Notes and trust assessments have separate inspection and conditional writes:

```text
pactrun revision note show <reference>
pactrun revision trust show <reference>
pactrun revision note set <exact-reference> --value "reviewed locally" --expect-absent
pactrun revision trust set <exact-reference> <trusted-or-distrusted> --expect-absent
```

Replace `<trusted-or-distrusted>` with your actual choice, `trusted` or
`distrusted`; do not copy an assessment you have not made. These creation examples
expect no previous value. If one exists, use its inspected value with `--expect`
instead. The [command overview](/commands) lists the corresponding clear forms.
Do not store secrets in aliases or notes.

Portable metadata is distinct from local metadata. During installation, select
`--metadata-conflict keep` or `overwrite` deliberately if metadata conflicts.
Neither policy changes immutable Revision identity.

## Move an Instance to another Revision

Install the target Revision, then inspect `instance migration-paths` and use a
declared Migration. Changing an alias does not rebind an existing Instance.
See [Migration operations](../pactrun-users/operations/snapshots-migrations-and-recovery.md).

**Done:** verify the reported export destination or inspect the changed metadata.
Neither result implies that an Instance changed Revision. Return to the
[user task index](./index.md) to choose an unrelated operation.

More detail: [User reference](../pactrun-users/reference/index.md).

<details>
<summary>Maintainer sources (optional)</summary>

Contracts: [Pack distribution](../spec/packages/distribution.md) and
[metadata commands](../spec/interfaces/commands.md).

</details>
