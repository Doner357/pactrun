---
title: Commands and output
---

# Commands and output

Use `pactrun --help` to inspect the executable you are running. The website's
**Command help** page is generated from this source tree's CLI declaration and
identifies its product version. It is reference material for this tree, not a
claim about every installed binary.

Help follows the command hierarchy. For example, `pactrun instance --help`
lists Instance operations, while `pactrun instance create --help` shows creation
forms and options. Help paths need no operation operands or Store. The same
queries work with `--format json` or `--format jsonl` before the command; machine
responses include a structured catalog for the requested scope.

For Action parameter syntax, timeouts, defaults, and examples, read the
[Invoke command reference](./invoke-reference.md), or run `pactrun invoke --help`.

## Choose a command family

| Task | Commands | Procedure |
| --- | --- | --- |
| Install or export a Revision | pack, revision | [Pack management](../../guides/pack-management.md) |
| Create and inspect Instances | instance create/list/show | [First Instance](../../introduction.md) |
| Supply or export configuration | input | [Inputs](../../guides/configure-inputs.md) |
| Execute and inspect operations | action, invoke, run | [Actions and diagnostics](../../guides/run-and-diagnose.md) |
| Inspect live data declarations | service-storage, resource | [Resources](../../guides/service-resources.md) |
| Back up or transition | snapshot, instance migrate | [Snapshots and Migration](./snapshots-migrations-and-recovery.md) |
| Retire and reclaim | instance delete/abandon, storage gc | [Retirement](../../guides/retirement.md) |

## References and pagination

Revision references use `<package>:<revision>`. Either component may be a user
name or an ID, so `app:stable`, `<package-id>:stable`, `app:<revision-id>` and
`<package-id>:<revision-id>` are supported. Names are local, case-sensitive and
1 to 31 ASCII characters. Unique lowercase hexadecimal ID prefixes require at
least eight digits. Full IDs remain usable even when names or short prefixes are
ambiguous. Revision ID digits omit `sha256:` in this reference syntax.
State-version tokens and authored capability identifiers still require exact values.

Where supported, lists use `--limit`, `--after`, and `--no-trunc`. Follow the
reported continuation rather than assuming the first page is complete. Pages
are separate read snapshots, not a frozen export of the whole catalog.
Revision lists are newest-installed first, with unknown times last; their returned
cursor includes its sort position and is not an editable Revision reference.

## Output modes

- Human output is for reading. Do not parse its tables as a stable API.
- `--format json` emits a complete response.
- `--format jsonl` streams events followed by a terminal result.

Check the response contract, result, and process exit status. Hook byte chunks
in machine modes use Base64. Cancellation on a closed JSONL receiver is opt-in
with `--cancel-on-output-close`; a disconnected consumer must not assume the
managed operation stopped by default.

## Plans, timeouts, and errors

Only use `--plan` on commands that support it. Plans do not reserve admission.
Snapshot and Migration timeout options differ from invoke's Action timeout;
use the [Snapshot options](../reference/snapshot-details.md) and
[Migration options](../reference/migration-details.md) rather than copying flags.
Syntax errors and operation failures have different exit conventions. A partially
completed operation needs inspection even when its final exit is nonzero.

More detail: [User reference](../reference/index.md).

<details>
<summary>Maintainer sources (optional)</summary>

Contracts: [commands](../../spec/interfaces/commands.md),
[machine output](../../spec/interfaces/machine-output.md), and
[selectors](../../spec/interfaces/id-selectors.md).

</details>
