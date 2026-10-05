---
title: Command and Output Reference
---

# Command and Output Reference

The [object-ID selector contract](./id-selectors.md) defines exact IDs and
unique prefixes for eligible operands. State tokens and author-assigned names
retain exact matching.

Current author descriptions are presented at the capability query and plan
boundaries specified by [PR-REQ-0363](./machine-output.md#pr-req-0363---capability-presentation-at-inspection-boundaries).
They are not verified prerequisites, execution eligibility, or historical facts.

## Resource areas

### PR-REQ-0353 - Human catalog, retained history and local metadata commands

Revision list/show and metadata show MUST expose installed exact identities and
complete typed metadata sources without depending on source directories.
Human summaries MAY truncate values explicitly; `--no-trunc` and detailed show
MUST expose full values. Truncated identifiers MUST NOT become resolvable input.
Text MUST be rendered without terminal-control injection. Machine-readable output follows the
[CLI machine interface](./machine-output.md).

`revision list`, `run list`, `instance history list` and `instance deletion list`
MUST use exclusive identity keyset pagination, default 50 and `--limit` 1-500.
`--after` need not identify a still-present row. Revision ordering uses exact
identity; Run and Instance lists use their respective IDs, not chronology.
Queries MUST fetch at most limit plus one base objects, report additional rows,
and provide a continuation preserving selectors and presentation options.
Each command MUST observe one read snapshot; separate pages are not frozen.

`run list` without a selector MUST enumerate retained Runs globally. A positional
name resolves only a live Instance; mutually exclusive `--instance-id` selects
retained history. Unknown identities fail; known identities without Runs return
an empty result. Instance history MUST distinguish recorded and current names;
deletion lists MUST discover obligations and receipts without duplicating IDs.
Queries MUST NOT initialize or upgrade storage, reconcile, create Runs, invoke
Hooks, expose native storage paths or read Input/Secret payloads.

`revision alias`, `revision note` and `revision trust` show/set/clear MUST use
typed values. Writes require exact Revision identities and explicit expected
states (`--expect` or `--expect-absent`), never an implicitly read expectation.
Alias clearing requires its exact expected target. PR-REQ-0253/0255 govern
validation, atomic semantic CAS, idempotency and absence. Conflicts MUST NOT
overwrite current state and MUST provide a reinspection instruction. Local trust
remains descriptive. No generic force, metadata history or version token is added.

**Verification: PR-TEST-0529, PR-TEST-0530, PR-TEST-0531, PR-TEST-0532, PR-TEST-0533, PR-TEST-0534, PR-TEST-0535, PR-TEST-0536, PR-TEST-0537.**

Closed command spelling (each listed catalog takes `--limit`, `--after` and
`--no-trunc`; show and write commands do not):

```text
pactrun revision list
pactrun revision show <revision-ref>
pactrun revision metadata show <revision-ref>
pactrun run list [<name> | --instance-id <id>]
pactrun instance history list
pactrun instance history show <instance-id>
pactrun instance deletion list
pactrun revision alias show <alias>
pactrun revision alias set <alias> <exact-ref> (--expect-absent | --expect <exact-ref>)
pactrun revision alias clear <alias> --expect <exact-ref>
pactrun revision note show <revision-ref>
pactrun revision note set <exact-ref> --value <text> (--expect-absent | --expect <text>)
pactrun revision note clear <exact-ref> (--expect-absent | --expect <text>)
pactrun revision trust show <revision-ref>
pactrun revision trust set <exact-ref> <trusted|distrusted> (--expect-absent | --expect <trusted|distrusted>)
pactrun revision trust clear <exact-ref> (--expect-absent | --expect <trusted|distrusted>)
```

Name-selected Run continuations MUST retain the resolved InstanceId rather than
resolving a potentially reused name on the next page. Human escaped text MUST
distinguish literal backslashes from escape sequences and present strings from
absence markers. Revision references retain their exact/label/alias grammar.

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
`run` namespace MUST be reserved for Run history and retained-output management,
including list, show, deletion and Artifact operations. It MUST NOT invoke
Package-defined Actions. Action introspection MUST be available independently
from invocation.

**Verification: PR-TEST-0114.**

### PR-REQ-0115 - Instance operations

The CLI MUST provide conceptual operations to create, list, show, migrate, and
delete Instances. Creation MUST permit an incomplete Instance. Inspection MUST
show missing required Inputs and trust state. Deletion MUST provide an explicit
AbandonManagement spelling rather than relying on a generic force flag.

PR-REQ-0271 defines create, list, and show syntax; Migration and
deletion have their own implemented command contracts in the
[Migration reference](../migrations/commands.md) and
[retirement reference](../lifecycle/retirement.md).

**Verification: PR-TEST-0436, PR-TEST-0437, PR-TEST-0525, PR-TEST-0599, PR-TEST-0604.**

### PR-REQ-0116 - Input operations

The CLI MUST provide conceptual operations to list, set, export, and delete
Instance Inputs according to active, retained, required, optional, and Secret
rules. Effective Secret classification MUST combine the active Revision
declaration with the payload's persistent sticky protection floor under
PR-REQ-0034, and MUST NOT be chosen by an arbitrary CLI flag.

PR-REQ-0271 defines Input list, set, export, and delete syntax and delegates
payload, retained, CAS, Secret, and output-publication semantics to
PR-REQ-0266 through PR-REQ-0268.

**Verification: PR-TEST-0076, PR-TEST-0147, PR-TEST-0280, PR-TEST-0558, PR-TEST-0593.**

### PR-REQ-0117 - Snapshot operations

The CLI MUST provide conceptual create, list, show, restore, export, import, and
delete operations for Snapshots. Filtering Snapshots by Instance MUST first
resolve the live Instance to its exact identity and then filter provenance; it
MUST NOT imply ownership.

**Verification: PR-TEST-0268, PR-TEST-0460.**

### PR-REQ-0118 - Revision operations

The CLI MUST provide conceptual list, show, export, import, and delete
operations for Revisions. Human references MUST be resolved to exact identity,
and ambiguity MUST fail.

**Verification: PR-TEST-0465, PR-TEST-0529, PR-TEST-0535, PR-TEST-0538, PR-TEST-0545.**

The [Pack distribution contract](../packages/distribution.md) defines
`revision export` and import through `pack install`, without a second import
command.

### PR-REQ-0119 - Security-sensitive authorization spelling

Secret export, sensitive Snapshot export, Secret declassification, recovery
override, and `AbandonManagement` MUST each use explicit, purpose-specific
authorization intent. A generic yes or force option MUST NOT silently authorize
disclosure, declassification, recovery bypass, or abandonment.

**Verification: PR-TEST-0078, PR-TEST-0173, PR-TEST-0270, PR-TEST-0311, PR-TEST-0437, PR-TEST-0593.**

### PR-REQ-0120 - Structured output boundary

Management and inspection commands SHOULD provide versioned machine-readable
output. In human mode, Hook terminal channels MUST remain direct streams and
MUST NOT be embedded in a JSON-style wrapper.

The [versioned JSON interface](./machine-output.md) is selected by a leading
`--format human|json|jsonl`; human is the default. Raw Input export to stdout
retains the payload stream boundary in PR-REQ-0266 for human and JSON modes.
JSONL requires a file destination for raw exports. Noninteractive terminal
output follows the ephemeral delivery contract in PR-REQ-0366; interactive
execution is refused in machine modes. Planning remains available.
Format selection does not change operation authorization or recovery.

**Verification: PR-TEST-0556, PR-TEST-0558, PR-TEST-0559, PR-TEST-0560, PR-TEST-0561, PR-TEST-0563, PR-TEST-0564, PR-TEST-0565.**

### PR-REQ-0271 - Application boundary and core management commands {#pr-req-0271---m2-application-and-minimal-human-cli}

The Application layer MUST expose crate-private typed orchestration equivalent to:

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

The standalone executable MUST obtain a non-empty absolute caller-provisioned
storage root from `PACTRUN_STORAGE_ROOT` and use its fixed `database/`,
`runtime-content/`, and `staging/` children. A missing or invalid value MUST
fail before data acquisition, schema migration, or persistent side effects.
The standalone executable has no `--storage-root` option or platform default. `pack generate-id`,
root `--help`, and root `--version` do not need the variable; the variable is
process configuration and is not an Input acquisition channel. Native package
launchers supply a platform-selected root when this variable is absent; they
do not replace an explicitly supplied value, including an invalid one.

Root help MUST describe each listed public command form and the important listed
options in concise task-facing language, including destructive and explicit
authorization boundaries. Human and machine help MUST expose the same complete
documentation text (machine `result.usage`), without opening storage or launching
a Hook. Descriptions do not introduce new command spellings or options.

**Verification: PR-TEST-0649.**

These operations are not a stable public Rust API. In particular,
`InstallPackSource(..., explicit_local_metadata)` is an intentional internal
orchestration capability for a typed local-metadata batch. Source YAML cannot carry local
metadata, and the CLI install command MUST pass an empty local-metadata
batch. The install command MUST NOT infer alias, note, or trust options or a `LocalInstall`
record from this internal parameter.

The core management commands have these forms:

The create-and-restore option is specified separately by
[PR-REQ-0346](../snapshots/commands.md#pr-req-0346---create-an-instance-and-restore-its-snapshot);
it composes Create with Restore rather than changing ordinary Create semantics.

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

Human command syntax is not an independent format-version domain. `pack generate-id` prints one
OS-CSPRNG-generated canonical `PackageId` and changes neither source nor
repository state. `pack install` fails when exact `package_id` is missing and
applies only source-projected portable metadata. Duplicate initial Input
identities and multiple stdin sources fail before acquisition. File and stdin
sources preserve exact bytes.

The `<reference>` grammar uses
`label:<ReferenceLabel>`, `alias:<LocalAlias>`, or
`exact:<PackageId>/sha256:<RevisionContentDigest-hex>`. The
[ID selector contract](./id-selectors.md) permits unique lowercase
hexadecimal prefixes of at least eight digits in either exact-reference component.
Bare tokens, uppercase spellings, missing Package identity or algorithm, and
repository guessing remain invalid. Resolution MUST produce one fixed exact
`RevisionIdentity` before the typed operation begins. Labels, aliases, and
state-version tokens retain their exact meanings.

Every `<instance>` operand accepts only exact `InstanceName`. Comparison
uses the preserved UTF-8 bytes with no trimming, normalization, case folding,
prefix lookup, or locale comparison. This operand is a name, not an `InstanceId`;
successful name resolution MUST produce the exact `InstanceId` before entering
the typed application operation.

The typed set and delete requests always require an expected state version. At
the human boundary only, omission of `--if-version` makes the CLI read the
current token once and submit that strict request; an explicit token is never
refreshed automatically. Export follows PR-REQ-0266, including atomic
no-clobber file publication, non-rollbackable raw stdout, and no force or
overwrite spelling. `--authorize-secret-export` is the explicit Input Secret export
authorization and has the narrow meaning in PR-REQ-0268.

Versioned output is defined by the
[CLI machine interface](./machine-output.md); stable error
identity remains owned by the [error taxonomy](./errors.md).

**Verification: PR-TEST-0078, PR-TEST-0079.**

### PR-REQ-0284 - Action, Plan, Run, and recovery commands {#pr-req-0284---m3-human-action-plan-run-and-recovery-spelling}

The CLI MUST provide these Action, Plan, Run, and recovery forms:

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
pactrun run list [<instance> | --instance-id <id>] [--limit <1..500>] [--after <run-id>] [--no-trunc]
pactrun run show <run-id>
pactrun run reconcile
pactrun instance resolve-manual-recovery <instance> [--if-version <token>]
```

Every exact Instance operand and Action/Parameter identity is resolved using
the existing identity grammar. RunId values are complete 32-character lowercase
hexadecimal identities; accepted selector prefixes are separately owned by
the [ID selector contract](./id-selectors.md).
`action list/show` and `run list/show` are structural-only
inspection and MUST not reconcile, launch, compile, acquire a staging lease,
or migrate storage. `--plan` uses the same read-only opening, displays the
planned work and applicable warnings, labels the output as a preview,
and MUST not create a Run or launch a Hook. If
`--authorize-recovery-override` is supplied to `--plan`, the output MUST show
that the authorization is projected for this invocation only; it MUST not
change the persisted recovery guard. A host-native `resolved_path` in the
human Plan projection MUST be lossless: ordinary valid UTF-8 paths remain
readable with terminal escaping, while non-UTF-8 paths use an explicit
platform-native code-unit or byte representation. This representation is a
human projection, not the machine-readable response format. Structured output
follows the [CLI machine interface](./machine-output.md). There is no raw
inspection option or public Rust API. Artifact export and deletion follow the
[object lifecycle contract](../lifecycle/objects-gc.md).

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

## Specialized export naming

### PR-REQ-0357 - Specialized envelope export filenames

Revision and Snapshot export interpret `--output <base-path>` as a filename base,
not a complete destination. The CLI MUST unconditionally append `.pack` for a
Revision and `.snapshot` for a Snapshot, exactly once per command. It MUST NOT
detect, strip, replace, normalize or avoid an existing suffix: `rv` produces
`rv.pack`, `rv.pack` produces `rv.pack.pack`, and `backup.snapshot` produces
`backup.snapshot.snapshot`. Parent path components and native filename units are
preserved. Empty bases, stdout `-`, and directory-only spellings MUST be refused.

All publication and no-clobber checks MUST use the final appended path, not the
base. An existing base file is not the destination and MUST remain untouched;
existing final destinations MUST never be replaced. Missing parent directories
are not created automatically. Success MUST report the final path using the
CLI's safe host-path presentation. The application publication boundary continues
to receive an exact final path; it does not append another suffix.

This is a CLI naming change, not a container, identity, integrity, permission or
schema change. Snapshot exports remain unencrypted Stored-ZIP and require
`--authorize-sensitive-export`; Revision exports retain their warning without
that flag. Imports consume the exact supplied path and validate contents, never
infer validity from an extension. Existing Snapshot `.zip` files remain importable.
Input and Run Artifact exports remain raw bytes with caller-selected complete
filenames; Input stdout remains supported. Raw exports MUST NOT acquire these
suffixes or a new envelope implicitly.

The supplied path is a base path: an existing suffix is not stripped. A doubled
suffix is deliberate, not a format-selection mechanism.

**Verification: PR-TEST-0550, PR-TEST-0551, PR-TEST-0552.**

## Operation-specific spelling {#remaining-open-spelling}

The [Snapshot commands](../snapshots/commands.md) specify Capture,
Restore, import, export, verification, and inspection. Unsupported stores are
refused without conversion or data deletion. The [Migration commands](../migrations/commands.md) provide
path discovery, planning and implemented execution. Recovery uses
PR-REQ-0287; Instance deletion and `AbandonManagement` use the
[retirement contract](../lifecycle/retirement.md).
[Managed-object lifecycle](../lifecycle/objects-gc.md) separately adds
Snapshot deletion, and [Pack transport](../packages/distribution.md)
owns Revision import/export. These commands preserve the purpose-specific
authorization boundaries in PR-REQ-0119. Unspecified commands and options MUST
NOT be inferred from an internal application API.
