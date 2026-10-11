---
title: Run and diagnose an Action
---

# Run and diagnose an Action

Use an installed Pack whose Hooks you trust. This example assumes the Pack
provides an Action named `inspect`; discover actual names before substituting one.
You need a [live Instance](./use-pack.md) and any
[required configuration](./configure-inputs.md). If `action list` is empty, stop
this procedure and choose a capability the Pack actually declares.

## Discover and plan

```text
pactrun action list demo
pactrun action show demo inspect
pactrun invoke demo inspect --plan
```

Review parameters, required Inputs, service access, and launch requirements.
Fix missing configuration or unavailable programs before execution. A successful
plan does not reserve admission for a later command.

## Execute

```text
pactrun invoke demo inspect
```

Use `--param name=value` for ordinary typed parameters, and supported file/stdin
parameter options for sensitive values. Confirm exact options with
[Invoke command reference](../pactrun-users/operations/invoke-reference.md).
An interactive Hook may need stdin itself; do not combine conflicting stdin uses.

## Inspect the outcome

```text
pactrun run list demo
pactrun run show <run-id>
```

Read the final outcome, failure cause, and diagnostics together. `run show`
distinguishes Hook-authored messages from Pactrun's own helper diagnostics. A
helper diagnostic identifies the command, failed stage, and reason; the Run can
still succeed when the script handles that error. The final outcome records
whether the operation succeeded.

`--no-retain-hook-text` leaves live output enabled and omits saved Hook text.
Pactrun's safe helper classifications remain available. Sequence gaps mark
omitted events; an incomplete collection can have an unknown tail.

## Diagnose a refusal or failure

| Situation | Next action |
| --- | --- |
| Missing required Input | Use the suggested `input set` commands, then preview again |
| Missing or invalid parameter | Inspect `action show` and supply the declared parameter |
| Conflict or changed state | Reinspect the Instance; plan again before retrying |
| Host executable unavailable | Check the Pack's interpreter/program prerequisites |
| Hook failure or cancellation | Inspect the Run; check service state before another write |
| Manual recovery required | Follow the Pack's recovery procedure before acknowledging recovery |
| Missing retained output | Check retention policy and whether output was registered |

An invocation refused for missing Inputs prints a command for each missing value,
for example `pactrun input set demo config --file <path>`. Replace `<path>` with
your configuration file. After binding, `run show` still records the Inputs that
were missing in that earlier attempt; inspect the Instance for current readiness.

`run reconcile` addresses retained execution bookkeeping; it does not repair
an external service. `instance resolve-manual-recovery` is an operator assertion
after repair, not a repair command. Do not use recovery override merely to silence
a warning. See [recovery rules](./recovery.md).

## Automation

Use `--format json` for a complete structured response or `--format jsonl` for
streamed events. Check the terminal result and exit status. Human tables are not
a machine API. See [machine output](../pactrun-users/reference/machine-output.md).
