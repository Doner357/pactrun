---
title: Actions, plans, and Runs
---

# Actions, plans, and Runs

An Action is a Pack-defined operation. Inspect its declared parameters, access,
and description before invoking it. Author descriptions are informational;
Pactrun still checks the operation's actual contract and current state.

## Plans

Use `--plan` where supported to inspect an operation without starting it. A plan
does not reserve resources or guarantee that a later invocation will pass.
State, available programs, or resource conditions can change between commands.

## Runs

Execution produces a Run with an identity and durable outcome. Live Hook output
is progress information; the command and Run result determine completion.
Use `run show` to inspect failure diagnostics and retained output availability.

Run inspection separates Hook-authored messages from Pactrun's safe helper
diagnostics. A handled helper error can be recorded in a successful Run. Use the
final outcome to determine how execution ended.

`--no-retain-hook-text` omits saved Hook text while keeping live output and
Pactrun's helper classifications available. Missing Input failures retain the
IDs observed at admission, even after the Instance is configured.

## Concurrent changes

An operation can fail because another command changed the Instance or acquired
conflicting access. Reinspect the Instance and plan again. Do not interpret a
retry as automatic rollback of effects already performed by a Hook.

Use [Run and diagnose an Action](../../guides/run-and-diagnose.md) for commands.

More detail: [User reference](../reference/index.md).

<details>
<summary>Maintainer sources (optional)</summary>

Contracts: [execution](../../spec/operations/actions-plans-runs.md) and
[diagnostics](../../spec/operations/diagnostics.md).

</details>
