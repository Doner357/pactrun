---
title: Introduction
---

# Introduction

This page is an informative tour. The Pactrun Developer section owns normative
requirements while the Pactrun User and Package Author sections remain planned
until after the initial release.

## What Pactrun is

Pactrun is a command-driven, local-first package management and execution tool.
A Package Author wraps service-specific behavior from tools such as Docker
Compose, systemd, database utilities, or service CLIs in a Hook. Pactrun owns a
consistent model for:

- immutable installed Revisions;
- managed Instances;
- Inputs and Secrets;
- typed invocation parameters;
- managed data and Snapshots;
- Revision Migration;
- Hook execution and Run history;
- workflow compilation, admission, and recovery.

Pactrun is a single executable and a modular monolith. It is not a daemon,
scheduler, remote orchestrator, web platform, secret vault, or replacement for
the underlying service tools.

## Product principles

The minimum useful Pack can consist of metadata, one Action, and one Hook.
Advanced Packs can add structured parameters, managed Inputs, Snapshots,
Migrations, Recipes, managed outputs, and interactive terminal behavior without
raising the minimum authoring cost for simple Packs.

Pactrun favors a single common path, typed diagnostics, explicit capability
descriptions, conservative data retention, and validation before side effects.
Managed-object legality, operation admission, and operational readiness remain
separate concepts. Pactrun only claims security properties that it can enforce.
The authoring surface is designed to be predictable for both human and LLM
authors: advanced features do not pollute the smallest valid example, and
authors are not required to understand internal transactions or recovery
bookkeeping.

## Core flow

```text
Package Source
    -> Revision Candidate
    -> validated and normalized content
    -> immutable Installed Revision
    -> managed Instance
    -> compiled Plan accepted as an execution attempt
    -> durable Run
    -> admission and execution
```

A Package Source is mutable authoring material. An Installed Revision is an
immutable execution identity. An Instance is the stable object a user manages
over time. Installing a Revision does not create an Instance. The durable Run
boundary is crossed only after resolution and compilation succeed and a
compiled Plan is accepted as an execution attempt; admission then continues in
that same Run.

## Operations at a glance

- An **Action** is a Package-defined user operation.
- **Snapshot Capture** and **Snapshot Restore** manage recovery objects.
- **Migration** moves an Instance along a declared Revision edge.
- **Cleanup** participates in managed Instance deletion.

These concepts may share the workflow execution machinery, but they are not one
generic authoring abstraction.

## Choose a role

- **Pactrun Developers** should begin with the
  [system model](./pactrun-developers/architecture/system-model.md) and
  [development policy](./pactrun-developers/engineering/development-and-verification.md).
- **Pactrun Users** can review the planned
  [Packages, Revisions, and Instances](./pactrun-users/concepts/packages-revisions-and-instances.md)
  guide structure.
- **Package Authors** can review the planned
  [authoring model](./package-authors/fundamentals/authoring-model.md) guide
  structure.
