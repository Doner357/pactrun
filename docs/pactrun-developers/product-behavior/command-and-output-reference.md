---
title: Command and Output Reference
---

# Command and Output Reference

**Status: Normative product behavior specification. Exact option spelling
remains open specification work where explicitly noted.**

## Resource areas

```text
pactrun
|- pack
|- revision
|- instance
|- input
|- action
|- invoke
|- snapshot
`- run
```

## PR-REQ-0114 - Action and Run namespaces

Package-defined operations MUST use `pactrun invoke <instance> <action>`. The
`run` namespace MUST be reserved for Run history inspection, including list and
show operations. Action introspection MUST be available independently from
invocation.

**Verification: Pending automated coverage.**

## PR-REQ-0115 - Instance operations

The CLI MUST provide conceptual operations to create, list, show, migrate, and
delete Instances. Creation MUST permit an incomplete Instance. Inspection MUST
show missing required Inputs and trust state. Deletion MUST provide an explicit
AbandonManagement spelling rather than relying on a generic force flag.

**Verification: Pending automated coverage.**

## PR-REQ-0116 - Input operations

The CLI MUST provide conceptual operations to list, set, export, and delete
Instance Inputs according to active, retained, required, optional, and Secret
rules. Secret classification MUST come from the active Revision declaration and
MUST NOT be chosen by an arbitrary CLI flag.

**Verification: Pending automated coverage.**

## PR-REQ-0117 - Snapshot operations

The CLI MUST provide conceptual create, list, show, restore, export, import, and
delete operations for Snapshots. Filtering Snapshots by Instance MUST first
resolve the live Instance to its exact identity and then filter provenance; it
MUST NOT imply ownership.

**Verification: Pending automated coverage.**

## PR-REQ-0118 - Revision operations

The CLI MUST provide conceptual list, show, export, import, and delete
operations for Revisions. Human references MUST be resolved to exact identity,
and ambiguity MUST fail.

**Verification: Pending automated coverage.**

## PR-REQ-0119 - Security-sensitive authorization spelling

Secret export, sensitive Snapshot export, Secret declassification, recovery
override, and `AbandonManagement` MUST each use explicit, purpose-specific
authorization intent. A generic yes or force option MUST NOT silently authorize
disclosure, declassification, recovery bypass, or abandonment.

**Verification: Pending automated coverage.**

## PR-REQ-0120 - Structured output boundary

Management and inspection commands SHOULD provide versioned machine-readable
output. Raw and interactive Hook terminal channels MUST remain direct streams
and MUST NOT be embedded in a JSON-style wrapper.

**Verification: Pending automated coverage.**

## Open spelling

Exact subcommand names and options for the security-sensitive actions above
remain open specification work. Initial Input acquisition spelling is a
separate deterministic binding-source design, not a sensitive-data
authorization boundary. Its spelling, completeness diagnostics, and versioned
structured schemas also remain open. Implementers must not treat examples on
this page as permission to weaken the established intent or behavior.
