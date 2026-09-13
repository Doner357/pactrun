---
title: System Model
---

# System Model

**Status: Normative architecture.**

<!-- spec-navigation:start -->
## Reading map (informative)

Start here for the product boundary, data ownership, and architectural constraints that apply to every operation.

Start with the [specification map](../index.md)
and [shared vocabulary](../glossary.md) if a term is unfamiliar.
Check [implementation status and remaining decisions](../../development/next-milestone.md)
before treating an approved contract as available runtime behavior.
The original status, rules, exceptions, and verification declarations below retain their meaning.
<!-- spec-navigation:end -->

This page defines Pactrun's product boundary, core ownership, and architectural
separation. User-facing and author-facing contracts are defined in their
respective sections.

## Normative product-design constraints

The following are formal architecture constraints even where conformance
requires design review rather than a mechanical test. They do not receive
artificial requirement or test identifiers solely to make subjective criteria
appear executable.

- Pactrun MUST preserve a high ceiling and a low floor: metadata, one Action,
  and one Hook are sufficient for the smallest useful Pack, while advanced
  capabilities MUST NOT increase the minimum authoring burden for simple Packs.
- Operations, diagnostics, Plans, and recovery guidance MUST let Pack users
  predict expected results, identify missing prerequisites, and understand
  available recovery actions.
- Package contracts MUST NOT force authors to understand unnecessary internal
  DAG, lock, transaction, checkpoint, or recovery-bookkeeping mechanisms.
- Common authoring paths, structures, and diagnostics SHOULD remain predictable
  for both humans and LLMs; advanced capabilities SHOULD NOT pollute the
  smallest valid examples.
- Pactrun MUST claim only security properties enforced by its actual authority
  or isolation mechanisms.

### PR-REQ-0001 - Product boundary

Pactrun MUST remain a local-first, install-managed, command-driven,
single-executable modular monolith. It MUST NOT require a daemon and MUST NOT
act as a scheduler, remote orchestrator, web platform, secret vault, or
replacement for service-specific tools.

**Verification: Pending automated coverage.**

### PR-REQ-0002 - Package taxonomy

`Package` MUST be the core package abstraction, and `Pack` MUST be the only
Package kind in the initial product scope. Pactrun MUST NOT infer Package,
Revision, Instance, Action, Snapshot, Migration, persistence, compiler, or CLI
semantics for a `Stack` concept.

**Verification: Pending automated coverage.**

### PR-REQ-0003 - Source-to-Instance boundary

Pactrun MUST transform a Package Source into a Revision Candidate, validate and
semantically normalize it, project identity-bearing semantics, canonicalize and
materialize owned runtime content, and commit an immutable Installed Revision.
Only an Installed Revision may serve as the execution definition for an
Instance.

M2 closes one minimal source frontend and installation path in PR-REQ-0258
through PR-REQ-0261. Its YAML is authoring input and MUST pass through this
Candidate boundary rather than becoming an installed contract directly.

**Verification: Pending automated coverage.**

### PR-REQ-0004 - Source independence

A Package Source MUST remain mutable authoring input and MUST NOT be treated as
an execution identity. An Instance execution MUST NOT depend on the original
Source remaining available after Revision installation.

M2 source acquisition MUST bind identity to the exact staged bytes, normalized
core, and runtime-content closure, not to a source pathname or filesystem
object identity.

**Verification:** PR-TEST-0143 proves the external source-independence
boundary. The exact opened/staged source objects, normalized core, and
runtime-content-closure clauses retain their lower-layer evidence under their
respective acquisition and installation requirements.

### PR-REQ-0005 - Installation and Instance creation

Installing Package content MUST create or associate an immutable Revision and
MUST NOT implicitly create an Instance.

M2 Revision installation and Instance creation are separate typed requests and
separate atomic database publications, as defined by PR-REQ-0261 and
PR-REQ-0265.

**Verification: PR-TEST-0143.**

