---
title: Remaining Capability Milestones
---

# Remaining capability milestones

**Status: Approved remaining scope and work order on 2026-09-19.
A is implemented, verified and integrated into local develop. C is implemented,
verified and integrated into local develop; D-E remain pending. B S0-S4 is implemented, verified
and integrated into local develop; see its
[record](./object-catalog-history-metadata-status.md).**

The operator approved the following grouping after the diagnostic and capability
gap review, and authorized recording the plan only. This record does not start
runtime implementation, amend Spec, allocate format/schema versions, commit,
merge, push or authorize publication. The [roadmap](./implementation-roadmap.md)
owns work order; [Spec](../spec/index.md) owns observable behavior. Each S0 must
close and obtain approval for externally meaningful decisions before implementation.

Lifecycle/GC, Snapshot Capacity and Restore Workflow, and Shell Adapter / Loader
remain implemented and integrated. Their acceptance evidence is not rewritten.
The original [2026-09-17 completion plan](./product-completion-milestones.md)
remains the historical scope/rationale record. This approval inserts three
capability milestones before its remaining machine-output and consolidation work.
A-E are sequence labels in this plan, not new M-series identifiers or versions;
M8 stays rejected and its number is not reused. Relative order is approved;
calendar dates and effort estimates are not assigned.

## Approved remaining order

| Order | Milestone | Delivery goal |
| --- | --- | --- |
| A | Execution Diagnostics and Instance State Observability | Make diagnostics, permitted failure explanations, exact Instance identity and trust/completeness state genuinely observable |
| B | Object Catalog, Historical Discovery and Metadata Operations | Make installed objects, retained history and existing metadata capabilities discoverable and operable |
| C | Revision Bundle Export and Import | Provide a portable immutable Revision round trip without rebuilding from source |
| D | Machine-readable CLI Output | Expose deliberate typed results for the completed command/inspection surface |
| E | Versioning and Baseline Consolidation | Close the formal baseline, support/evolution mechanisms and remaining obligation inventory |

**Delivery update:** A is implemented, verified and integrated into local develop.
B is also implemented, verified and integrated under the separately approved
[implementation baseline](./design-notes/object-catalog-history-metadata-baseline.md).
C is now implemented, verified and integrated into local develop under the
[Pack transport baseline](./design-notes/pack-transport-baseline.md). Its
[status record](./pack-transport-status.md) distinguishes implementation evidence
from the separately authorized local integration. D follows C and requires separate S0 approval.
The sections below retain the original grouping rationale;
the approved Pack baseline owns the subsequent concrete scope refinement.
The original work-order decision was **Next: A.**
At that decision, D is no longer the immediately next capability. A and B establish
the observable data and public operations; C adds transport results and failures.
Only then should D close machine-output models and command coverage. E stays last.
This avoids encoding missing features as JSON or repeatedly redesigning envelopes.

## A. Execution Diagnostics and Instance State Observability

### Scope

- Complete the path from canonical Hook diagnostic to a usable consumer, rather
  than counting validation and discarded text as diagnostic delivery.
- Present severity, Hook code, message and execution association while keeping
  Hook-authored text distinct from Pactrun error identity and durable Run outcome.
- Define both live presentation and post-run inspection. Do not silently defer
  the latter again or suggest unavailable text was never emitted.
- Show stable InstanceId, the current recovery guard/trust consequence, exact
  active Revision and configuration completeness. Input completeness alone must
  not be presented as unconditional execution eligibility.

### S0 decisions and slices

| Slice | Deliverable |
| --- | --- |
| S0 | Align PR-REQ-0097, PR-REQ-0283 and PR-REQ-0285; close live/history presentation, retention/disclosure policy and output-channel ownership; establish the shared gap inventory below |
| S1 | Typed diagnostic flow and complete Instance inspection models |
| S2 | Human live presentation, policy-governed post-run inspection and Instance status presentation |
| S3 | Positive and negative end-to-end acceptance, including cancellation, sensitive text and broken output |

The original grouping proposed live usability without default text persistence.
On 2026-09-19 the operator superseded it with default bounded retention and
ordinary post-run disclosure for debugging, including completion/protocol-error
explanations. See the [approved execution baseline](./execution-diagnostics-observability-status.md).
S0 must record defaults, authorization,
retention limits/lifetime, omitted/truncated/not-retained reporting and terminal
none/output/interactive behavior. This plan does not approve a specific logging
flag, table, field, schema version or opt-in spelling. Hook authors remain
responsible for sensitive text; Pactrun must not claim universal Secret detection.

