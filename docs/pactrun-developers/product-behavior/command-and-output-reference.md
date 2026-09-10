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

### PR-REQ-0114 - Action and Run namespaces

Package-defined operations MUST use `pactrun invoke <instance> <action>`. The
`run` namespace MUST be reserved for Run history inspection, including list and
show operations. Action introspection MUST be available independently from
invocation.

**Verification: PR-TEST-0114.**

### PR-REQ-0115 - Instance operations

The CLI MUST provide conceptual operations to create, list, show, migrate, and
delete Instances. Creation MUST permit an incomplete Instance. Inspection MUST
show missing required Inputs and trust state. Deletion MUST provide an explicit
AbandonManagement spelling rather than relying on a generic force flag.

PR-REQ-0271 fixes only the M2 create, list, and show spelling; Migration and
deletion remain later milestone commands.

**Verification: Pending automated coverage.**

### PR-REQ-0116 - Input operations

The CLI MUST provide conceptual operations to list, set, export, and delete
Instance Inputs according to active, retained, required, optional, and Secret
rules. Effective Secret classification MUST combine the active Revision
declaration with the payload's persistent sticky protection floor under
PR-REQ-0034, and MUST NOT be chosen by an arbitrary CLI flag.

PR-REQ-0271 fixes the M2 list, set, export, and delete spelling and delegates
payload, retained, CAS, Secret, and output-publication semantics to
PR-REQ-0266 through PR-REQ-0268.

**Verification: Pending automated coverage.**

### PR-REQ-0117 - Snapshot operations

The CLI MUST provide conceptual create, list, show, restore, export, import, and
delete operations for Snapshots. Filtering Snapshots by Instance MUST first
resolve the live Instance to its exact identity and then filter provenance; it
MUST NOT imply ownership.

**Verification: Pending automated coverage.**

### PR-REQ-0118 - Revision operations

The CLI MUST provide conceptual list, show, export, import, and delete
operations for Revisions. Human references MUST be resolved to exact identity,
and ambiguity MUST fail.

**Verification: Pending automated coverage.**

### PR-REQ-0119 - Security-sensitive authorization spelling

Secret export, sensitive Snapshot export, Secret declassification, recovery
override, and `AbandonManagement` MUST each use explicit, purpose-specific
authorization intent. A generic yes or force option MUST NOT silently authorize
disclosure, declassification, recovery bypass, or abandonment.

**Verification: Pending automated coverage.**

### PR-REQ-0120 - Structured output boundary

Management and inspection commands SHOULD provide versioned machine-readable
output. Raw and interactive Hook terminal channels MUST remain direct streams
and MUST NOT be embedded in a JSON-style wrapper.

M2 deliberately adds no machine-readable envelope; raw Input export to stdout
is the payload stream boundary in PR-REQ-0266.

**Verification: Pending automated coverage.**

### PR-REQ-0271 - M2 application and minimal human CLI

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

Every stateful M2 command MUST obtain a non-empty absolute caller-provisioned
storage root from `PACTRUN_STORAGE_ROOT` and use its fixed `database/`,
`runtime-content/`, and `staging/` children. A missing or invalid value MUST
fail before data acquisition, schema migration, or persistent side effects.
M2 has no `--storage-root` spelling or platform default. `pack generate-id`,
root `--help`, and root `--version` do not need the variable; the variable is
process configuration and is not an Input acquisition channel.

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

The M2 `<reference>` grammar is closed to
`label:<ReferenceLabel>`, `alias:<LocalAlias>`, or
`exact:<PackageId>/sha256:<RevisionContentDigest-hex>`. Bare tokens, digest
prefixes, uppercase spellings, missing Package identity or algorithm, and
repository guessing are invalid. Resolution MUST produce one exact
`RevisionIdentity` before the typed operation begins.

Every M2 `<instance>` operand accepts only exact `InstanceName`. Comparison
uses the preserved UTF-8 bytes with no trimming, normalization, case folding,
prefix lookup, or locale comparison. M2 exposes no human `InstanceId` spelling;
successful name resolution MUST produce the exact `InstanceId` before entering
the typed application operation.

The typed set and delete requests always require an expected state version. At
the human boundary only, omission of `--if-version` makes the CLI read the
current token once and submit that strict request; an explicit token is never
refreshed automatically. Export follows PR-REQ-0266, including atomic
no-clobber file publication, non-rollbackable raw stdout, and no force or
overwrite spelling. `--authorize-secret-export` is the only M2 Secret export
authorization and has the narrow meaning in PR-REQ-0268.

