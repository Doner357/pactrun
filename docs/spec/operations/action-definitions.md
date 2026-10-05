---
title: Action Definitions
---

# Action Definitions

## Actions

### PR-REQ-0125 - Action semantics

An Action MUST represent a Package-defined user operation. Pactrun MUST treat
its stable identity, parameter schema, execution requirements, I/O contract,
access requirement, implementation, and declared managed outputs as semantics;
it MUST NOT infer service behavior from the Action's name.

**Verification: PR-TEST-0069, PR-TEST-0148, PR-TEST-0168, PR-TEST-0592.**

### PR-REQ-0126 - Action is not Hook

An Action MUST remain the user operation and a Hook MUST remain one possible
implementation mechanism. Snapshot, Migration, and Cleanup MUST NOT be authored
as Actions even when they ultimately launch Hooks.

**Verification: PR-TEST-0069, PR-TEST-0585, PR-TEST-0592.**
