---
title: Product Completion Milestones
---

# Product completion milestones

**Status: Approved planning scope and work order, recorded on 2026-09-17.
Detailed design and runtime implementation are pending.**

Subsequent execution of the first scope is tracked in the
[lifecycle implementation record](./managed-object-lifecycle-status.md). Its
approved lifecycle contracts and verified local develop integration do not complete the later
scopes or authorize publication. V9 and GC activation are tracked there separately
from the earlier Artifact-only checkpoint. The status above records the original
planning decision.

Subsequent Snapshot capacity/restore completion is recorded in its
[integrated closeout](./snapshot-capacity-and-restore-status.md). Shell Adapter /
Loader is [implemented, verified and integrated into local develop](./shell-adapter-loader-status.md).
Its diagnostic presentation gap is recorded as a separate unresolved follow-up,
not silently completed by integration. These execution records do not change the
historical planning status above or start machine-readable CLI output.

This informative plan records the operator's decisions after the completed
Instance-retirement milestone. Its list numbers express work order, not new
M-series identifiers. M8 remains rejected and its number is not reused.
The [roadmap](./implementation-roadmap.md) owns scheduling; [Spec](../spec/index.md)
owns observable behavior. This record does not start runtime implementation,
freeze candidate CLI/helper syntax, change current capacity limits or authorize
publication. Close the detailed design and affected Spec before each implementation.

The rationale paragraphs below record the reasons for this planning decision
now. They do not invent the original reasoning behind older implementations.

## Agreed work order

| Order | Milestone | Included small feature |
| --- | --- | --- |
| 1 | Managed Object Lifecycle and GC | Run Artifact export |
| 2 | Snapshot Capacity and Restore Workflow | Instance create-and-restore |
| 3 | Shell Adapter / Loader | None separately scheduled |
| 4 | Machine-readable CLI Output | None separately scheduled |
| 5 | Versioning and Baseline Consolidation | Version support and first-formal-baseline reorganization together |

Versioning is last, not a required preliminary design milestone. Complete the
product capabilities first, then consolidate their interfaces and implementation
into the first formal baseline. Until that approved consolidation, existing
contracts still govern development; planning a reset is not permission to
silently reinterpret Frozen bytes during ordinary implementation.

Artifact export and create-and-restore may be implemented and accepted as bounded
slices within their milestones without waiting for every larger slice to finish.
Do not add independent mini-milestones merely for these conveniences.

## 1. Managed Object Lifecycle and GC

### Purpose and existing foundation

Complete the endpoints of persistent managed-object lifetimes and safely reclaim
unneeded content. Ending a logical object's life and reclaiming its physical
content are related but separate operations.

The [resource-lifetime contract](../spec/foundations/resources-and-versioning.md)
already defines independent Snapshot lifetime, Run/Artifact retention, pins and
reachability. The current implementation is not equally complete for every object:

| Object | Existing foundation | Remaining work |
| --- | --- | --- |
| Snapshot | Durable first-class object, independent of origin Instance/Run and producer installation | Explicit deletion and safe content reclamation |
| Revision | Immutable installation, active-Instance guards and execution pins | Public deletion/uninstallation and unneeded runtime-content reclamation |
| Run record | Terminal results, reconciliation and release of execution-only references | History-removal rules that preserve necessary recovery/retirement evidence |
| Run Artifact | Internal streaming and independent deletion, with tests | Public export and deletion operations |
| Instance / ServiceStorage | Cleanup, Abandon, finalization and detached handoff/discard | Preserve those protections across new reclamation paths |
| Managed Input payload | Binding deletion and reference-aware payload reclamation | Verify that new object deletion preserves existing protections |
| Workspace / uncommitted candidate | Execution-scoped cleanup | Keep temporary data distinct from persistent objects |

`src/application.rs` already exposes crate-private `stream_run_artifact` and
`delete_run_artifact`. The human CLI reports Artifact summaries but does not
export or delete them. Test-only SQL deletion is not a user-operable lifecycle.

### Agreed design boundaries

- Keep object-specific deletion semantics; do not cascade all related objects.
  Snapshot has no default expiry and survives Instance/Revision removal.
