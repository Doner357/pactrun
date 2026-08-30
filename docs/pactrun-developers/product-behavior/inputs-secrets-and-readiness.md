---
title: Inputs, Secrets, and Readiness
---

# Inputs, Secrets, and Readiness

**Status: Normative product behavior specification except where linked to an
owning requirement.**

## Persistent and execution data scopes

Pactrun keeps ownership and lifetime explicit:

| Concept | Lifetime | Scope | Authority and example |
| --- | --- | --- | --- |
| Revision Runtime Content | Immutable | Revision | Pactrun-owned installed executable and support content |
| Managed Input Binding | Persistent | Instance | Pactrun-authoritative detached domain, password, or configuration payload |
| `ServiceStorage` / ServiceStorage-backed Managed Service Resource | Persistent | Instance | Pactrun-provided storage lifetime with service-authoritative live contents |
| Workspace | One execution | Execution | Hook-writable temporary scratch |
| Run Artifact | Retained by policy | Run | Pactrun-managed committed Action output |
| Snapshot | Durable immutable object | Snapshot | Validated recovery state, not a live-state mirror |

The first two persistent Instance-data models are not interchangeable. A
Managed Input Binding is an opaque detached value whose presence, replacement,
retention, and Secret protection are managed by Pactrun. A ServiceStorage-backed
Managed Service Resource is live state that the service may create or mutate
directly. Pactrun may identify and expose its contract without owning,
versioning, linearizing, content-addressing, or automatically Snapshotting the
live bytes.

Visibility does not decide ownership: a Pactrun-visible persistent file is not
therefore required to be an Input, and an Input MUST NOT be kept synchronized
implicitly with a service-owned file.

## Three value-delivery scopes

Pactrun also keeps three value-delivery concepts separate:

| Concept | Lifetime | Scope | Example |
| --- | --- | --- | --- |
| Instance Input or Secret | Persistent | Instance | Domain, password, detached configuration payload |
| Invocation Parameter | One invocation | Action or Snapshot operation | Tail count, follow mode, one-time credential |
| Managed Input | One managed workflow | Workflow | Snapshot payload or Migration content |

They are not interchangeable generic maps.

## Informative live-file example

A NetBird Revision may declare a future Managed Service File with semantic
identity `management_config`, associated with storage `netbird_state` and path
`management.json`. The declaration may exist while the file is absent. The
service may later create and continuously use the same file, and a user may
need to inspect or edit that live state. A compatible Revision transition or
container recreation continues to use that one persistent resource without
copying or rematerializing it.

This case is not modeled as `Input -> materialize -> synchronize`. Formal
identity encoding, authoring, authority wire shape, access encoding,
compatibility representation, retention persistence, and durable representation
remain future-version design gates. The example does not classify non-
ServiceStorage-backed resources.

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
