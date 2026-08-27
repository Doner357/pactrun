---
title: Inputs, Secrets, and Readiness
---

# Inputs, Secrets, and Readiness

**Status: Normative product behavior specification except where linked to an
owning requirement.**

## Three input scopes

Pactrun keeps three concepts separate:

| Concept | Lifetime | Scope | Example |
| --- | --- | --- | --- |
| Instance Input or Secret | Persistent | Instance | Domain, password, configuration file |
| Invocation Parameter | One invocation | Action or Snapshot operation | Tail count, follow mode, one-time credential |
| Managed Input | One managed workflow | Workflow | Snapshot payload or Migration content |

They are not interchangeable generic maps.

## PR-REQ-0090 - Ordinary execution context

An ordinary Action or Snapshot Capture MUST receive the active Revision's
current active Input and Secret bindings as one Instance context. Retained
bindings MUST NOT appear in ordinary execution context. Migration and Cleanup
MAY receive typed active and retained contexts defined by their own contracts.

**Verification: Pending automated coverage.**

## PR-REQ-0091 - Readiness admission

Ordinary Actions and Snapshot Capture MUST require
`RequiredInputsSatisfied == true` before launching a Hook. Missing bindings MUST
produce actionable diagnostics. Migration and Cleanup MUST instead use their
operation-specific requirements, and Restore MUST validate the staged Snapshot
state rather than target pre-restore completeness.

**Verification: Pending automated coverage.**

## Managing active and retained bindings

The normative mutation rules are
[PR-REQ-0033](../architecture/identity-and-state.md#pr-req-0033---binding-mutation-invariants).
In user terms:

- a required Input can be absent initially, but cannot be ordinarily deleted
  after it is bound under the active Revision;
- optional active Inputs can be set, replaced, or removed;
- retained Inputs can be inspected, exported, or deleted, but not directly set
  or replaced;
- Input mutations are management operations and do not create Runs.

## PR-REQ-0092 - Explicit Secret export

Export of a Secret Input MUST require an explicit sensitive-data operation and
authorization. A generic confirmation option MUST NOT implicitly authorize
Secret disclosure, Snapshot Secret export, or Secret declassification.

**Verification: Pending automated coverage.**

## PR-REQ-0093 - Secret deletion disclaimer

Deleting or discarding a Secret MUST remove its managed reference according to
the operation contract, but Pactrun MUST NOT claim physical secure erasure of
underlying storage.

**Verification: Pending automated coverage.**

## UTF-8 and opaque Input data

Input payloads are opaque bytes and may contain empty, binary, or UTF-8 data.
Pactrun preserves them without interpreting their language or file extension.
Textual Package metadata, parameters, and output interfaces that specify UTF-8
must accept full Unicode, including symbols and emoji.