- Separate provenance from strong references. Historical provenance need not
  permanently retain an installed Revision, while active use, accepted Runs,
  checkpoints and unresolved obligations protect data they still require.
- Coordinate deletion with Restore, export, verification and other active use.
  Define admission, commit, interruption and retry behavior explicitly.
- Reclaim only content whose eligibility follows from ownership and references,
  not age, filenames or the absence of a live Instance.
- Preserve existing retirement receipts and protected custody. Abandoned or
  detached service data is not generic garbage; explicit handoff/discard governs it.
- Inventory Package registration, historical identities and receipts without
  assuming that every row type needs a public delete command.

A finished Run has ended execution, not necessarily its record's lifetime.
Deleting history cannot erase evidence required to understand unresolved recovery
or retirement. Decide whether to refuse deletion or retain sufficient evidence;
do not silently discard an obligation to make a command succeed.

### Artifact export slice

Export a published Artifact identified by Run and output identity to a selected
file. Reuse streaming and reviewed staging/publication mechanisms. Do not
accidentally impose an unrelated Managed Input limit through a reused helper.
The output remains owned by its Run, not a new global Artifact taxonomy.

`pactrun run artifact export <run-id> <output-id> --output <path>` is candidate
syntax only. A file-only, no-overwrite first slice is recommended; destination,
overwrite and sensitive-data authorization details still need approval. Opaque
Artifact content is not assumed free of Secrets. An Action's file becomes an
Artifact only after valid submission and managed publication, not merely because
it was written into Workspace or an output slot.

### Rationale and exclusions

Putting these endpoints together lets them share a consistent reference model
and prevents export/delete races from being designed independently. Artifact
export is a delivery feature in this group, not itself GC. Existing service
retirement is reused rather than repeated.

No background daemon, scheduler, automatic default expiry, complex retention
platform, service-state sweep or generic dynamic GC framework is required.

### Open decisions and acceptance

Close public syntax/intent, exact reference and active-use rules, receipt/history
preservation, persistence changes, physical-work/transaction boundaries, retry
and space-reclamation behavior. Suggested slices are ownership inventory;
contracts; Artifact and Snapshot operations; Revision/content; Run history;
cross-object reclamation verification.

Test active-use coordination, unresolved obligations, repeated operations,
concurrent reads/deletes, process loss and I/O failures. Demonstrate both actual
eligible reclamation and non-destruction of protected/unrelated data. Export
tests cover exact bytes, empty/chunked files, missing objects, destination
conflicts and failures without a misleading completed output.

## 2. Snapshot Capacity and Restore Workflow

This scope is now implemented, verified and integrated into local develop. See
the [implementation and acceptance record](./snapshot-capacity-and-restore-status.md).
The foundation and pending-choice text below records the original planning
checkpoint; the linked normative contracts now own the approved changes.

### Purpose and existing foundation

Remove arbitrary fixed service-data byte ceilings and provide a convenient
composition of Instance creation and Restore. Snapshot is state recovery, not
Revision installation or whole-machine recovery. The availability of a compatible
Revision and its execution prerequisites is not a defect in that boundary.

At this planning checkpoint the [capacity contract](../spec/behavior/m4-runtime-capabilities.md)
enforced separate profiles, including an 8 GiB service blob and 16 GiB Capture
service closure. These are current build capabilities, not permanent identity
constraints. Chunked storage and streaming already exist, but real large-payload
tests are not proof of arbitrary-scale operation.

### Agreed capacity direction

Pactrun is to impose no fixed product byte ceiling on Snapshot service data.
Processing uses bounded memory within representable formats, supported backends
and available resources. Inability to complete produces safe failure and clear
diagnostics, not truncation or a false corruption/success claim.

Current PR-REQ-0293 remains binding until its owning contract is revised. The
new direction does not remove parser/metadata bounds, checked arithmetic,
structural validation or backend limits. Independent Managed Input and Hook
frame limits are not implicitly removed. No unlimited-resource or throughput
promise, or containment of trusted Hook writes, is introduced.

