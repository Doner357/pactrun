---
title: Failure and Manual Recovery
---

# Failure and Manual Recovery

## Failure and recovery

### PR-REQ-0109 - Failure is not manual recovery

A failed, cancelled, timed-out, or interrupted Run MUST NOT by itself place an
Instance in `ManualRecoveryRequired`. That consequence occurs only when durable
recovery state says an unresolved recovery-risk boundary was open. If execution
is lost during service-owned transformation while risk is open, recovery of the
Pactrun-owned committed boundary does not prove that service state remains
coherent with the source Revision.

**Verification: PR-TEST-0085.**

### PR-REQ-0110 - Recovery options

An Instance in `ManualRecoveryRequired` MUST allow inspection, legal Input
management, explicit one-execution override, `ResolveManualRecovery`,
exact-compatible Restore, and `AbandonManagement` according to their separate
preconditions. Ordinary managed execution remains blocked by default.

**Verification: PR-TEST-0156, PR-TEST-0173, PR-TEST-0259, PR-TEST-0329, PR-TEST-0612.**
