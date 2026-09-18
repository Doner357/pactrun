---
title: Shell Adapter and Loader Baseline
---

# Shell Adapter and Loader baseline

The operator approved this scope and continuous S0-S6 execution on 2026-09-18.
Approval is not implementation, format Freeze, release or permission to commit,
merge, push, deploy, change host policy, or weaken acceptance.

## Decisions

Pactrun ships one execution-scoped loader in its executable, not an interpreter,
daemon, workflow engine or alternate semantic Hook protocol. POSIX sh and Bash
and Windows PowerShell 7 and Windows PowerShell 5.1 are the acceptance matrix.
Ordinary scripts need no initialization, helper import or termination boilerplate.

Core V3 and YAML V3 explicitly select a shell_loader launch with a shell kind,
host executable command and owned script Content ID. Existing Hook arguments,
protocol selection, terminal contract and service declarations retain their
meanings. These authoring choices bear identity; the running Pactrun path does
not. Frozen V1/V2 formats and Hook wire protocols remain unchanged. No implicit
reinterpretation or fallback of direct/interpreter Hooks is permitted.

The loader establishes the single Core connection and Session before starting
the script. Repeated `pactrun hook` helpers use a separate execution-scoped,
owner-private socket or named pipe, never terminal streams or additional Core
connections. Script-facing discovery is injected, not trusted from ambient
configuration. Interpreter resolution retains exact admitted-path semantics.
Launch must preserve argument data without shell-code interpolation. Do not
change error preferences, execution policy, profiles or host PATH.

Helpers expose the complete granted Session and convenient typed access to
parameters, Inputs, Workspace, outputs, Snapshot content and service resources.
Structured data has a JSON-file interface and opaque bytes have a file interface.
Do not require Secrets or arbitrary bytes to pass through shell strings, text
pipelines or environment variables. No automatic persistent Session dump is added.
Outputs and Capture descriptors are explicitly registered; no Workspace scan or
declarative filename mapping is added. Helpers expose diagnostics, acknowledged
risk transitions and V2 target-ready without exposing Session lifecycle mechanics.

Normal zero exit requests ordinary success, subject to the existing request,
output and risk conditions. Nonzero exit requests failure. Handled intermediate
command failures do not independently determine script failure. Cancellation,
abnormal process loss and uncertain protocol/helper results cannot become success.
Risk entry waits for the durable acknowledgment. Registrations and receipts are
not durable Run success. Transform Sessions require explicit target-ready,
receipt and normal zero process-tree termination; never ordinary success or
automatic risk resolution. No uncertain request is automatically retried.

## Execution and evidence

| Slice | Completion evidence |
| --- | --- |
| S0 | Owning contracts, rationale and capability/verification inventory |
| S1 | V3 projection, strict codecs, identity vectors, content closure and admission |
| S2 | Plain-script real-process journeys across the four shell combinations |
| S3 | Complete helpers, private IPC, byte fidelity, isolation and loss handling |
| S4 | Action, Migration, Capture, Restore, Cleanup and V2 transform parity |
| S5 | Fault, cancellation, terminal, argument and process-tree acceptance |
| S6 | Exact-source remote full CI, Windows checks and documentation closeout |

Execute these slices continuously. Pause only for a genuine semantic decision,
missing authorization/environment or an acceptance blocker needing operator help.
Ordinary implementation choices and repairable test failures are not pause gates.
Test the real contract, including forbidden behavior, before claiming completion.

No persistence schema change is preapproved. Audit admission and old-writer
compatibility before activation; if a new schema boundary is necessary, explain
the concrete persisted-meaning/compatibility consequences before changing it.

See the [implementation record](../shell-adapter-loader-status.md) for actual
availability and evidence. This baseline does not supersede owning normative
requirements.