Audit acquisition, import, persistence, verification, export and Restore as one
path. Inspect metadata growth, logical expansion, staging space, large transactions,
cancellation and storage exhaustion; streaming one stage alone is insufficient.

### Create-and-restore convenience slice

An Instance creation option selects a supported Snapshot and composes the existing
Create operation followed by existing Restore. Candidate syntax is
`pactrun instance create <name> --revision <reference> --restore-from <snapshot-id>`.
This is not a new execution kind or a cross-operation atomic transaction.

Existing exact producer/target RevisionIdentity matching and Restore Admission
remain intact. There is no implicit Revision installation, Snapshot import,
Migration or compatibility relaxation. Incomplete Instance creation is already
legal; Restore evaluates its staged Snapshot context rather than requiring
duplicate pre-Restore Inputs.

| Outcome | Meaning |
| --- | --- |
| Create fails | Do not start Restore |
| Both succeed | Return success only after existing durable Restore success |
| Create succeeds, Restore fails or is refused | Keep the Instance; report partial completion, its identity/state and any Restore Run |
| Loss between operations | The Instance may exist with no Restore started; do not infer resume |
| Loss during Restore | Preserve existing Run/recovery behavior |

Do not automatically delete the new Instance on Restore failure. Bind Restore
to the exact created Instance rather than a replacement resolved later by name.
Read-only early checks do not replace Admission. Final flag spelling, explicit
versus derived Revision selection, initial Input interaction, Restore parameters,
execution options and any planning interface remain design decisions.

### Rationale, slices and acceptance

Growing service data should not require a new product model because it crosses
an arbitrary GiB threshold. The limit removal still needs evidence of bounded
processing and safe failures. Create-and-restore reduces repeated operator steps
without inventing a transaction or compensation engine; its grouping with capacity
shares Restore evaluation, not a mandatory implementation dependency.

Approve capacity rules, audit the full path, change only the necessary mechanisms
and test real increasing payload sizes including sizes beyond former ceilings.
Test memory bounds, identity/integrity, materialization, cancellation and disk
exhaustion. Counters alone do not establish large-data safety. Test convenience
against the equivalent two operations, including missing/incompatible Snapshots,
incomplete targets, partial success and concurrent changes. No backup scheduler,
automatic rollback or Revision-inclusive bundle is included.

## 3. Shell Adapter / Loader

The design gates below are now closed by the approved
[baseline](./design-notes/shell-adapter-loader-baseline.md), the owning
[Core V3](../spec/contracts/revision-core-format-v3.md),
[YAML V3](../spec/contracts/pack-source-yaml-v3.md) and
[Loader](../spec/contracts/shell-loader.md) contracts. The
[implementation record](./shell-adapter-loader-status.md) supplies actual evidence;
the original purpose, rationale and acceptance criteria below are retained.

### Purpose and existing foundation

Make shell Hooks practical through a built-in Session loader, not a second
workflow engine. The [Hook contract](../spec/contracts/hooks-recovery-and-cleanup.md)
already allows wrappers and SDKs that preserve one canonical semantic protocol.
The internal process adapter is not a public shell SDK.

[Hook V1](../spec/contracts/hook-protocol-v1.md) and
[Hook V2](../spec/contracts/hook-protocol-v2.md) provide the wire and state machines.
Framing uses a 4-byte U32BE length and a separate 16 MiB frame limit; this work
does not replace it with another wire format.

### Agreed loader model

The loader connects, handshakes, receives the Session, prepares convenient shell
interfaces, runs the script and performs protocol-aware termination. Scripts do
not manage connect/session_start/session_ready/close or add initialization and
exit boilerplate. One loader supports both usage levels:

- Ordinary scripts can use their normal arguments, environment and exit status
  without knowing Pactrun exists or calling any helper.
- Advanced scripts can optionally access granted parameters, Inputs, resources,
  Workspace, outputs, diagnostics, risk requests and operation-specific facilities.
  They still do not take over Session management.

The loader owns one persistent protocol connection. Repeated helper calls route
through that execution's loader, not new connections to the single-use endpoint.
It is shipped with the Pactrun executable, without requiring a new daemon or
bundling an interpreter. Direct Hooks and independent language SDKs remain valid.

