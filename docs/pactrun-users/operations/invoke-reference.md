---
title: Invoke command reference
---

# Invoke command reference

Run a declared Action on a live Instance. Discover its parameter IDs and types
with `action show`. Replace the operands below with names from your installed Pack.

## Syntax

```text
pactrun [--format human|json|jsonl] invoke <instance> <action>
    [--param <parameter-id>=<text>]...
    [--param-file <parameter-id>=<host-path>]...
    [--param-stdin <parameter-id>]
    [--plan]
    [--authorize-recovery-override]
    [--startup-timeout-ms <milliseconds>]
    [--action-timeout-ms <milliseconds>]
    [--termination-grace-ms <milliseconds>]
    [--no-retain-hook-text]
    [--cancel-on-output-close]
```

The global output selection precedes the command. This candidate has no
`invoke … --help` subcommand help; that option is rejected. Use this reference
for invocation details and `pactrun --help` for the command overview.

## Parameters

| Option | Operand and behavior |
| --- | --- |
| `--param` | `id=text`. Repeat for distinct parameters. Ordinary command-line text; avoid sensitive values here. |
| `--param-file` | `id=path`. Repeat for distinct parameters. Reads the complete file as protected UTF-8 parameter text. |
| `--param-stdin` | `id`. Reads all stdin as one protected UTF-8 parameter value. At most one stdin parameter; rejected for interactive Actions, including plans. |

The first `=` separates the parameter ID from its value or path. Quote the
complete assignment when your shell would otherwise split it. Duplicate IDs,
unknown parameters, and conflicting stdin sources are refused. Missing parameters
use authored defaults when present; otherwise the operation is incomplete.

Pactrun does not trim whitespace, remove a BOM, expand shell syntax, or normalize
Unicode. File/stdin input is parameter text, not a JSON wrapper. Integer text uses
whole decimal notation; Boolean text is exactly `true` or `false`. A newline in
an integer or Boolean file makes it invalid. Strings retain their exact text.
Protect source files and avoid commands that print sensitive values to history.

### Exact text values

These are invocation-text rules, including `--param-file` and `--param-stdin`.
They differ from YAML numeric-default spellings used by Pack authors.

| Declared type | Accepted text | Refused examples |
| --- | --- | --- |
| `integer` | Canonical signed whole decimal text, within −9,007,199,254,740,991 through +9,007,199,254,740,991 | `+1`, `01`, `1.0`, `1e0`, out-of-range values, surrounding whitespace |
| `float` | Strict decimal JSON-number text, converting to finite binary64 | `NaN`, `Infinity`, `+1`, `01`, `.5`, `1.`, overflow such as `1e400`, surrounding whitespace |
| `boolean` | Exactly `true` or `false` | `True`, `FALSE`, `0`, trailing newline |
| `string` | Exact Unicode text, including empty or number-looking strings | Invalid UTF-8/scalar sequences; no implicit trimming or conversion is performed |

Integer grammar, matching the whole value:

```regex
-?(0|[1-9][0-9]*)
```

Float grammar, matching the whole value:

```regex
-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?
```

An integer token is also valid
for a Float parameter. Negative zero normalizes to zero for numeric parameters;
float underflow such as `1e-400` is accepted as zero. A newline also invalidates a
float value. An empty string can be supplied as `--param message=` when `message`
is a declared string parameter. Use the exact parameter names from `action show`.

## Planning and execution policy

| Option | Default and effect |
| --- | --- |
| `--plan` | Off. Compile a read-only preview; create no Run and launch no Hook. Does not reserve later admission. |
| `--authorize-recovery-override` | Off. Bypass only the operation's recovery trust guard. A preview projects this authorization without changing stored state. |
| `--startup-timeout-ms` | Unlimited when omitted. Bound startup using non-negative decimal milliseconds. |
| `--action-timeout-ms` | Unlimited when omitted. Bound Action execution. Snapshot/Migration commands use `--execution-timeout-ms` instead. |
| `--termination-grace-ms` | 5000 milliseconds when omitted. Time allowed for termination before escalation. |
| `--no-retain-hook-text` | Off. Suppress saved Hook text; live diagnostic display remains enabled. |
| `--cancel-on-output-close` | Off. Request cancellation if a JSONL execution consumer closes stdout. Closing the receiver alone does not stop managed execution by default. |

Zero timeout means immediate expiry; zero grace means no grace period. Negative,
non-decimal, or unsupported deadline values are refused. A timeout or cancellation
can leave external effects that require inspection; it does not promise rollback.

## Examples

These examples assume an Action named `inspect` with a string parameter named
`message`. The [author guide](../../package-authors/fundamentals/actions-inputs-and-parameters.md)
shows that declaration. The parameter-free starter Pack needs that declaration
installed in a new Revision before these parameter examples apply.

```text
pactrun invoke demo inspect --param message=hello --plan
pactrun invoke demo inspect --param-file message=./message.txt --action-timeout-ms 10000
pactrun --format json invoke demo inspect --param message=hello
```

For a noninteractive Action, pass one file through stdin without echoing its value.
In a POSIX shell:

```sh
pactrun invoke demo inspect --param-stdin message < ./message.txt
```

On PowerShell, prefer `--param-file` when preserving file bytes; text pipelines
can change encoding or newlines. Inspect the terminal result and exit status.
Success is 0, syntax/option errors are 2, and operation failure/cancellation is 1.
A successful plan is a preview only. Executed invocation returns success after
durable successful publication. After a failure, use `run show` and inspect the
Instance before retrying.

More detail: [User reference](../reference/index.md).

<details>
<summary>Maintainer sources (optional)</summary>

Owners: [command spelling](../../spec/interfaces/commands.md#pr-req-0284---m3-human-action-plan-run-and-recovery-spelling),
[parameter sources and deadlines](../../spec/operations/actions-plans-runs.md#pr-req-0288---invocation-parameter-sources-and-policy-values),
[typed text](../../spec/operations/parameters.md#pr-req-0273---primitive-invocation-text-lexical-profile),
and [machine output](../../spec/interfaces/machine-output.md).

</details>
