---
title: User reference
---

# User reference

Use these references for complete spelling, options, output meanings, limits,
and safety rules. These pages are lookup material, not a sequence of operations
to run. Spec and Development are optional maintainer sources.

| Need | Reference |
| --- | --- |
| Understand terms and ownership | [User vocabulary](./vocabulary.md) |
| Choose commands, identifiers, and output | [Command guide](../operations/command-and-output-reference.md), [command details](./command-details.md) |
| Supply Action parameters or deadlines | [Invoke reference](../operations/invoke-reference.md) |
| Inspect user preferences and Store defaults | [Configuration reference](./configuration.md) |
| Inspect Pactrun phases and timings | [Core diagnostics](./core-traces.md) |
| Understand a service prerequisite refusal | [Service failure details](./service-diagnostics.md) |
| Inspect historical Input provenance without keeping payload backups | [Input history](./input-history.md) |
| Copy a support report with explicit content selection | [Run support reports](./run-export.md) |
| Capture, verify, export, import, or Restore | [Snapshot details](./snapshot-details.md), [limits](./snapshot-limits.md) |
| Select a Migration path and target files | [Migration details](./migration-details.md) |
| Inspect or obtain an allowed service location | [Resource details](./resource-details.md) |
| Delete, abandon, or resolve a deletion attempt | [Retirement and recovery](./retirement-details.md) |
| Delete retained Runs, Artifacts, Snapshots, or Revisions | [Retained objects and GC](./object-lifecycle.md) |
| Automate safely | [Current JSON/JSONL reference](./operation-output.md), [legacy interface](./machine-output.md), [response schema](./cli-machine.schema.json), [event schema](./cli-events.schema.json) |

The advanced references are synchronized views of the same product rules, not
separately maintained alternatives. Their optional source panels are for tracing
rules or contributing to Pactrun. Use installed `pactrun --help` to check the
binary you are actually running.
