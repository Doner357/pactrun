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

Hook text can be omitted with `--no-retain-hook-text`; live output stays enabled.
Absence of retained text therefore does not establish that the Hook was silent.

## Concurrent changes

An operation can fail because another command changed the Instance or acquired
conflicting access. Reinspect the Instance and plan again. Do not interpret a
retry as automatic rollback of effects already performed by a Hook.

Use [Run and diagnose an Action](../../guides/run-and-diagnose.md) for commands.

More detail: [User reference](../reference/index.md).

<details>
<summary>Maintainer sources (optional)</summary>

Contracts: [execution](../../spec/behavior/actions-plans-and-runs.md) and
[diagnostics](../../spec/behavior/execution-diagnostics.md).

</details>
