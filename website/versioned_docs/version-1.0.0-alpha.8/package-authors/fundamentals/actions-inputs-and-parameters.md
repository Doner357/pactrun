---
title: Declare Actions, Inputs, and parameters
---

# Declare Actions, Inputs, and parameters

For all types, allowed values, omission defaults, and constraints, use
[Pack fields and values](../reference/pack-fields.md).

## Choose the Input contract

The examples inherited from the first Pack tutorial use Source `1.0-alpha.2`.
That format keeps global `required` declarations and its existing Hook behavior.
It remains supported; updating Pactrun does not rewrite your Pack.

Choose Source `1.0-alpha.3` when operations need different Inputs. Keep Input
identities and protection global, omit global `required`, and give each operation
an `inputs` object with `required` and `optional` lists. An environment precheck
can use empty lists and run before service configuration is supplied. Its Hook
receives no managed Input files unless they are declared in those lists.

Migration distinguishes source and target lists; Cleanup also supports retained
references. See [operation-scoped fields](../reference/operation-format.md) and
[Input read scope](../reference/operation-inputs.md) for exact syntax and defaults.
Source `1.0-alpha.3` uses Hook Protocol `1.0-alpha.2`; direct Hooks must implement
that selected protocol, while the built-in Shell loader supplies it for scripts.

Parameters are now available on Migration and Cleanup as well as Actions and
Snapshot operations. All may declare ordinary reports. A report is not a
Snapshot payload or a Migration target Input: see
[common capabilities](../reference/operation-capabilities.md). Restore still
requires the Snapshot's exact producer Revision; it does not perform Migration.

## Choose the data lifetime

Before editing, complete the [first Pack tutorial](./authoring-model.md) so you
have a working manifest and Hook to extend. Keep that Package ID for this lineage.

Use Inputs for Instance configuration and parameters for one invocation. Use
ServiceStorage for service-owned live data. Keep execution scratch in Workspace.
Do not use runtime content or metadata to smuggle mutable configuration.

## Declare an Input

Place this property directly inside `revision` when your Pack needs configuration:

```yaml
inputs:
  - id: config
    required: true
    protection: normal
```

A required Input can make a new Instance incomplete until configured. Secret
protection must match the declared value's handling; it does not encrypt the host.
Provide operator instructions for acquiring and rotating sensitive values.

## Declare an Action

The [author tutorial](./authoring-model.md) contains an executable observe Action.
Declare access according to the operation's contract, including required Inputs
and any service authority. Descriptions help discovery but do not grant permissions.

Replace the chosen Action's `parameters` property with this sequence:

```yaml
parameters:
  - id: message
    type: string
    sensitive: false
    default: hello
```

Declare the actual type and sensitivity. A sensitive parameter must not be
printed in diagnostics. Defaults participate in the authored contract; changing
them creates different Revision content.

Use `pactrun hook parameter message` inside a Shell Loader script to obtain the
bound JSON scalar. Parse it as JSON rather than evaluating it as shell code.
The operator supplies ordinary parameters with `--param message=value`.

## Validate the user-facing contract

Install the Pack in isolated storage. Inspect `action show`, then test `--plan`,
success, missing required values, invalid typed values, and Hook failure. Confirm
that sensitive content does not appear in output or diagnostics. Use declared
output slots for retained artifacts and register them explicitly.

Editing the source does not update a live Instance. Reinstall the changed Pack,
copy its new reference from `revision list`, and create a fresh test Instance or use a declared
Migration. Do not expect the earlier parameter-free `hook-demo` to gain fields.

If you applied both examples above to the starter Pack, create `config.txt` with
synthetic bytes, such as `demo`, using a text editor. Then run:

```text
pactrun pack install ./pack
pactrun instance create field-demo --revision <new-reference> --input-file config=./config.txt
pactrun action show field-demo inspect
pactrun invoke field-demo inspect --param message=hello --plan
```

Use an unused name and copy the full install-output reference as `<new-reference>`.
Expect `config` to be bound, `message` to appear in the Action definition, and a
valid plan. The starter script still prints its original greeting unless you
change it to read `message`. Inspect failures before retrying.

**Done:** your declarations are installed and checked against a matching Instance.
Retire `field-demo` after reviewing its deletion plan. Continue with
[runtime files](./recipes-and-runtime-content.md).

More detail: [Pack fields and values](../reference/pack-fields.md).

<details>
<summary>Maintainer sources (optional)</summary>

Contracts: [Action definitions](../../spec/operations/action-definitions.md),
[invocation parameters](../../spec/operations/parameters.md),
[Pack source](../../spec/packages/source-format.md), and
[Hook protocol](../../spec/interfaces/hook-protocol.md).

</details>