### Completion gate

An emitted diagnostic must reach a user-visible result with its classification
and execution association. Post-run access must work under the approved policy,
or explicitly state that content was not retained. A guarded Instance must not
look merely ready because its Inputs are present. Renderer or output-pipe failure
must not lose execution ownership, replay Hooks or rewrite a committed outcome.
Diagnostic error severity must not silently become a second Run-failure rule.

## B. Object Catalog, Historical Discovery and Metadata Operations

### Scope

- Complete Revision list/show, including exact identities, relevant declarations
  and permitted metadata; discovery cannot require remembering an install response.
- Enumerate retained Runs without first resolving a currently live Instance name.
  Provide an explicit InstanceId selector and discoverable retirement/deletion
  records, with current names and historical provenance kept separate.
- Preserve name-reuse correctness. Never fabricate a past name or associate old
  history with a newly created same-name Instance.
- Expose label associations, presentation and provenance with their complete typed
  source tuples. Ambiguity must be inspectable, not resolved by implicit preference.
- Complete local alias creation/replacement/clearing and inspection, and local
  note/trust assessment read/set/clear operations, using existing explicit semantic
  CAS and idempotency rules rather than last-write-wins.

### S0 decisions and slices

| Slice | Deliverable |
| --- | --- |
| S0 | Close query scope, explicit selectors, deterministic ordering, bounded enumeration and metadata command semantics |
| S1 | Revision catalog and complete metadata inspection |
| S2 | Historical discovery across live and deleted Instances |
| S3 | Local alias, note and trust assessment operations |
| S4 | Read-only, name-reuse, conflict/CAS and usable end-to-end acceptance |

Local trust remains descriptive assessment, not a new execution-denial policy.
Publisher attribution remains a claim, not authentication. No LocalInstall model,
metadata history, metadata version token, fuzzy authoritative resolution or trust
policy is inferred. S0 closes command spelling against PR-REQ-0087/0088/0089,
PR-REQ-0118 and the existing typed metadata rules, particularly PR-REQ-0253/0255.

### Completion gate

An operator can find an installed Revision or retained history without saving
every returned ID beforehand. An alias reference has a usable creation and
management path. Distinct metadata claims remain inspectable and conflicts do
not become overwrites. Queries do not create Runs, implicitly reconcile or
upgrade storage. Deleted/name-reused objects remain separated by exact identity.

## C. Revision Bundle Export and Import

### Scope

Complete the remaining Revision transport operations in PR-REQ-0118; a Snapshot
bundle and source-directory installation are not substitutes. Define an
independently versioned, self-describing Revision Bundle carrying exact canonical
Revision components, the complete runtime-content closure and explicitly selected
portable metadata. Provide export, verification, import and usable results.

The installed immutable Revision is authoritative, not its old source directory.
Import must not rerun authoring or regenerate runtime content. Package identities,
canonical bytes and Revision digests survive the round trip. Packaging/compression
is not identity. Apply existing semantic tuple/idempotency rules to included claims;
do not silently choose a winner when labels become ambiguous.

Instance Inputs/Secrets, live ServiceStorage, Workspace and Runs are not implicit
bundle content. Local aliases, notes and trust assessments do not transfer by
default. This is not whole-store backup, registry, signing, publisher-trust
selection or a promise to detect hard-coded sensitive bytes in Pack content.

### S0 decisions and slices

| Slice | Deliverable |
| --- | --- |
| S0 | Close the bundle format, portable carriage, conflict/publication policy, compatibility and structural/resource limits |
| S1 | Validation and streaming export |
| S2 | Safe import and atomic authoritative publication |
| S3 | Cross-store round trip, repeat import, conflicts, corruption and hostile-input acceptance |

No envelope syntax, compression choice, version number, metadata merge option or
persistence migration is approved merely by this scope. S0 must preserve the
independent domains and existing constraints of PR-REQ-0021/0022 and PR-REQ-0081.

### Completion gate

Export followed by import into a fresh store preserves exact identity and bytes.
Repeated import is identity-idempotent and does not duplicate equivalent metadata.
Legal label ambiguity is retained and later resolution refuses it. Unsupported,
corrupt or malicious input does not publish partial authoritative objects or escape
the intended filesystem boundary. Existing Snapshot transport remains unchanged.

