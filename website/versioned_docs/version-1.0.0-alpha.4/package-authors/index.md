---
title: Pack author guide
---

# Pack author guide

Use this section to write, validate, and deliver Packs and Hooks. You do not need
to read Pactrun's implementation plan or internal persistence design to start.

**Prerequisite:** Pactrun is installed and the selected command has been verified.
If not, complete the [shared executable setup](../guides/installation.md) first,
then return here. Installing Pactrun does not require writing a Pack.

## Build a working Pack

1. [Prepare an isolated authoring workspace](./setup.md).
2. [Run your first Shell Hook](./fundamentals/authoring-model.md).
3. [Declare Inputs and parameters](./fundamentals/actions-inputs-and-parameters.md).
4. Understand [runtime files and distribution](./fundamentals/recipes-and-runtime-content.md).
5. Read [Hook output, risk, and Cleanup guidance](./managed-capabilities/hooks-recovery-and-cleanup.md).

## Add only the capabilities your Pack needs

[Snapshots](./managed-capabilities/snapshots-and-managed-data.md) and
[Migration](./managed-capabilities/migrations.md) are independent worked tutorials.
Neither assumes that you have completed the other. Use a disposable authoring
environment with unused example names and destinations, or create a fresh one.
These optional tutorials are not automatic next steps after every Pack.

## Find a field or a rule

| Question | Author reference |
| --- | --- |
| What can I put in this YAML field? | [Pack fields, choices, defaults, and examples](./reference/pack-fields.md) |
| How do I declare live service data and transitions? | [Service and Migration fields](./reference/service-fields.md) |
| What may a script read, write, or return? | [Shell helper reference](./reference/shell-loader.md) |
| How do I implement a direct/interpreter Hook? | [Direct Hook message reference](./reference/hook-protocol.md) |
| What exact source syntax and metadata shapes are accepted? | [Complete Pack source reference](./reference/source-format.md) |
| What changes Revision identity, and what are the validation rules? | [Declaration types and identity](./reference/revision-format.md) |
| What Snapshot sizes and structure can the runtime handle? | [Snapshot authoring limits](./reference/snapshot-limits.md) |

The field guides explain everyday choices. The advanced references are generated
from the same product rules and remain in this author section. Their optional
maintainer-source panels provide traceability; they are not prerequisites.

## Before delivery

Test plans, successful execution, missing or invalid values, cancellation,
failure, and sensitive-data handling in disposable storage. Test exported Packs
in another fresh store. Tell operators which programs they need, what data each
operation changes, how to back it up, and how to inspect and recover from failure.
Hooks are trusted host programs; declaration checks do not provide an OS sandbox.
