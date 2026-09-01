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

PR-REQ-0271 fixes only the M2 create, list, and show spelling; Migration and
deletion remain later milestone commands.

**Verification: Pending automated coverage.**

## PR-REQ-0116 - Input operations

The CLI MUST provide conceptual operations to list, set, export, and delete
Instance Inputs according to active, retained, required, optional, and Secret
rules. Effective Secret classification MUST combine the active Revision
declaration with the payload's persistent sticky protection floor under
PR-REQ-0034, and MUST NOT be chosen by an arbitrary CLI flag.

PR-REQ-0271 fixes the M2 list, set, export, and delete spelling and delegates
payload, retained, CAS, Secret, and output-publication semantics to
PR-REQ-0266 through PR-REQ-0268.

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

M2 deliberately adds no machine-readable envelope; raw Input export to stdout
is the payload stream boundary in PR-REQ-0266.

**Verification: Pending automated coverage.**

## PR-REQ-0271 - M2 application and minimal human CLI

M2 MUST expose crate-private typed orchestration equivalent to:

```text
GeneratePackageId
InstallPackSource(source_root, explicit_local_metadata)
CreateInstance(name, revision_ref, initial_binding_sources)
ListInstances
ShowInstance
ListInputs
SetInput(instance, input_id, expected_version, source)
DeleteInput(instance, input_id, expected_version)
ExportInput(instance, input_id, destination, secret_authorization?)
```

These operations are not a stable public Rust API. In particular,
`InstallPackSource(..., explicit_local_metadata)` is an intentional internal
orchestration capability for a typed M1-D batch. Source YAML cannot carry local
metadata, and the M2 human install command MUST pass an empty local-metadata
batch. M2 MUST NOT infer alias, note, or trust options or a `LocalInstall`
record from this internal parameter.

The closed M2 minimal human CLI has exactly this fixed spelling:

```text
pactrun pack generate-id
pactrun pack install <source-root>

pactrun instance create <name> --revision <reference>
    [--input-file <input-id>=<host-path>]...
    [--input-stdin <input-id>]
pactrun instance list
pactrun instance show <instance>

pactrun input list <instance>
pactrun input set <instance> <input-id>
    (--file <host-path> | --stdin)
    [--if-version <token>]
pactrun input export <instance> <input-id>
    --output <host-path|->
    [--authorize-secret-export]
pactrun input delete <instance> <input-id>
    [--if-version <token>]
```

This spelling is normative for the minimal M2 profile but is not a Frozen
format or public compatibility version. `pack generate-id` prints one
OS-CSPRNG-generated canonical `PackageId` and changes neither source nor
repository state. `pack install` fails when exact `package_id` is missing and
applies only source-projected portable metadata. Duplicate initial Input
identities and multiple stdin sources fail before acquisition. File and stdin
sources preserve exact bytes.

The typed set and delete requests always require an expected state version. At
the human boundary only, omission of `--if-version` makes the CLI read the
current token once and submit that strict request; an explicit token is never
refreshed automatically. Export follows PR-REQ-0266, including atomic
no-clobber file publication, non-rollbackable raw stdout, and no force or
overwrite spelling. `--authorize-secret-export` is the only M2 Secret export
authorization and has the narrow meaning in PR-REQ-0268.

M2 defines no machine-readable envelope, stable public error catalog, or
automation API, and it does not change Frozen Error Taxonomy V1.

**Verification: Pending automated coverage.**

## Remaining open spelling

Exact subcommands and options for Migration, Snapshot, recovery, Instance
deletion, `AbandonManagement`, and other later milestones remain open. Their
future security-sensitive spellings must preserve PR-REQ-0119. The fixed M2
profile above MUST NOT be expanded implicitly to fill those later gaps.
