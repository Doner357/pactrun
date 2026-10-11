---
title: System Model
---

# System Model

<!-- spec-navigation:start -->
Pactrun installs Pack definitions as immutable Revisions and uses those
Revisions to manage long-lived Instances. Packs declare operations; trusted
Hooks perform the service-specific work.

## Core objects {#reading-map-informative}

| Object | Meaning |
| --- | --- |
| Package | A stable product lineage. Pack is the supported Package kind. |
| Source | Mutable authoring files used to produce a Revision. |
| Revision | An immutable operational definition and its owned runtime content. |
| Instance | A long-lived managed object with its own identity, one active Revision, and persistent data. |

Installing a Revision and creating an Instance are separate operations. Editing
or removing the Source does not change an installed Revision or the definition
used by an existing Instance. See [Packages, Revisions, and Instances](./objects.md)
for resolution, installation, and inspection rules.
<!-- spec-navigation:end -->

## Product boundary

### PR-REQ-0001 - Product boundary

Pactrun MUST remain a local-first, install-managed, command-driven,
single-executable modular monolith. It MUST NOT require a daemon and MUST NOT
act as a scheduler, remote orchestrator, web platform, secret vault, or
replacement for service-specific tools.

**Verification: PR-TEST-0142, PR-TEST-0143, PR-TEST-0606.**

### PR-REQ-0002 - Package taxonomy

`Package` MUST be the core package abstraction, and `Pack` MUST be the only
supported Package kind. Pactrun MUST NOT infer Package, Revision, Instance,
Action, Snapshot, Migration, persistence, compiler, or CLI semantics for a
`Stack` concept.

**Verification: PR-TEST-0068, PR-TEST-0592.**

## From Source to Instance

```text
Mutable Package Source
        -> Revision Candidate
        -> validation and semantic normalization
        -> identity projection and runtime-content materialization
        -> immutable Installed Revision

Installed Revision
        -> separate Instance creation
        -> Instance using that Revision
```

The installation boundary makes execution independent of an author's working
directory. The Source describes what to install; the Installed Revision is the
definition used for execution.

### PR-REQ-0003 - Source-to-Instance boundary

Pactrun MUST transform a Package Source into a Revision Candidate, validate and
semantically normalize it, project identity-bearing semantics, canonicalize and
materialize owned runtime content, and commit an immutable Installed Revision.
Only an Installed Revision may serve as the execution definition for an
Instance.

The [Pack source contract](../packages/source-format.md) defines the YAML frontend
and its Candidate projection. YAML is authoring input and MUST pass through the
Candidate boundary rather than becoming an installed contract directly.

**Verification: PR-TEST-0072, PR-TEST-0143, PR-TEST-0607.**

### PR-REQ-0004 - Source independence

A Package Source MUST remain mutable authoring input and MUST NOT be treated as
an execution identity. An Instance execution MUST NOT depend on the original
Source remaining available after Revision installation.