M2 defines no machine-readable envelope, stable public error catalog, or
automation API, and it does not change Frozen Error Taxonomy V1.

**Verification: PR-TEST-0078, PR-TEST-0079.**

### PR-REQ-0284 - M3 human Action, Plan, Run, and recovery spelling

The M3 human CLI MUST provide exactly these additional command forms:

```text
pactrun action list <instance>
pactrun action show <instance> <action>
pactrun invoke <instance> <action>
    [--param <parameter-id>=<text>]...
    [--param-file <parameter-id>=<host-path>]...
    [--param-stdin <parameter-id>]
    [--plan]
    [--authorize-recovery-override]
    [--startup-timeout-ms <milliseconds>]
    [--action-timeout-ms <milliseconds>]
    [--termination-grace-ms <milliseconds>]
pactrun run list <instance>
pactrun run show <run-id>
pactrun run reconcile
pactrun instance resolve-manual-recovery <instance> [--if-version <token>]
```

Every exact Instance operand and Action/Parameter identity is resolved using
the existing identity grammar. A RunId is a complete 32-character lowercase
hexadecimal value. `action list/show` and `run list/show` are structural-only
inspection and MUST not reconcile, launch, compile, acquire a staging lease,
or migrate storage. `--plan` uses the same read-only opening, displays the
exact compiled Plan and applicable warnings, explicitly states that it is not
Admission, and MUST not create a Run or launch a Hook. If
`--authorize-recovery-override` is supplied to `--plan`, the output MUST show
that the authorization is projected for this invocation only; it MUST not
change the persisted recovery guard. A host-native `resolved_path` in the
human Plan projection MUST be lossless: ordinary valid UTF-8 paths remain
readable with terminal escaping, while non-UTF-8 paths use an explicit
platform-native code-unit or byte representation. This representation is a
human projection, not a stable machine-readable envelope. No stable JSON envelope,
raw inspection option, Artifact export/delete command, or public Rust API is
added by this spelling.

Success is exit code 0, syntax or option error is 2, and operation failure or
cancellation is 1. `invoke` returns 0 only after durable `Succeeded` terminal
publication; cancellation before acceptance returns 1 and explicitly states
that no Run was created.

**Verification: PR-TEST-0114, PR-TEST-0118, PR-TEST-0119, PR-TEST-0120,
PR-TEST-0131, PR-TEST-0133, PR-TEST-0140, PR-TEST-0141, PR-TEST-0142,
PR-TEST-0144, PR-TEST-0145, PR-TEST-0148, PR-TEST-0149, PR-TEST-0150,
PR-TEST-0156, PR-TEST-0159, PR-TEST-0165, PR-TEST-0166, PR-TEST-0167,
PR-TEST-0168, PR-TEST-0172, PR-TEST-0173, PR-TEST-0174, PR-TEST-0175,
PR-TEST-0176, PR-TEST-0177.**

### PR-REQ-0287 - Explicit owner-loss reconciliation and manual recovery

`run reconcile` MUST perform one explicit owner-loss reconciliation pass for
the current storage root, print the RunIds it terminalized as `Interrupted`,
and succeed when there is nothing to change. It MUST read durable recovery
state only, MUST not replay Hooks or salvage output, and MUST leave Runs whose
owner lease is held or inconclusive unchanged. `run show` MAY therefore display
an unreconciled Running orphan.

`--authorize-recovery-override` MUST bypass only the trust guard for one
invocation. `instance resolve-manual-recovery` is the purpose-specific
operator assertion; it MUST use the existing guarded no-Hook, no-Compiler,
no-Run mutation and optional token-first CAS semantics. Neither command may
claim to have verified or restored service-owned state.

**Verification: PR-TEST-0112, PR-TEST-0118, PR-TEST-0156, PR-TEST-0159,
PR-TEST-0165, PR-TEST-0173, PR-TEST-0174.**

## Remaining open spelling

Exact subcommands and options for Migration, Snapshot, recovery, Instance
deletion, `AbandonManagement`, and other later milestones remain open. Their
future security-sensitive spellings must preserve PR-REQ-0119. The fixed M2
profile above MUST NOT be expanded implicitly to fill those later gaps.
