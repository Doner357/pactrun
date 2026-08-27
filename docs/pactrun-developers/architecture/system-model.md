---
title: System Model
---

# System Model

**Status: Normative architecture.**

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

## PR-REQ-0001 - Product boundary

Pactrun MUST remain a local-first, install-managed, command-driven,
single-executable modular monolith. It MUST NOT require a daemon and MUST NOT
act as a scheduler, remote orchestrator, web platform, secret vault, or
replacement for service-specific tools.

**Verification: Pending automated coverage.**

## PR-REQ-0002 - Package taxonomy

`Package` MUST be the core package abstraction, and `Pack` MUST be the only
Package kind in the initial product scope. Pactrun MUST NOT infer Package,
Revision, Instance, Action, Snapshot, Migration, persistence, compiler, or CLI
semantics for a `Stack` concept.

**Verification: Pending automated coverage.**

## PR-REQ-0003 - Source-to-Instance boundary

Pactrun MUST transform a Package Source into a Revision Candidate, validate and
semantically normalize it, project identity-bearing semantics, canonicalize and
materialize owned runtime content, and commit an immutable Installed Revision.
Only an Installed Revision may serve as the execution definition for an
Instance.

**Verification: Pending automated coverage.**

## PR-REQ-0004 - Source independence

A Package Source MUST remain mutable authoring input and MUST NOT be treated as
an execution identity. An Instance execution MUST NOT depend on the original
Source remaining available after Revision installation.

**Verification: Pending automated coverage.**

## PR-REQ-0005 - Installation and Instance creation

Installing Package content MUST create or associate an immutable Revision and
MUST NOT implicitly create an Instance.

**Verification: Pending automated coverage.**

## PR-REQ-0006 - Distinct operation concepts

Action, Snapshot Capture, Snapshot Restore, Migration, and Cleanup MUST remain
distinct domain concepts. Sharing compiler or execution primitives MUST NOT
collapse them into a catch-all authoring-level `Operation` model.

**Verification: Pending automated coverage.**

## PR-REQ-0007 - Managed execution pipeline

Every managed execution MUST pass through Resolution, Workflow Compilation, an
immutable Execution Plan, acceptance as an execution attempt with durable Run
creation, Executor Admission, and execution. The Run MUST exist before an
admission failure is published. Management operations that do not execute a
workflow MUST NOT be forced through that pipeline.

**Verification: Pending automated coverage.**

## PR-REQ-0008 - Compiler and Executor ownership

The Workflow Compiler MUST own interpretation of resolved domain semantics and
production of a typed plan. The Executor MUST execute that plan without
reinterpreting authoring files, CLI syntax, Action names, or workflow policy.

**Verification: Pending automated coverage.**

## PR-REQ-0009 - Modular-monolith dependency direction

The architecture MUST maintain explicit ownership and a central composition
root. Core domain code MUST NOT depend on CLI, persistence-adapter, or
third-party adapter types. Implementations MUST avoid global mutable state,
implicit registration, service locators, untyped dynamic maps, and string-key
plumbing for domain contracts.

**Verification: Pending automated coverage.**

## PR-REQ-0010 - Policy and mechanism separation

Pactrun-specific semantics MUST be owned by Pactrun domain code. Stable
third-party libraries MAY supply commodity mechanisms, but library behavior and
types MUST NOT become the source of Pactrun policy or identity semantics.

**Verification: Pending automated coverage.**

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
decision documented in [Implementation Guidance](../engineering/implementation-guidance.md).
