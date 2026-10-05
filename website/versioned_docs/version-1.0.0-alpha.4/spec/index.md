---
title: Pactrun Specification
---

# Pactrun Specification

Pactrun installs immutable Pack Revisions and manages long-lived Instances.
The specification describes their definitions, behavior, interfaces, and data
boundaries for the selected documentation version.

Start with [System and Core Concepts](./core/index.md), or choose the subject
you need below. The [Vocabulary](./core/vocabulary.md) defines shared terms.

## [System and Core Concepts](./core/index.md)

Understand the objects Pactrun manages and the boundaries between installation, execution, and service-owned data.

## [Packages and Revisions](./packages/index.md)

Define, install, identify, and distribute immutable Pack Revisions. Descriptive metadata remains separate from operational identity.

## [Instances and Data](./instances/index.md)

Manage Instance state, Input bindings, Secrets, and service-owned resources. Each kind of data has its own ownership and lifetime.

## [Operations and Execution](./operations/index.md)

Resolve and preview operations, admit an exact execution context, and follow a Run through completion or failure.

## [Snapshots and Restore](./snapshots/index.md)

Capture a complete recovery representation, validate its integrity, transport it, and restore it to an exact-compatible Instance.

## [Migration](./migrations/index.md)

Move an Instance through declared Revision transitions. Each edge has its own requirements, service transformation, and durable commit.

## [Lifecycle and Recovery](./lifecycle/index.md)

Understand resource retention, Cleanup, retirement, abandonment, and recovery after interrupted work.

## [Hooks and Integration Interfaces](./interfaces/index.md)

Use the Hook protocol, shell adapter, CLI, and structured outputs without conflating their distinct authority and version boundaries.

## [Compatibility and Storage](./storage/index.md)

Determine supported versions and the persistence rules for immutable content, Revision records, and the management database.

For individual contracts, use the [reference index](./catalog.md).