## D. Machine-readable CLI Output

Retain the [original machine-output direction](./product-completion-milestones.md#4-machine-readable-cli-output),
but close its public models after A-C. Human and machine renderers use the same
deliberate typed projection, not parsed prose or wholesale private-struct export.
Include Instance trust/completeness, diagnostic policy state, Revision catalog,
history discovery, metadata operations and bundle results in the coverage matrix.

S0 closes envelope/version/flag choices, typed results/errors, partial results,
absence and output ownership; no spelling such as `--format json` is frozen here.
Implement management/inspection and Plans, then execution/transport results and
raw/interactive coexistence, followed by schema and failure-path acceptance.

Completion requires usable automation without parsing human prose or conflicting
human/machine facts. Structured mode grants no extra authority, side effects or
automatic retry. Preserve raw Input export and Hook terminal streams. Unsupported
combinations fail explicitly before side effects. A few easy commands are not a
complete command-coverage gate; no HTTP/MCP service is added.

## E. Versioning and Baseline Consolidation

Retain the [original final-baseline scope](./product-completion-milestones.md#5-versioning-and-baseline-consolidation)
and [release-readiness policy](./release-readiness.md). Implement formal support
and evolution mechanisms, consolidate current schema/formats/interfaces, separate
history from active rules and handle development-only compatibility exactly under
the approved product policy. Final-contract conformance, not development origin,
decides data acceptance. Existing contracts remain binding until approved changes.

Close representations and removal/reorganization inventory before implementation,
then validate the formal baseline, support/refusal boundaries and all remaining
obligations. This is a final consistency check, not permission to leave A-D gaps
unowned until the end. It neither changes 0.1.0 to 1.0.0 automatically nor authorizes
publication. Internal actual-use evaluation and release-mechanism acceptance remain
separate pre-publication gates under release readiness.

## Shared obligation inventory and completion rules

A's S0 establishes one complete delivery inventory. The 2026-09-18 read-only audit
found 85 `Pending automated coverage` markers; this is a dated observation, not
85 missing functions or a fixed target count. Re-enumerate the current tree.
Classify each relevant numbered rule and unnumbered constraint, including partial
coverage such as the Revision command family, and stale deferred claims for
already-implemented capabilities. Do not bulk-change Pending to Passed.

| Classification | Required treatment |
| --- | --- |
| Implemented with sufficient existing evidence, linkage incomplete | Repair actual bidirectional traceability |
| Implemented but no proof of the user-visible result | Assign positive/negative acceptance and any necessary repair |
| Promised but not implemented | Assign an explicit A-E owner and slice |
| Intentionally excluded or conditional future scope | Keep the rationale and trigger explicit; do not pretend completion |
| Historical status incorrectly presented as current | Correct current navigation without rewriting the historical decision |

Every in-scope gap needs a concrete owner, deliverable and acceptance condition.
Do not replace old Pending labels with another unassigned later-work bucket or
invent test IDs for nonexistent evidence. Follow each milestone through:

```text
Product obligation -> user entry point -> observable result
-> positive/negative/failure tests -> source-qualified verification evidence
```

A repository method does not establish an available CLI; accepted protocol bytes
do not establish diagnostic usability; retained data does not establish discovery;
negative redaction tests do not establish a working positive use case. Preserve
requirement/test identities and follow the existing risk-based verification policy.
Use full configured-remote CI for the applicable runtime integration gates and
platform-specific checks; documentation-only planning uses documentation checks.

## Exclusions and authorization boundary

Full user, Pack Author and Hook usage guides/placeholders are excluded by the
operator's instruction. Necessary Spec, CLI help, implementation records and
acceptance material still accompany actual capability work; this exclusion does
not justify an undocumented command contract.

Do not pull OS sandbox/WASI/container isolation, encrypted Secret/Snapshot storage,
registry/signing/trust enforcement, broader external-resource taxonomy, LocalInstall
or metadata history, detailed Stack semantics, cross-Package adoption or a public
Candidate API into this plan merely because they are deferred. M8 Recipes remain
rejected. New protocol progress/event facilities are not implicitly approved.

This task records the approved grouping, relative order, scope and gates only.
Externally meaningful retention/disclosure, CLI and bundle decisions remain S0
work. No runtime work, new normative requirement, schema migration, deployment,
Git integration or formal release is authorized by writing this record.