### Exit and capability boundaries

Normal zero-status script exit defaults to a request for successful completion.
Nonzero exit requests failure; cancellation, abnormal loss and protocol/adapter
errors never become success merely because no error helper was called.

Original completion conditions still apply: no outstanding request, required
outputs submitted and valid risk/terminal ordering. Never auto-clear Open risk,
infer service coherence, collect arbitrary Workspace files or report receipt as
durable Run success. V2 transform Sessions keep their target_ready/receipt/exit
ordering rather than ordinary success completion. Those advanced signals remain
available without exposing Session mechanics.

Helpers hide encoding and request IDs, not semantics: risk entry returns success
only after acknowledgment. Preserve every supported protocol capability and
data representation, including opaque file content; do not require all bytes or
Secrets to pass through shell strings/environment variables. Keep helper IPC
separate from terminal streams. Do not silently change shell error options or
infer failure from intermediate commands that the script legitimately handles.

### Rationale, open decisions and acceptance

Authors should not each reimplement a binary client merely to run a shell script.
Hiding mechanical setup while retaining optional complete capabilities provides
a low floor without reducing the ceiling. The loader adds no authority, automatic
service repair, declarative workflow language or alternate protocol meaning.

Close shell/platform support, launch integration, helper names/returns, data
representation, private IPC, process/cancellation handling and output registration.
Test plain scripts, optional helpers, all supported operations, exact risk ack
ordering, missing outputs, helper/script/loader loss, terminal I/O and special
target publication. A direct Hook escape route is not a substitute for capability
parity in the loader. Real Pack evaluation can begin here and be repeated on the
final consolidated baseline.

## 4. Machine-readable CLI Output

### Purpose and existing foundation

Provide results that agents and ordinary automation can understand reliably.
Command semantics and permissions remain identical to human use. The aim is
output usability, not a privileged agent mode.

Current output uses human label/value lines, tabular text and textual errors;
some CLI paths stringify typed application failures. The
[structured-output boundary](../spec/behavior/command-and-output-reference.md)
already calls for versioned machine-readable management/inspection output.
[Error Taxonomy V1](../spec/contracts/error-taxonomy-v1.md) defines error identity,
not a whole result envelope or universal retryability classification.

### Agreed direction

- Offer general-purpose structured output; `--format json` is candidate syntax,
  not an approved flag. Do not repurpose existing `--output` file destinations.
- Project typed results/errors into deliberate public models before human/JSON
  rendering. Do not parse prose back into data or serialize private structs wholesale.
- Represent applicable identities, state tokens, readiness, missing Inputs,
  guards, Plan facts/runtime checks, Run results and errors as data. Human text
  can explain those facts but is not the only machine decision input.
- Preserve raw Input export and Hook terminal/interactive streams. Do not mix
  logs into a claimed single JSON result or wrap terminal channels as JSON.
- Preserve safe disclosure and exit semantics. Missing output does not prove
  no side effects; retain useful execution identity/partial-result information
  without automatic retry, replay or different authorization behavior.

### Rationale, open decisions and acceptance

One structured interface serves humans' scripts and agents alike. Implementing
it after the other capabilities avoids repeatedly redesigning results for new
deletion, export, Restore and capacity outcomes. It remains separate from the
shell loader because command presentation and Hook execution have different
responsibilities and failure boundaries.

Close flag/envelope/version, field types and absence rules, parse/startup error
mapping, output-channel ownership and the command coverage matrix. Design any
separate execution-result destination without assuming a new flag or event stream.
Unsupported raw/interactive combinations need explicit early behavior rather
than a silent fallback after side effects.

Start with management/inspection and Plans, then close other results and terminal
coexistence. Test schemas, errors, empty/absent data, safe disclosure, unchanged
side effects, invalid arguments and interrupted output. No HTTP/MCP server or
agent-specific command system is included.

## 5. Versioning and Baseline Consolidation

### Purpose and approved policy

