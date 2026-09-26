---
title: Execution Diagnostics
---

# Execution diagnostics

**Status: Implemented and verified normative behavior; integration is tracked separately.**

<!-- spec-navigation:start -->
Read [Run behavior](./actions-plans-and-runs.md), [Hook Protocol](../contracts/hook-protocol.md)
and [V11 persistence](../persistence/persistence-baseline.md). Delivery is recorded
in the [implementation status](../../development/execution-diagnostics-observability-status.md).
<!-- spec-navigation:end -->

### PR-REQ-0351 - Bounded, attributed evidence for post-run debugging

Accepted Runs MUST support debugging without replaying side-effectful operations.
New Hook diagnostic, completion and protocol-error explanations MUST be retained
by default and attributed to their Run, exact execution context and Hook invocation.
Run-local sequence determines order; receipt time is not the Hook's internal
occurrence time. Primary and secondary Pactrun failures, outcome, result references
and the current Instance recovery guard MUST remain distinct.

Only protocol/state-machine-accepted Hook messages qualify for the historical
journal. Diagnostic error severity MUST NOT become a second Run failure rule.
Invalid raw frames, terminal streams and Input/Secret payloads MUST NOT enter
that journal. CLI machine delivery may temporarily spool terminal streams and
accepted live diagnostics under PR-REQ-0366/0367, independently of journal budgets.
Hook authors remain
responsible for sensitive text; escaping is not universal redaction or encryption.
Default retention can put Hook-authored sensitive text into the store and backups.

General diagnostics MUST keep an initial 1 MiB/1,024-event prefix and a rolling
3 MiB/3,072-event suffix. Once sealed, the prefix does not resume accepting events.
Completion/protocol-error explanations MUST have a separate newest 1 MiB/256-event
budget. Byte budgets count code/message UTF-8 bytes; count limits also apply.
Overlong messages keep UTF-8-safe first and last 32 KiB portions, with a recorded
split and explicit omitted-middle indication. Hook codes are never truncated.
Omitted sequence intervals and unknown tails MUST NOT be disguised as continuity.

All Hook execution command families MUST accept --no-retain-hook-text to suppress
persistent Hook code/message text without disabling live display. Preview MUST NOT
establish a collection. Run show MUST expose new retained explanations without
another authorization flag; human Run list remains structural, while machine
Run list includes available inspection data under PR-REQ-0365. Legacy free text
MUST NOT be retroactively treated as newly collected evidence.

Noninteractive live explanations use stderr. Interactive execution MUST defer
presentation until the terminal is returned. Presentation and retention completeness
are independent. A blocked/broken renderer MUST NOT hold supervision or ownership.

The producer MUST use bounded memory and no persistence/output I/O. Collection
MUST schedule incremental commits when evidence is pending, at most 250 ms apart
under normal scheduling, with priority scheduling for terminal explanations. This is
not a durability deadline under storage contention/failure. A bounded contention
retry is permitted; permanent failure MUST stop persistence without changing Run
outcome or causing replay. Normal shutdown MUST wait only a bounded interval.
Only a fully drained collection may be marked closed; capacity omissions remain
visible even when closed. An unclosed collection has an unknown tail.

Rationale: non-retention and first-events-only budgets could remove the evidence
needed after failure. Separate prefix, suffix and terminal budgets retain startup
context and recent failure explanations without unbounded storage. Evidence does
not become a second recovery authority.

**Verification: PR-TEST-0518, PR-TEST-0519, PR-TEST-0520, PR-TEST-0526, PR-TEST-0527, PR-TEST-0528, PR-TEST-0489, PR-TEST-0495, PR-TEST-0113.**
