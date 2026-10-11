---
title: Shell Loader
---

# Shell Loader

### PR-REQ-0349 - Session-managed scripts and explicit helpers

The built-in Loader MUST own one persistent canonical Hook connection and Session
lifecycle. It MUST validate the admitted protocol version, receive Session start,
and send readiness before running a script. Scripts MUST NOT need setup imports,
handshake or completion boilerplate. Repeated helpers MUST use separate private
IPC, not reconnect the canonical Hook Protocol endpoint or consume terminal streams.

The host interpreter is not bundled. Sh receives the absolute script path and
ordered args; Bash receives `--noprofile --norc --`, script and args. PowerShell
receives `-NoLogo -NoProfile -File`, script and args. No command-string interpolation,
execution-policy override, error-preference change or shell failure-option change
is permitted. Equivalent unambiguous Windows drive path spelling may be used for
PowerShell's script authorization manager; ambiguous path normalization is refused.
Existing direct/interpreter Hook behavior does not change.

The script receives `PACTRUN_EXECUTABLE` naming the running executable and
`PACTRUN_SHELL_HELPER_ENDPOINT` naming its execution-private IPC. Ambient values
are replaced. The Loader does not pass Core transport discovery to the script.
Discovery is not a durable identity, a permission to acquire another Session, or
an OS sandbox. The trusted-Hook host access contract remains unchanged.

`pactrun hook` supplies these closed commands:

| Command | Result or effect |
| --- | --- |
| `session` | Exact granted Session JSON, not a durable export |
| `parameter ID` | JSON scalar retaining the bound value's type |
| `workspace` | Granted scratch root as text |
| `input ID [--view current\|source\|target] [--role active\|retained] [--copy-to FILE]` | Granted path, or explicit opaque-byte copy |
| `resource HANDLE` | Path of the granted service authority |
| `output ID [--file FILE]` | Staging path, or byte copy into its granted slot; not registration |
| `output-register ID` | Explicitly register an allocated Action or Migration output |
| `candidate` | Capture candidate root |
| `snapshot-content` | Restore content root and exact logical descriptor JSON |
| `diagnostic --file FILE` | Valid canonical diagnostic message from JSON file |
| `protocol-error --file FILE` | Hook protocol-error message; terminates non-successfully |
| `risk enter\|resolve` | Wait for the matching durable risk acknowledgment |
| `capture-register --file FILE` | Register one role/path/candidate_path descriptor |
| `completion --file FILE` | Optional code/message and sticky failure selection, not Session completion |
| `target-ready` | Submit registered target handles and wait for the target-publication receipt |

Accepted diagnostic text is
presented live and retained by default for Run inspection under the
[execution diagnostics policy](../operations/diagnostics.md).
`--no-retain-hook-text` suppresses persistent Hook text without disabling live
display. Interactive presentation waits until the terminal is returned. Hook
authors remain responsible for keeping sensitive values out of diagnostics.

Built-in helper errors report a safe command, stage and reason through
[Pactrun-owned Run diagnostics](../operations/diagnostics.md#pr-req-0378---pactrun-owned-execution-diagnostics).
A handled helper failure remains diagnostic evidence when the script later
succeeds. It is not a Hook-authored diagnostic or a new completion-failure rule.

Read results support `--output FILE`, creating a new file instead of overwriting
an existing file. JSON files are UTF-8 without an added BOM. Text-file output
contains the exact text without an added newline; terminal results append a
newline. JSON-input files use the strict JSON profile and frame-size bound.
Copy interfaces stream opaque bytes outside IPC; scripts need not route bytes
or Secrets through shell strings. No parameter or Secret is automatically
expanded into environment variables or retained logs. Explicit Session output
can contain sensitive data and remains the trusted script's responsibility.

Helpers return zero only for successful local completion or the required peer
acknowledgment. Registration is not durable publication; target receipt is not
Run success. State-changing helpers are serialized. Invalid local requests may
return nonzero without ending the Session. Once a submitted helper's delivery
becomes uncertain, Loader loss or helper loss MUST prevent successful completion;
issued requests MUST NOT be automatically retried. Helpers cannot enlarge grants.
Waiting for private connection capacity remains governed by the execution owner,
not an additional helper execution deadline. A busy connection may be retried
before issuing any request; a missing or invalid endpoint fails without replay.

Normal zero script exit requests success only when all original conditions hold.
Nonzero exit requests failure, and handled intermediate command errors do not
independently imply failure. Diagnostics do not imply completion failure. Explicit
completion failure cannot be changed back to success. Migration failure submits
no target outputs. Capture defaults to an explicitly empty descriptor collection.
There is no automatic Workspace collection or output registration.

The Loader waits for completion_accepted without claiming durable publication.
Cancellation, abnormal loss, malformed protocol, Open-risk success and missing
required outputs cannot become success. Transform Sessions retain acknowledged
Open, explicit target-ready, receipt and normal zero tree-termination ordering;
ordinary success and independent risk resolution are forbidden. No automatic
repair, replay, rollback, risk resolution or inference of service coherence occurs.
Existing Core supervision owns cancellation, terminal channels, child-tree
termination and durable outcome arbitration.

**Verification: PR-TEST-0690.**

**Verification: PR-TEST-0486, PR-TEST-0488, PR-TEST-0489, PR-TEST-0490, PR-TEST-0491,
PR-TEST-0495, PR-TEST-0496, PR-TEST-0497, PR-TEST-0498, PR-TEST-0499,
PR-TEST-0500, PR-TEST-0501, PR-TEST-0502, PR-TEST-0503, PR-TEST-0504,
PR-TEST-0505, PR-TEST-0506, PR-TEST-0507, PR-TEST-0508, PR-TEST-0509,
PR-TEST-0511, PR-TEST-0512, PR-TEST-0513, PR-TEST-0514, PR-TEST-0515, PR-TEST-0516,
PR-TEST-0517, PR-TEST-0616, PR-TEST-0617.**