Source acquisition MUST bind identity to the exact staged bytes, normalized
core, and runtime-content closure, not to a source pathname or filesystem
object identity. The [source acquisition rules](../packages/source-format.md#pr-req-0259---sourcerelativepathv1-and-safe-source-acquisition)
define how opened source objects and staged bytes remain consistent.

**Verification: PR-TEST-0143.**

### PR-REQ-0005 - Installation and Instance creation

Installing Package content MUST create or associate an immutable Revision and
MUST NOT implicitly create an Instance.

Revision installation and Instance creation MUST use separate typed requests
and separate atomic database publications. Their respective commit boundaries
are defined by [Revision installation](./objects.md#pr-req-0261---revision-installation-publication)
and [Instance creation](../instances/inputs-secrets.md#pr-req-0265---instance-creation-and-transient-payload-staging).

**Verification: PR-TEST-0143.**

## Managed execution

### PR-REQ-0006 - Distinct operation concepts

Action, Snapshot Capture, Snapshot Restore, Migration, and Cleanup MUST remain
distinct domain concepts. Sharing compiler or execution primitives MUST NOT
collapse them into a catch-all authoring-level `Operation` model.

**Verification: PR-TEST-0069, PR-TEST-0585, PR-TEST-0592.**

### PR-REQ-0007 - Managed execution pipeline

Every managed execution MUST pass through these stages, in order:

1. Resolution.
2. Workflow Compilation.
3. An immutable Execution Plan.
4. Acceptance as an execution attempt, with durable Run creation.
5. Executor Admission.
6. Execution.

The Run MUST exist before an admission failure is published. Management
operations that do not execute a workflow MUST NOT be forced through this
pipeline.

**Verification: PR-TEST-0089, PR-TEST-0225, PR-TEST-0233, PR-TEST-0302, PR-TEST-0397, PR-TEST-0558, PR-TEST-0607, PR-TEST-0612, PR-TEST-0615.**

### PR-REQ-0008 - Compiler and Executor ownership

The Workflow Compiler MUST own interpretation of resolved domain semantics and
production of a typed plan. The Executor MUST execute that plan without
reinterpreting authoring files, CLI syntax, Action names, or workflow policy.

**Verification: PR-TEST-0090, PR-TEST-0115, PR-TEST-0595, PR-TEST-0610, PR-TEST-0611, PR-TEST-0614.**

## Persistent and execution data ownership

Ownership determines how data can change. Pactrun-owned bindings, live service
data, and temporary execution files have different lifetimes and guarantees.
Visibility to Pactrun or to a user does not by itself make data an Input.

| Data | Lifetime and scope | Authority and purpose |
| --- | --- | --- |
| Revision Runtime Content | Immutable; Revision-scoped | Pactrun owns the installed executable and support content identified by the exact Revision. |
| Managed Input Binding | Persistent; Instance-scoped | Pactrun owns the detached, opaque value and manages its presence, replacement, retention, and protection. |
| `ServiceStorage` / ServiceStorage-backed Managed Service Resource | Persistent; Instance-scoped | Pactrun manages provided-storage lifetime. The service owns the live contents. |
| Workspace | Temporary; execution-scoped | A Hook uses granted scratch authority for uncommitted work and staging. |
| Run Artifact | Run-scoped | Pactrun owns committed output belonging to one Run. |
| Snapshot | Durable and immutable | Pactrun owns a validated recovery representation, not a live-state mirror. |

A Managed Service File is a file-shaped ServiceStorage-backed Managed Service
Resource. The service may create or mutate its authoritative live bytes without
Pactrun observing the mutation time. Pactrun may identify and expose the
resource contractually, but MUST NOT:

- Treat its live bytes as a Managed Input Binding.
- Maintain a second authoritative-looking copy through implicit synchronization.
- Claim the linearization guarantees that apply to Pactrun-owned bindings.

This resource category does not automatically include Docker volumes, external
databases, remote objects, or other service-owned resources. The
[identity and ownership rules](../instances/service-resources.md)
define the ServiceStorage-backed boundary.

## Conceptual layers

```text
CLI
        -> Application and Resolution
        -> Domain and Authoring
        -> Workflow Compiler
        -> Execution Plan
        -> Executor
             -> Hook Runtime
             -> Managed Data
             -> Persistence
```

These layers assign responsibilities; they do not prescribe a particular crate,
module, trait, or repository layout.

### PR-REQ-0009 - Modular-monolith dependency direction

The architecture MUST maintain explicit ownership and a central composition
root. Core domain code MUST NOT depend on CLI, persistence-adapter, or
third-party adapter types. Implementations MUST avoid global mutable state,
implicit registration, service locators, untyped dynamic maps, and string-key
plumbing for domain contracts.

**Verification: PR-TEST-0605.**

### PR-REQ-0010 - Policy and mechanism separation

Pactrun-specific semantics MUST be owned by Pactrun domain code. Stable
third-party libraries MAY supply commodity mechanisms, but library behavior and
types MUST NOT become the source of Pactrun policy or identity semantics.

**Verification: PR-TEST-0041, PR-TEST-0068, PR-TEST-0605.**

## Product-design constraints {#normative-product-design-constraints}

- Pactrun MUST preserve a high ceiling and a low floor: metadata, one Action,
  and one Hook are sufficient for the smallest useful Pack. Advanced
  capabilities MUST NOT increase the minimum authoring burden for simple Packs.
- Operations, diagnostics, Plans, and recovery guidance MUST let Pack users
  predict expected results, identify missing prerequisites, and understand
  available recovery actions.
- Package contracts MUST NOT force authors to understand unnecessary internal
  DAG, lock, transaction, checkpoint, or recovery-bookkeeping mechanisms.
- Common authoring paths, structures, and diagnostics SHOULD remain predictable
  for both humans and LLMs. Advanced capabilities SHOULD NOT pollute the
  smallest valid examples.
- Pactrun MUST claim only security properties enforced by its actual authority
  or isolation mechanisms.