Consolidate the completed product into its first formal baseline, then implement
how that baseline evolves. This combines versioning and baseline reorganization
from [release readiness](./release-readiness.md), and intentionally comes last.
The owning [product policy](../spec/foundations/product-versioning-and-compatibility.md)
defines formal same-Major compatibility and baseline acceptance.

Development iterations are not prior formal product releases. The shipping
product should understand its formal contracts, not the historical milestone
sequence. There is no development-generation discriminator or rejection solely
because data was produced during development. Previously created data that fully
conforms to the final contract is usable under that contract; this is current
validation, not a backward-compatibility path. Shape resemblance or parseability
alone does not prove identity, references, invariants or semantic conformance.

### Agreed scope

- Keep product, authoring, Hook, identity/integrity, transport, machine-output and
  persistence version domains independent where their contracts require it.
- Implement current support/requirement checks and future formal same-Major
  evolution boundaries without inventing a prior formal release or speculative
  migration chain. Reject unsupported meaning before affected side effects.
- Consolidate schema/formats, helper/CLI interfaces and product-oriented Spec.
  Preserve active obligations and traceability while separating development history.
- Remove development-only compatibility readers, dispatch branches and migration
  chains retained solely for historical versions. A mechanism still needed by
  the final contract is not removed merely because it was first written earlier.
- Accept fully conforming data irrespective of development provenance. Reject
  nonconforming data without legacy guessing, conversion, overwrite or deletion.
  Do not introduce a generation marker merely to exclude development data.
- Keep strict development Freeze decisions, vectors, verification practices and
  evolution experience in development history as reference, not shipping history
  that the runtime/tests must understand. Formal contracts may themselves be Frozen.
- Remove old milestone labels/dependencies from implementation and tests here;
  do not renumber real contract versions or discard still-valid behavioral tests.
- Retain the separate development-to-release promotion: work remains 0.1.0 until
  authorized 1.0.0 release validation, and that version change performs no second
  baseline reset. Conformance, not the shared 0.1.0 label, determines data usability.

### Rationale, open decisions and acceptance

Finishing capabilities first reduces repeated interface/schema reorganization.
The first formal product has no earlier formal version to support; imposing a
historical origin barrier would add machinery without serving that compatibility
goal. Same-Major obligations start from published formal contracts, not from
every experimental iteration. This does not establish a fixed support duration
or a security-only breaking-change exception policy.

Close requirement/capability syntax, supported-format and upgrade matrices,
final encodings and concrete removal/reorganization inventory. Do not turn the
cleanup into a generic OLD/NEW switch framework or retain development migration
chains to preserve experiments.

Acceptance verifies final-contract acceptance/rejection independent of origin,
complete invariants rather than field names, removal of legacy-only paths,
support refusal before side effects and formal evolution semantics. Retain
relevant crash/vector/property evidence under product-oriented names. Test exact
builds and final contracts; the actual release artifact is verified separately.

## Cross-cutting rules and release handoff

New functions and tests use product/behavior names, not milestone labels. Existing
labels wait for final consolidation rather than unrelated mass renaming now.
Follow the [contributor policy](./development-and-verification.md) for naming,
design rationale, Git Flow and validation. Concrete new behavior belongs in Spec,
not only in a deletable implementation-status record.

No implementation schedule or effort estimate is inferred from section length.
Lifecycle now covers more than Snapshot deletion, so the earlier Snapshot-only
difficulty estimate no longer applies. Capacity has large-data/backend uncertainty;
loader has process/protocol fidelity risk; full structured execution output is
broader than read-only JSON inspection. Versioning includes the final consolidation.

NetBird and authentik are suggested real-Pack evaluation candidates, not proven
integrations or a committed service-support matrix. Bad Pack risk reporting is
not automatically a Pactrun defect: use trials to test whether correct Packs can
express the intended workflow without inventing service-specific Domain concepts.

Internal end-to-end evaluation/correction, useful usage documentation, platform
qualification and release-mechanism rehearsal remain in release readiness.
Repeat affected validation on the final baseline. Formal publication is a
separately authorized action after acceptance, not an automatic milestone effect.
This planning record changes no executable, stored data or current runtime limit.