### PR-REQ-0006 - Distinct operation concepts

Action, Snapshot Capture, Snapshot Restore, Migration, and Cleanup MUST remain
distinct domain concepts. Sharing compiler or execution primitives MUST NOT
collapse them into a catch-all authoring-level `Operation` model.

**Verification: Pending automated coverage.**

### PR-REQ-0007 - Managed execution pipeline

Every managed execution MUST pass through Resolution, Workflow Compilation, an
immutable Execution Plan, acceptance as an execution attempt with durable Run
creation, Executor Admission, and execution. The Run MUST exist before an
admission failure is published. Management operations that do not execute a
workflow MUST NOT be forced through that pipeline.

**Verification: Pending automated coverage.**

### PR-REQ-0008 - Compiler and Executor ownership

The Workflow Compiler MUST own interpretation of resolved domain semantics and
production of a typed plan. The Executor MUST execute that plan without
reinterpreting authoring files, CLI syntax, Action names, or workflow policy.

**Verification: Pending automated coverage.**

### PR-REQ-0009 - Modular-monolith dependency direction

The architecture MUST maintain explicit ownership and a central composition
root. Core domain code MUST NOT depend on CLI, persistence-adapter, or
third-party adapter types. Implementations MUST avoid global mutable state,
implicit registration, service locators, untyped dynamic maps, and string-key
plumbing for domain contracts.

**Verification: Pending automated coverage.**

### PR-REQ-0010 - Policy and mechanism separation

Pactrun-specific semantics MUST be owned by Pactrun domain code. Stable
third-party libraries MAY supply commodity mechanisms, but library behavior and
types MUST NOT become the source of Pactrun policy or identity semantics.

**Verification: Pending automated coverage.**

## Persistent and execution data ownership

Pactrun distinguishes data by ownership, lifetime, and authority. Visibility to
Pactrun or to a user does not by itself make persistent data an Input.

| Concept | Lifetime and scope | Authoritative owner | Role |
| --- | --- | --- | --- |
| Revision Runtime Content | Immutable, Revision-scoped | Pactrun | Installed executable and support content addressed by the exact Revision |
| Managed Input Binding | Persistent, Instance-scoped | Pactrun | Detached opaque binding whose presence, replacement, retention, and protection are Pactrun-managed |
| `ServiceStorage` / ServiceStorage-backed Managed Service Resource | Persistent, Instance-scoped | Pactrun for provided-storage lifetime; service for live contents | Attached live state whose semantic identity and use contract are managed without transferring content authority |
| Workspace | Temporary, execution-scoped | Hook within granted scratch authority | Uncommitted scratch and staging |
| Run Artifact | Run-scoped | Pactrun | Committed output belonging to one Run |
| Snapshot | Durable immutable object | Pactrun | Validated recovery representation, not a live-state mirror |

A Managed Service File is a file-shaped ServiceStorage-backed Managed Service
Resource. The service may create or mutate its authoritative live bytes without
Pactrun observing the mutation time. Pactrun may identify and expose the
resource contractually, but MUST NOT treat those bytes as a Managed Input
Binding, maintain a second authoritative-looking copy through implicit
synchronization, or claim the same linearization guarantees that apply to
Pactrun-owned bindings. This taxonomy does not automatically include Docker
volumes, external databases, remote objects, or other service-owned resources.
The normative requirements are in
[Identity and State](./identity-and-state.md#servicestorage-backed-semantic-closure),
and the cross-specification synthesis is in
[ServiceStorage Semantic Baseline](../../development/design-notes/service-storage-semantic-baseline.md).

## Conceptual layers

```text
CLI or future frontend
        -> Application and Resolution
        -> Domain and Authoring
        -> Workflow Compiler
        -> Execution Plan
        -> Executor
             -> Hook Runtime
             -> Managed Data
             -> Persistence
```

The exact crate, module, trait, and repository layout is an implementation
decision documented in [Implementation Guidance](../../development/implementation-guidance.md).
