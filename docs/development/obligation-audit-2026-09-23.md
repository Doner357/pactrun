---
title: Delivery Obligation Audit (2026-09-23)
---

# Delivery obligation audit: 2026-09-23

**Status: Completed documentation inventory; outstanding product verification is
not closed. E design and runtime implementation remain unapproved.**

This audit records the operator-requested navigation correction and obligation
classification against local develop source commit
`f81c308` on 2026-09-23. It supplements the
[2026-09-19 inventory](./shared-obligation-inventory.md), whose original rows are a
historical snapshot, not current delivery status. The
[remaining capability plan](./remaining-capability-milestones.md) still owns work
order and [Spec](../spec/index.md) owns requirements. These review packages are
engineering follow-ups, not newly approved E slices or new product requirements.
No Spec verification marker, runtime code, frozen representation, product version,
release workflow, or usage guide is changed by this audit.

## Census and method

**Pre-closure snapshot:** The census, classifications and verification results
below describe the original documentation-only audit. The later authorized
[verification closure](./verification-gap-closure.md) records current evidence
repairs, new tests and remaining counts; it supersedes this page for follow-up
status, not for the historical observations or exclusions.

- Enumerated all 370 numbered requirement definitions in `docs/spec/**/*.md`.
- 76 blocks in 17 files contain the literal `Pending automated coverage`:
  75 standalone Pending declarations and one already-cited, scope-qualified
  historical paragraph in PR-REQ-0218. A match is not a missing feature.
- The remaining 294 blocks comprise 291 with cited test IDs and three retired
  requirements (PR-REQ-0137, PR-REQ-0139, PR-REQ-0140). Cited does not mean that this
  audit reran or established complete acceptance for every clause.
- The original inventory lists 353 numbered rules; the 17 later rules are recorded
  below. No requirement or test ID is invented, renumbered or retired here.
- Compared Pending clauses with current subsystem entry points, actual
  `Test-ID` / `Verifies` declarations, selected assertions, and implementation
  closeouts. All evidence IDs below resolve to source declarations. Except for
  PR-REQ-0218's five existing Rust/Node verifier tests, the Pending umbrella IDs lack direct
  reverse `Verifies` links: adjacent detailed-rule tests are candidates, not
  already-complete bidirectional traceability.
- Scanned non-code prose outside numbered sections for uppercase normative
  keywords, finding 12 section groups; these and additional boundaries are
  assigned below. This is a navigation aid, not a claim that keyword scanning
  proves all semantic completeness. Embedded DDL, canonical bytes, schemas and
  fixtures remain owned by their numbered contracts and were not modified.

The earlier observations of 85 markers on 2026-09-18 and 86 Pending-numbered
blocks in the initial 2026-09-19 inventory remain dated observations. Counts must
not be compared as a feature-completion metric.

## Classification and closure rules

| Class | Count | Meaning and required follow-up |
| --- | --- | --- |
| L - Linkage | 12 | Existing implementation and concrete assertions support the core behavior; repair the umbrella rule/test relationship only after checking every clause and both directions. No new passing run is claimed. |
| P - Partial proof | 56 | Implementation or neighboring tests exist, but the complete user-visible, negative, lifecycle or architectural obligation is not established. Deliver the row's named acceptance or review; repair runtime only if a demonstrated gap requires it. |
| N - Not implemented | 5 | Formal product-version/baseline mechanisms remain unimplemented. E S0 must settle and approve representations before runtime work. |
| X - Conditional exclusion | 1 | Future enforced isolation is conditional, not an approved missing runtime feature. Retain the trigger and refusal boundary. |
| H - Historical scope | 2 | Old Pending language describes a former verifier/writer scope. Preserve history and distinguish present evidence instead of claiming the old scope is today's unsupported feature set. |

A row is closed only when its owning requirement has a clause-level evidence map,
real reverse test declarations where mechanically verifiable, the appropriate
positive/negative/failure assertions, and source-qualified verification. An L row
is not permission to bulk-replace Pending with Passed. A P row is not a finding
that its implementation is broken. Author intent and architecture-only claims
need explicit review evidence, not a fabricated runtime test. Newly discovered
observable ambiguity returns to the owning design gate, not a guessed test oracle.

## Follow-up owners and deliverables

Owners below are subsystem responsibilities, not assignments to named people.
Each row specifies an acceptance condition in addition to this shared deliverable.
No all-purpose unassigned “E later” bucket replaces the old inventory.

| Owner / review package | Concrete deliverable and entry points |
| --- | --- |
| Architecture | Composition/dependency and operation-boundary review; `src/lib.rs`, `src/domain.rs`, Application, Workflow and Executor. Preserve Pack-only, local command execution. |
| Identity | Core/source/installed-Revision identity matrix and direct traceability; authoring, Core codecs and Revision persistence. |
| Inputs | Binding/state-token/context/disclosure matrix through Instance/Input CLI, persistence and actual Hook Sessions. |
| Execution | Resolution-to-plan-to-Run matrix for every operation, parameter lifetime and Workspace publication boundaries. |
| Migration | Source normalization, path selection, per-edge publication and declassification acceptance via actual migration commands. |
| Recovery | Risk ACK/guard/recovery-operation acceptance plus explicit security-boundary review. |
| Content and transport | Independent-format dispatch and exact immutable-content/metadata round trips through install/import/export. |
| CLI | Complete user-entry-point and pre-side-effect refusal matrix, including ambiguous selectors and purpose-specific authorization. |
| Formal baseline | E S0 decision package for support/evolution and consolidation; implementation/tests follow separate approval. Publication remains separate. |
| Historical scope | Current-versus-historical navigation corrections tied to actual closeouts; no format removal or reinterpretation. |

Prefer closing L evidence mappings and resolving P proof questions before making
E design assumptions. N decisions stay in E S0. An A-D regression found by a P
check belongs to that capability's existing contract, not permission to defer the
feature until release. These priorities do not authorize code changes in this task.

## Pending-block disposition

Test references here are evidence candidates inspected in source, not new
`Verification` declarations. Source locations are indexed below. Every current
Pending-containing numbered block appears exactly once in this table.

| Requirement and owning contract | Class | Owner | Existing evidence candidates | Concrete remaining acceptance / disposition |
| --- | --- | --- | --- | --- |
| [PR-REQ-0001 - Product boundary](../spec/foundations/system-model.md#pr-req-0001---product-boundary) | P | Architecture | PR-TEST-0142, PR-TEST-0143 | One-shot executable works without a daemon; add an architecture/command-surface review excluding scheduling, orchestration, web and vault responsibilities. Runtime smoke tests alone cannot prove the negative product boundary. |
| [PR-REQ-0002 - Package taxonomy](../spec/foundations/system-model.md#pr-req-0002---package-taxonomy) | P | Architecture | PR-TEST-0068, PR-TEST-0069 | Closed Pack parsing exists; explicitly verify Stack-like/unknown package-kind input cannot select another model or create storage. Keep Stack outside scope. |
| [PR-REQ-0003 - Source-to-Instance boundary](../spec/foundations/system-model.md#pr-req-0003---source-to-instance-boundary) | P | Architecture | PR-TEST-0072, PR-TEST-0143 | Source validation and installed-content execution exist; connect every admission path to an installed Revision and add rejection of an uninstalled Candidate if not already asserted. |
| [PR-REQ-0006 - Distinct operation concepts](../spec/foundations/system-model.md#pr-req-0006---distinct-operation-concepts) | P | Architecture | PR-TEST-0069, PR-TEST-0585 | Separate authoring fields and real operation-specific execution exist; verify all five capabilities remain distinct through plan, Run discriminator and completion, including negative cross-operation completion. |
| [PR-REQ-0007 - Managed execution pipeline](../spec/foundations/system-model.md#pr-req-0007---managed-execution-pipeline) | P | Architecture | PR-TEST-0089, PR-TEST-0302, PR-TEST-0397 | Acceptance-before-admission is exercised for Action, Migration and retirement; complete the same ordering/no-Run management matrix for Capture and Restore and map the whole pipeline. |
| [PR-REQ-0008 - Compiler and Executor ownership](../spec/foundations/system-model.md#pr-req-0008---compiler-and-executor-ownership) | P | Architecture | PR-TEST-0090, PR-TEST-0115 | Compiler facts and admission revalidation exist; review all compiler/executor entry points for authoring or CLI re-interpretation and cover a changed-source-after-plan counterexample. |
| [PR-REQ-0009 - Modular-monolith dependency direction](../spec/foundations/system-model.md#pr-req-0009---modular-monolith-dependency-direction) | P | Architecture | No direct test identified | Inspect composition in src/lib.rs, src/domain.rs and src/workflow.rs; deliver a dependency/ownership review of all domain modules plus a bounded regression check for forbidden adapter imports. No architecture-wide automated proof was identified. |
| [PR-REQ-0010 - Policy and mechanism separation](../spec/foundations/system-model.md#pr-req-0010---policy-and-mechanism-separation) | P | Architecture | PR-TEST-0068, PR-TEST-0041 | Schema-directed parsing and explicit projection own important policy; review identity, lifecycle and compatibility library boundaries rather than inferring architectural compliance from these two tests. |
| [PR-REQ-0011 - Stable Package lineage](../spec/foundations/identity-and-state.md#pr-req-0011---stable-package-lineage) | P | Identity | PR-TEST-0069, PR-TEST-0053, PR-TEST-0538 | Explicit PackageId and transport preservation exist; complete a single lineage matrix for source location, content, publisher/presentation changes and explicit fork re-identification. |
| [PR-REQ-0012 - Operational content digest](../spec/foundations/identity-and-state.md#pr-req-0012---operational-content-digest) | P | Identity | PR-TEST-0044, PR-TEST-0067, PR-TEST-0346 | Digest/metadata and live-state separation have evidence; enumerate every excluded metadata field and every supported Core version in the digest-change/non-change matrix. |
| [PR-REQ-0013 - Exact Revision identity](../spec/foundations/identity-and-state.md#pr-req-0013---exact-revision-identity) | L | Identity | PR-TEST-0053, PR-TEST-0054 | Connect exact pair identity to existing same-digest/different-Package and immutable/idempotent storage assertions; do not substitute digest-only identity. |
| [PR-REQ-0014 - RevisionCore projection](../spec/foundations/identity-and-state.md#pr-req-0014---revisioncore-projection) | P | Identity | PR-TEST-0040, PR-TEST-0041, PR-TEST-0331 | Explicit canonical projection exists; map all invocation-affecting fields in each supported Core format and their metadata exclusions, retaining V1's closed schema. |
| [PR-REQ-0015 - Runtime content closure identity](../spec/foundations/identity-and-state.md#pr-req-0015---runtime-content-closure-identity) | P | Identity | PR-TEST-0044, PR-TEST-0051, PR-TEST-0069 | Logical closure and physical deduplication are separated; verify role/path/kind/executable changes versus source path/timestamp/ownership changes explicitly. |
| [PR-REQ-0016 - Semantic normalization](../spec/foundations/identity-and-state.md#pr-req-0016---semantic-normalization) | P | Identity | PR-TEST-0041, PR-TEST-0068 | Typed scalar normalization and Frozen vectors exist; cover duplicate semantic keys and ordered versus unordered collections across all current authoring versions. |
| [PR-REQ-0017 - RevisionCoreFormatV1 hash contract](../spec/foundations/identity-and-state.md#pr-req-0017---revisioncoreformatv1-hash-contract) | L | Identity | PR-TEST-0044, PR-TEST-0045 | Link the production V1 byte/frame/digest and invalid-vector assertions to this umbrella rule; retain the independent conformance oracle, not only a serializer round trip. |
| [PR-REQ-0018 - Stable published identity](../spec/foundations/identity-and-state.md#pr-req-0018---stable-published-identity) | P | Identity | PR-TEST-0044, PR-TEST-0045, PR-TEST-0339, PR-TEST-0494 | Frozen bytes and refusal paths exist; inventory every dispatch point and verify unsupported versions never fall back after an internal/schema change. |
| [PR-REQ-0023 - Stable Instance identity](../spec/foundations/identity-and-state.md#pr-req-0023---stable-instance-identity) | P | Identity | PR-TEST-0531, PR-TEST-0436, PR-TEST-0307, PR-TEST-0258 | Name reuse, Migration and Restore are exercised; add or identify one direct identity-stability/non-reuse assertion covering Input mutation as well as those operations. |
| [PR-REQ-0025 - Opaque InstanceStateVersion](../spec/foundations/identity-and-state.md#pr-req-0025---opaque-instancestateversion) | P | Inputs | PR-TEST-0076, PR-TEST-0259, PR-TEST-0301, PR-TEST-0330 | CAS/publication tests exist; explicitly prove ABA prevention after restoring earlier values and that service-owned byte edits do not affect the opaque token. |
| [PR-REQ-0026 - State-version publication](../spec/foundations/identity-and-state.md#pr-req-0026---state-version-publication) | P | Inputs | PR-TEST-0076, PR-TEST-0301, PR-TEST-0263, PR-TEST-0330 | Map every authoritative mutation to one atomic token change, including active Revision, Inputs, Restore, per-edge Migration, guard entry and manual resolution; test rollback leaves the old token. |
| [PR-REQ-0027 - Non-versioned observations](../spec/foundations/identity-and-state.md#pr-req-0027---non-versioned-observations) | P | Inputs | PR-TEST-0067, PR-TEST-0346, PR-TEST-0558 | Metadata, live storage and management output have evidence; complete the no-token-change matrix for Hook-only side effects, Snapshot objects, Runs and Artifacts. |
| [PR-REQ-0028 - Stable Input identity and opaque bytes](../spec/foundations/identity-and-state.md#pr-req-0028---stable-input-identity-and-opaque-bytes) | P | Inputs | PR-TEST-0075, PR-TEST-0147 | Opaque, empty and binary bytes have concrete assertions; stable semantic meaning is also an author obligation and must not be claimed as inferred or mechanically enforced from an ID string. |
| [PR-REQ-0029 - One managed binding registry](../spec/foundations/identity-and-state.md#pr-req-0029---one-managed-binding-registry) | L | Inputs | PR-TEST-0280, PR-TEST-0285 | Link the single-registry active/retained derivation and read-only observation assertions; reject treating roles as separate value histories. |
| [PR-REQ-0030 - Retained binding lifetime](../spec/foundations/identity-and-state.md#pr-req-0030---retained-binding-lifetime) | P | Inputs | PR-TEST-0280, PR-TEST-0312, PR-TEST-0437 | Retention/reactivation and retirement have evidence; close explicit discard/delete/remove/abandon lifetime cases without turning Revision provenance into binding identity. |
| [PR-REQ-0031 - RequiredInputsSatisfied](../spec/foundations/identity-and-state.md#pr-req-0031---requiredinputssatisfied) | P | Inputs | PR-TEST-0301, PR-TEST-0321, PR-TEST-0525 | Incomplete intermediate/target readiness is exercised; explicitly include required empty-as-present and incomplete creation without a new lifecycle state. |
| [PR-REQ-0032 - Initial binding acquisition and Instance publication](../spec/foundations/identity-and-state.md#pr-req-0032---initial-binding-acquisition-and-instance-publication) | P | Inputs | PR-TEST-0147, PR-TEST-0072 | Initial acquisition and rejection have partial evidence; identify or add multi-binding create rollback after one acquisition fails, proving no Instance or subset of requested bindings is published. |
| [PR-REQ-0035 - Management operations](../spec/execution/execution-and-concurrency.md#pr-req-0035---management-operations) | P | Execution | PR-TEST-0088, PR-TEST-0558, PR-TEST-0545 | Direct management paths exist; assert no Plan/Run for every listed family and token/guard behavior only for authoritative Instance mutations, including failed requests. |
| [PR-REQ-0038 - Resolver ownership](../spec/execution/execution-and-concurrency.md#pr-req-0038---resolver-ownership) | P | Execution | PR-TEST-0115, PR-TEST-0080, PR-TEST-0586 | Exact selection and primitive parameter parsing exist; complete resolver/compiler boundary coverage including ambiguity and explicit security intent before source acquisition or mutation. |
| [PR-REQ-0040 - Typed immutable plan](../spec/execution/execution-and-concurrency.md#pr-req-0040---typed-immutable-plan) | P | Execution | PR-TEST-0081, PR-TEST-0366 | Typed detached plans exist, including unnumbered compiler tests; assign real test IDs only when closing their traceability and verify immutability/sequence/no-live-byte snapshot claims across operation types. |
| [PR-REQ-0044 - Accepted execution continuity](../spec/execution/execution-and-concurrency.md#pr-req-0044---accepted-execution-continuity) | L | Execution | PR-TEST-0301, PR-TEST-0305, PR-TEST-0258 | Link multi-edge owner continuation and successful Restore assertions to accepted execution continuity; its own committed boundary must not invalidate the same Run. |
| [PR-REQ-0077 - Separate version domains](../spec/foundations/resources-and-versioning.md#pr-req-0077---separate-version-domains) | P | Content and transport | PR-TEST-0369, PR-TEST-0385, PR-TEST-0183, PR-TEST-0557 | Mixed Core/Hook versions and independent Snapshot/CLI checks exist; complete the current version-domain inventory without coupling product, persistence, source, bundle, integrity, Core, Hook or CLI numbers. |
| [PR-REQ-0079 - Revision Core format ownership](../spec/foundations/resources-and-versioning.md#pr-req-0079---revision-core-format-ownership) | P | Content and transport | PR-TEST-0044, PR-TEST-0331, PR-TEST-0493 | Concrete Core codecs/vectors exist; map schema, normalization, ordering, closure, byte framing, domain and hash algorithm for each supported version, not V1 alone. |
| [PR-REQ-0082 - Hook Protocol version](../spec/foundations/resources-and-versioning.md#pr-req-0082---hook-protocol-version) | P | Content and transport | PR-TEST-0025, PR-TEST-0323, PR-TEST-0362, PR-TEST-0491 | Wire conformance and real adapters exist; verify every supported protocol's establishment/cancel/EOF/authority behavior and preserve helper-as-adapter boundaries. |
| [PR-REQ-0084 - Revision import identity](../spec/foundations/resources-and-versioning.md#pr-req-0084---revision-import-identity) | P | Content and transport | PR-TEST-0538, PR-TEST-0539, PR-TEST-0540, PR-TEST-0544 | Revision bundle identity/metadata/idempotence exist; explicitly close same-digest/different-Package and changed-digest/same-Package transport cases. Remove obsolete future-bundle status only in a scoped follow-up. |
| [PR-REQ-0085 - Snapshot import identity](../spec/foundations/resources-and-versioning.md#pr-req-0085---snapshot-import-identity) | P | Content and transport | PR-TEST-0206, PR-TEST-0258 | V1/V2 Snapshot transport and Restore exist; map every provenance/binding/service-content field and same-ID/different-digest rejection at actual import, not only manifest parsing. |
| [PR-REQ-0086 - Exact resolution before operation](../spec/behavior/packages-revisions-and-instances.md#pr-req-0086---exact-resolution-before-operation) | P | CLI | PR-TEST-0062, PR-TEST-0066, PR-TEST-0532, PR-TEST-0586 | Exact selectors, deduplicated labels and stable order exist; trace all consuming operations and assert ambiguous resolution fails before acquisition/mutation with the owning actionable error. |
| [PR-REQ-0090 - Ordinary execution context](../spec/behavior/inputs-secrets-and-readiness.md#pr-req-0090---ordinary-execution-context) | P | Inputs | PR-TEST-0312, PR-TEST-0322, PR-TEST-0226 | Migration sees active/retained views and Capture pins its registry; verify actual Action/Capture Session context excludes retained bindings while preserving the full pinned recovery representation. |
| [PR-REQ-0093 - Secret deletion disclaimer](../spec/behavior/inputs-secrets-and-readiness.md#pr-req-0093---secret-deletion-disclaimer) | P | Inputs | PR-TEST-0076, PR-TEST-0147 | Managed references and export denial are exercised; review help/errors/docs for no secure-erasure claim and verify deletion removes only the authorized reference, not promised physical/WAL erasure. |
| [PR-REQ-0105 - Declared Migration path](../spec/behavior/snapshots-migrations-and-recovery.md#pr-req-0105---declared-migration-path) | L | Migration | PR-TEST-0277, PR-TEST-0278, PR-TEST-0290 | Link exact same-lineage path selection and missing/invalid-edge rejection; no newest Revision or undeclared shortcut may become a route. |
| [PR-REQ-0106 - Chained progress](../spec/behavior/snapshots-migrations-and-recovery.md#pr-req-0106---chained-progress) | L | Migration | PR-TEST-0303, PR-TEST-0304, PR-TEST-0317 | Link cancellation/crash evidence retaining the last committed intermediate Revision; later failure must not undo an earlier edge or replay a Hook. |
| [PR-REQ-0107 - Incomplete Migration result](../spec/behavior/snapshots-migrations-and-recovery.md#pr-req-0107---incomplete-migration-result) | P | Migration | PR-TEST-0301, PR-TEST-0321, PR-TEST-0307 | Incomplete intermediate continuation and successful incomplete final publication exist; close CLI/human/machine reporting as readiness rather than failure or manual recovery. |
| [PR-REQ-0108 - Declassification authorization](../spec/behavior/snapshots-migrations-and-recovery.md#pr-req-0108---declassification-authorization) | L | Migration | PR-TEST-0281, PR-TEST-0311 | Link compile-time and real CLI declaration-plus-authorization checks, including denied transition without publication and authorized target-only declassification. |
| [PR-REQ-0110 - Recovery options](../spec/behavior/snapshots-migrations-and-recovery.md#pr-req-0110---recovery-options) | P | Recovery | PR-TEST-0156, PR-TEST-0173, PR-TEST-0259, PR-TEST-0437 | Existing recovery paths cover override, resolution, Restore and abandonment; complete one guarded-Instance matrix for inspection/legal Input management and every denial/precondition. |
| [PR-REQ-0115 - Instance operations](../spec/behavior/command-and-output-reference.md#pr-req-0115---instance-operations) | P | CLI | PR-TEST-0078, PR-TEST-0307, PR-TEST-0436, PR-TEST-0437, PR-TEST-0525 | All named Instance families are available; replace the stale later-milestone navigation in a scoped follow-up and map incomplete creation, missing/trust inspection and explicit abandonment to public acceptance. |
| [PR-REQ-0116 - Input operations](../spec/behavior/command-and-output-reference.md#pr-req-0116---input-operations) | P | CLI | PR-TEST-0076, PR-TEST-0147, PR-TEST-0280 | Input CRUD/CAS/disclosure and retention exist; close the public active/retained/required/optional/sticky-floor matrix, including rejection of arbitrary protection-selection flags. |
| [PR-REQ-0119 - Security-sensitive authorization spelling](../spec/behavior/command-and-output-reference.md#pr-req-0119---security-sensitive-authorization-spelling) | P | CLI | PR-TEST-0078, PR-TEST-0311, PR-TEST-0173, PR-TEST-0437 | Purpose-specific export, declassification, override and abandon paths exist; test a generic yes/force option cannot authorize any of them, including sensitive Snapshot export. |
| [PR-REQ-0123 - Explicit normalized semantics](../spec/contracts/authoring-model.md#pr-req-0123---explicit-normalized-semantics) | P | Migration | PR-TEST-0041, PR-TEST-0280 | Normalized transitions and single-registry effects exist; identify direct source shorthand versus explicit Carry/Keep equivalence and prove installed execution does not reapply source defaults. |
| [PR-REQ-0124 - Separate authoring capabilities](../spec/contracts/authoring-model.md#pr-req-0124---separate-authoring-capabilities) | P | Migration | PR-TEST-0069, PR-TEST-0336 | Separate capability projections exist; add or identify closed-schema rejection of catch-all operation authoring without removing shared private execution primitives. |
| [PR-REQ-0125 - Action semantics](../spec/contracts/actions-inputs-and-parameters.md#pr-req-0125---action-semantics) | P | Execution | PR-TEST-0069, PR-TEST-0148, PR-TEST-0168 | Named Actions carry typed parameters/access/I/O/output semantics; identify or add arbitrary Action-name tests proving no service behavior is inferred from familiar names. |
| [PR-REQ-0126 - Action is not Hook](../spec/contracts/actions-inputs-and-parameters.md#pr-req-0126---action-is-not-hook) | P | Execution | PR-TEST-0069, PR-TEST-0585 | Authoring and completion use distinct capability models; prove Snapshot/Migration/Cleanup cannot be smuggled in as generic Actions while allowing shared Hook machinery. |
| [PR-REQ-0127 - Stable InputIdentity](../spec/contracts/actions-inputs-and-parameters.md#pr-req-0127---stable-inputidentity) | P | Inputs | PR-TEST-0039, PR-TEST-0069 | Stable lexical identity is represented; document the author-owned non-reuse obligation separately from grammar validation and retain independent frontend versioning. |
| [PR-REQ-0128 - Input payload opacity](../spec/contracts/actions-inputs-and-parameters.md#pr-req-0128---input-payload-opacity) | L | Inputs | PR-TEST-0075, PR-TEST-0147, PR-TEST-0346 | Link empty/binary round-trip and live-storage separation assertions; payload filenames must not select parsing or implicit synchronization. |
| [PR-REQ-0129 - Required declaration meaning](../spec/contracts/actions-inputs-and-parameters.md#pr-req-0129---required-declaration-meaning) | P | Inputs | PR-TEST-0301, PR-TEST-0321, PR-TEST-0396 | Incomplete Migration and deletion prerequisites differ from ordinary execution; complete Action/Capture rejection and optional/missing/empty Input cases without making incomplete Instances illegal. |
| [PR-REQ-0130 - Active and retained contexts](../spec/contracts/actions-inputs-and-parameters.md#pr-req-0130---active-and-retained-contexts) | P | Inputs | PR-TEST-0312, PR-TEST-0322 | Actual Migration Session contexts exist; close Action active-only and Cleanup typed retained-context assertions rather than using a protocol fixture as execution proof. |
| [PR-REQ-0131 - Secret is protection metadata](../spec/contracts/actions-inputs-and-parameters.md#pr-req-0131---secret-is-protection-metadata) | P | Inputs | PR-TEST-0076, PR-TEST-0147, PR-TEST-0346 | Sticky protection and byte opacity exist; pair automated disclosure checks with review of no vault/secure-erasure promises, including Hook-facing documentation. |
| [PR-REQ-0132 - No implicit declassification](../spec/contracts/actions-inputs-and-parameters.md#pr-req-0132---no-implicit-declassification) | L | Inputs | PR-TEST-0281, PR-TEST-0311, PR-TEST-0313 | Link declared-plus-authorized declassification and invalid Hook completion assertions; neither an author default nor a Hook-selected protection flag may bypass the floor. |
| [PR-REQ-0133 - Invocation-parameter lifecycle](../spec/contracts/actions-inputs-and-parameters.md#pr-req-0133---invocation-parameter-lifecycle) | P | Execution | PR-TEST-0080, PR-TEST-0150, PR-TEST-0167, PR-TEST-0175 | Parsing and real Hook delivery/protection exist; add or identify an explicit post-invocation assertion that parameters did not create persistent Input bindings or Managed Data. |
| [PR-REQ-0134 - Initial parameter types](../spec/contracts/actions-inputs-and-parameters.md#pr-req-0134---initial-parameter-types) | P | Execution | PR-TEST-0080, PR-TEST-0068 | Four typed primitive parsers and sensitive values exist; close public CLI/authoring rejection of raw argument-vector passthrough rather than relying on type declarations alone. |
| [PR-REQ-0136 - Shared machinery is not shared identity](../spec/contracts/actions-inputs-and-parameters.md#pr-req-0136---shared-machinery-is-not-shared-identity) | P | Execution | PR-TEST-0069, PR-TEST-0585 | Distinct Action/Capture/Restore completions are exercised; map shared parameter binding to operation-specific identities and reject cross-operation parameters/completion authority. |
| [PR-REQ-0138 - Revision is the reproducibility boundary](../spec/contracts/recipes-and-runtime-content.md#pr-req-0138---revision-is-the-reproducibility-boundary) | L | Content and transport | PR-TEST-0044, PR-TEST-0051 | Link canonical Core plus closure digest assertions; do not turn source repeatability into a product guarantee. |
| [PR-REQ-0141 - Pactrun-owned materialization](../spec/contracts/recipes-and-runtime-content.md#pr-req-0141---pactrun-owned-materialization) | L | Content and transport | PR-TEST-0143, PR-TEST-0346 | Link real execution after source removal and distinct service-owned live storage; installed runtime must not depend on mutable source bytes. |
| [PR-REQ-0142 - Logical content roles](../spec/contracts/recipes-and-runtime-content.md#pr-req-0142---logical-content-roles) | P | Content and transport | PR-TEST-0069, PR-TEST-0051, PR-TEST-0331 | Logical roles and independent source paths exist; prove identical blobs in different roles change identity where specified, and mark the already-implemented Core V2 resource declaration as current without altering V1. |
| [PR-REQ-0143 - Workspace](../spec/contracts/snapshots-and-managed-data.md#pr-req-0143---workspace) | P | Execution | PR-TEST-0313, PR-TEST-0263, PR-TEST-0346 | Rejected completion and rollback protect publication; close Workspace lifetime/cleanup and no-ServiceStorage-substitution proof at real execution boundaries. |
| [PR-REQ-0159 - Declarative transition scope](../spec/contracts/migrations.md#pr-req-0159---declarative-transition-scope) | P | Migration | PR-TEST-0280, PR-TEST-0312, PR-TEST-0334 | Declarative continuity and typed Hook transforms exist; explicitly reject split/merge/arbitrary Input payload transformation in declarative syntax while retaining independently approved service transformations. |
| [PR-REQ-0166 - Package Migration responsibilities](../spec/contracts/migrations.md#pr-req-0166---package-migration-responsibilities) | P | Migration | PR-TEST-0312, PR-TEST-0371, PR-TEST-0373 | Opaque Input and service transitions exist; prove declarative paths do not launch optional Hooks or parse/copy service content implicitly. Service schema correctness remains Package responsibility. |
| [PR-REQ-0170 - Session authority is not host isolation](../spec/contracts/hooks-recovery-and-cleanup.md#pr-req-0170---session-authority-is-not-host-isolation) | P | Recovery | PR-TEST-0328 | Trusted Hook writes outside Pactrun are concretely not rolled back; audit public wording to distinguish mediated authority from OS confinement, including another CLI invocation outside the Session. |
| [PR-REQ-0171 - Future isolation must be enforced](../spec/contracts/hooks-recovery-and-cleanup.md#pr-req-0171---future-isolation-must-be-enforced) | X | Recovery | No direct test identified | Conditional future isolation only. No sandbox backend is authorized; reactivate only with approved identity-bearing host requirements, operator policy and enforceable fail-closed backend design. |
| [PR-REQ-0173 - Risk-entry request](../spec/contracts/hooks-recovery-and-cleanup.md#pr-req-0173---risk-entry-request) | L | Recovery | PR-TEST-0096, PR-TEST-0369, PR-TEST-0328 | Link durable risk-ACK ordering, persistence-failure continuation and real post-ACK side effects; no successful ACK may precede durable risk publication. |
| [PR-REQ-0174 - Risk-resolution request](../spec/contracts/hooks-recovery-and-cleanup.md#pr-req-0174---risk-resolution-request) | P | Recovery | PR-TEST-0085, PR-TEST-0369 | Durable risk resolution exists; explicitly demonstrate resolved risk with a failed operation and unchanged external effects, rather than equating resolution with success or rollback. |
| [PR-REQ-0175 - No nested risk taxonomy](../spec/contracts/hooks-recovery-and-cleanup.md#pr-req-0175---no-nested-risk-taxonomy) | P | Recovery | PR-TEST-0104, PR-TEST-0028, PR-TEST-0030 | Closed Clear/Open state and invalid completion ordering exist; explicitly reject nested risk/severity syntax and duplicate entry without durable mutation before claiming the entire no-nested-taxonomy rule. |
| [PR-REQ-0218 - Authority is not isolation and verification is layered](../spec/contracts/hook-protocol-v1.md#pr-req-0218---authority-is-not-isolation-and-verification-is-layered) | H | Historical scope | PR-TEST-0025, PR-TEST-0026, PR-TEST-0029, PR-TEST-0031, PR-TEST-0032, PR-TEST-0258, PR-TEST-0312, PR-TEST-0437 | Its Pending sentence describes the old wire-verifier boundary, not absent M3-M7 runtime. Preserve that limited claim and link subsequent runtime records; retain the no-host-isolation boundary and audit unproved subclaims separately. |
| [PR-REQ-0308 - V6 Migration persistence and upgrade boundary](../spec/persistence/persistence-schema-v6.md#pr-req-0308---v6-migration-persistence-and-upgrade-boundary) | H | Historical scope | PR-TEST-0294, PR-TEST-0295, PR-TEST-0296, PR-TEST-0297, PR-TEST-0298, PR-TEST-0304, PR-TEST-0327 | V5-to-V6 fixture and Migration recovery evidence exist, but today's writer is V11. Separate historical V6 guarantees from current admission/upgrade contracts; do not claim ordinary V11 bootstrap writes V6 or retire compatibility without E approval. |
| [PR-REQ-0329 - Product Major compatibility boundary](../spec/foundations/product-versioning-and-compatibility.md#pr-req-0329---product-major-compatibility-boundary) | N | Formal baseline | No direct test identified | E S0 must approve concrete compatibility representations; later implement same-Major old-Pack execution/default-preservation and unsupported-new-capability matrices, not merely SemVer parsing. |
| [PR-REQ-0330 - Internal evolution preserves supported external meaning](../spec/foundations/product-versioning-and-compatibility.md#pr-req-0330---internal-evolution-preserves-supported-external-meaning) | N | Formal baseline | No direct test identified | E S0 must select the formal support/upgrade matrix; later prove already-persisted identity, references and unresolved obligations survive supported evolution. Existing development schema upgrades are not a formal-release mechanism. |
| [PR-REQ-0331 - Explicit support selection and actionable refusal](../spec/foundations/product-versioning-and-compatibility.md#pr-req-0331---explicit-support-selection-and-actionable-refusal) | N | Formal baseline | No direct test identified | E S0 must settle requirement/capability syntax and unsupported-reader behavior; later test actionable pre-side-effect refusal, no fallback and no minimum version inferred from authoring-tool provenance. |
| [PR-REQ-0332 - First formal baseline and origin-independent acceptance](../spec/foundations/product-versioning-and-compatibility.md#pr-req-0332---first-formal-baseline-and-origin-independent-acceptance) | N | Formal baseline | No direct test identified | E S0 must inventory retained versus development-only contracts/codecs/migrations; later accept all conforming data regardless of origin and reject nonconforming data without repair, overwrite, deletion or a provenance barrier. |
| [PR-REQ-0333 - Development and formal-release baseline separation](../spec/foundations/product-versioning-and-compatibility.md#pr-req-0333---development-and-formal-release-baseline-separation) | N | Formal baseline | No direct test identified | Retain 0.1.0 now; E S0 must specify exact-build evidence and actual-version checks. A separately authorized 1.0.0 release candidate requires real artifact verification without an identity reset or runtime bypass. |

## Later numbered rules omitted by the historical inventory

These 17 rules already have owning Spec declarations and actual reverse test
links. Retain their current integrated status and detailed closeout evidence;
there is no new implementation backlog merely because the old inventory omitted
them. Formal-baseline conformance remains a later E gate. The tests listed here
are copied from current declarations, not evidence of a fresh run in this task.

| Owning rule | Current evidence |
| --- | --- |
| [PR-REQ-0354 - Pack containers and exact Revision transport](../spec/contracts/pack-distribution-v1.md#pr-req-0354---pack-containers-and-exact-revision-transport) | PR-TEST-0538, PR-TEST-0540, PR-TEST-0542, PR-TEST-0544, PR-TEST-0545, PR-TEST-0548 |
| [PR-REQ-0355 - Pack metadata conflict policy](../spec/contracts/pack-distribution-v1.md#pr-req-0355---pack-metadata-conflict-policy) | PR-TEST-0539, PR-TEST-0543, PR-TEST-0545, PR-TEST-0546 |
| [PR-REQ-0356 - Revision export and publication safety](../spec/contracts/pack-distribution-v1.md#pr-req-0356---revision-export-and-publication-safety) | PR-TEST-0538, PR-TEST-0541, PR-TEST-0543, PR-TEST-0544, PR-TEST-0545, PR-TEST-0546, PR-TEST-0547, PR-TEST-0548, PR-TEST-0549 |
| [PR-REQ-0357 - Specialized envelope export filenames](../spec/behavior/command-and-output-reference.md#pr-req-0357---specialized-envelope-export-filenames) | PR-TEST-0550, PR-TEST-0551, PR-TEST-0552 |
| [PR-REQ-0358 - Explicit CLI presentation selection](../spec/contracts/cli-json-v1.md#pr-req-0358---explicit-cli-presentation-selection) | PR-TEST-0553, PR-TEST-0557, PR-TEST-0558, PR-TEST-0561, PR-TEST-0566 |
| [PR-REQ-0359 - Versioned typed CLI response](../spec/contracts/cli-json-v1.md#pr-req-0359---versioned-typed-cli-response) | PR-TEST-0554, PR-TEST-0556, PR-TEST-0557, PR-TEST-0558, PR-TEST-0559, PR-TEST-0560, PR-TEST-0562, PR-TEST-0563, PR-TEST-0564, PR-TEST-0565, PR-TEST-0566, PR-TEST-0567, PR-TEST-0568, PR-TEST-0569 |
| [PR-REQ-0360 - Payload and terminal preservation](../spec/contracts/cli-json-v1.md#pr-req-0360---payload-and-terminal-preservation) | PR-TEST-0555, PR-TEST-0558, PR-TEST-0559, PR-TEST-0560, PR-TEST-0561, PR-TEST-0562, PR-TEST-0563, PR-TEST-0564, PR-TEST-0565, PR-TEST-0566, PR-TEST-0567, PR-TEST-0568, PR-TEST-0569 |
| [PR-REQ-0361 - Owner-private IPC initialization](../spec/contracts/hooks-recovery-and-cleanup.md#pr-req-0361---owner-private-ipc-initialization) | PR-TEST-0570, PR-TEST-0572, PR-TEST-0574, PR-TEST-0577 |
| [PR-REQ-0362 - Built-in Loader startup attribution](../spec/contracts/hooks-recovery-and-cleanup.md#pr-req-0362---built-in-loader-startup-attribution) | PR-TEST-0571, PR-TEST-0572, PR-TEST-0574, PR-TEST-0576 |
| [PR-REQ-0363 - Capability presentation at inspection boundaries](../spec/contracts/cli-json-v1.md#pr-req-0363---capability-presentation-at-inspection-boundaries) | PR-TEST-0573, PR-TEST-0575 |
| [PR-REQ-0364 - Task-oriented human presentation](../spec/contracts/cli-json-v1.md#pr-req-0364---task-oriented-human-presentation) | PR-TEST-0578 |
| [PR-REQ-0365 - Complete public machine projections](../spec/contracts/cli-json-v1.md#pr-req-0365---complete-public-machine-projections) | PR-TEST-0579 |
| [PR-REQ-0366 - Noninteractive machine delivery](../spec/contracts/cli-json-v1.md#pr-req-0366---noninteractive-machine-delivery) | PR-TEST-0580, PR-TEST-0581, PR-TEST-0583, PR-TEST-0585 |
| [PR-REQ-0367 - CLI event stream V1](../spec/contracts/cli-json-v1.md#pr-req-0367---cli-event-stream-v1) | PR-TEST-0580, PR-TEST-0581, PR-TEST-0582, PR-TEST-0585 |
| [PR-REQ-0368 - Explicit output-close cancellation](../spec/contracts/cli-json-v1.md#pr-req-0368---explicit-output-close-cancellation) | PR-TEST-0581, PR-TEST-0582, PR-TEST-0584 |
| [PR-REQ-0369 - Unique object prefixes](../spec/contracts/cli-id-selectors.md#pr-req-0369---unique-object-prefixes) | PR-TEST-0586, PR-TEST-0587, PR-TEST-0588, PR-TEST-0589 |
| [PR-REQ-0370 - Usable human abbreviations](../spec/contracts/cli-id-selectors.md#pr-req-0370---usable-human-abbreviations) | PR-TEST-0587, PR-TEST-0589 |

The 274 other cited-only rules already appear in the historical inventory;
current Spec declarations and their source tests supersede the snapshot's cited-ID
column. This audit checks their reference integrity, not every assertion or runtime
path. A-D completion evidence remains in
[A](./execution-diagnostics-observability-status.md),
[B](./object-catalog-history-metadata-status.md),
[C](./pack-transport-status.md), [D](./cli-presentation-status.md),
[CLI refinement](./cli-interface-refinement-status.md),
[IPC](./ipc-and-capability-presentation-status.md), and
[short selectors](./short-id-selectors-status.md). Together with the 17 later
rules, these account for all 291 cited-only blocks.

## Unnumbered constraints and bounded exclusions

### Additional cited-rule linkage gap

The full reference-integrity scan also finds one gap outside the 76 Pending
blocks: [PR-REQ-0091](../spec/behavior/inputs-secrets-and-readiness.md#pr-req-0091---readiness-admission)
is named by PR-TEST-0090 in `src/application.rs`, but its owning verification
declaration does not cite that test. **Inputs / Execution owns this L follow-up:**
check the readiness-revalidation assertions against the umbrella clause and add
the missing forward link with scoped verification. The rule's other existing
citations remain intact; this is not evidence of a missing readiness runtime.
This extra linkage item is separate from the 12 L rows in the Pending table.

All current forward verification citations resolve to tests with corresponding
reverse declarations. PR-REQ-0004 uses prose after `**Verification:**` rather
than the usual all-bold declaration; it already cites PR-TEST-0143 and is not a
missing link. Historical prose about a test's former scope is not counted as a
new verification declaration.

### Unnumbered section inventory

The following rows cover all 12 keyword-bearing section groups found outside
numbered rules. Section-level summaries do not replace the complete owning text.
Code/schema/DDL details and prose within numbered rules stay with those rules.

| Owning section | Owner and current evidence | Deliverable / acceptance |
| --- | --- | --- |
| [Remaining open spelling](../spec/behavior/command-and-output-reference.md#remaining-open-spelling) | CLI / Historical scope; PR-TEST-0307, PR-TEST-0437, PR-TEST-0545 | Replace obsolete future-command navigation only after linking the now-integrated Migration, deletion, recovery and transport contracts; keep closed parsing and no implicit scope expansion. |
| [Persistent and execution data scopes](../spec/behavior/inputs-secrets-and-readiness.md#persistent-and-execution-data-scopes) | Inputs; PR-TEST-0346 | Verify visible service files never acquire an implicit Input copy or synchronization authority. |
| [ServiceStorage is not an Input](../spec/contracts/actions-inputs-and-parameters.md#servicestorage-backed-service-state-is-not-an-input) | Inputs / Historical scope; PR-TEST-0346 | Preserve service-owned in-place editing and explicit acquisition; correct “future” status against implemented Core V2 without inventing an external-resource taxonomy. |
| [Hook wire verification scope](../spec/contracts/hook-protocol-v1.md#reading-map-informative) | Recovery / Historical scope; PR-TEST-0025, PR-TEST-0258, PR-TEST-0312 | Keep conformance fixtures distinct from production filesystem/commit evidence; link later actual-runtime proof rather than broadening the old verifier claim. |
| [Candidate evolution and exclusions](../spec/contracts/pack-source-yaml-v1.md#candidate-evolution-and-exclusions) | Identity; PR-TEST-0068, PR-TEST-0494 | Follow exact selected source versions; refuse unknown versions and do not introduce implicit fallback or a public Candidate API. |
| [Presentation and provenance](../spec/contracts/recipes-and-runtime-content.md#presentation-and-provenance) | Content and transport; PR-TEST-0062, PR-TEST-0544 | Preserve typed claims without inferring trust or authority; prove portable metadata handling separately from Revision identity. |
| [Shared recovery and ownership](../spec/execution/m4-snapshot-lifecycle-approval-baseline.md#shared-recovery-and-ownership-boundary) | Recovery; PR-TEST-0325, PR-TEST-0326 | Confirm loss before terminalization; unknown/live/corrupt states cannot authorize replay, output adoption or rollback. Preserve committed outcomes. |
| [Product-design constraints](../spec/foundations/system-model.md#normative-product-design-constraints) | Architecture / CLI; PR-TEST-0068, PR-TEST-0578 | Review minimal Pack burden, predictable authoring, actionable diagnostics and no transaction-bookkeeping leakage. Include security wording review; individual tests do not prove all usability claims. |
| [Data ownership](../spec/foundations/system-model.md#persistent-and-execution-data-ownership) | Inputs / Execution; PR-TEST-0346, PR-TEST-0371 | Verify detached Inputs, live service storage, Workspace, Artifacts and Snapshots retain distinct lifetime/authority; do not claim state-token linearization of live bytes. |
| [Required repository ordering](../spec/persistence/persistence-schema-v2.md#required-repository-ordering) | Identity; PR-TEST-0066 | Audit SQL ordering and merged metadata ranks against typed order, including filtered leading columns; current persistence must preserve the same meaning. |
| [Typed metadata interface](../spec/persistence/persistence-schema-v2.md#planned-crate-private-typed-interface) | Identity; PR-TEST-0064, PR-TEST-0065, PR-TEST-0067 | Verify replacement targets exist and contradictory batches/CAS conflicts publish nothing; distinguish historical proposed interface spelling from current private implementation. |
| [Snapshot loading and mutation invariants](../spec/persistence/persistence-schema-v5.md#logical-loading-and-mutation-invariants) | Execution / Historical scope; PR-TEST-0225, PR-TEST-0229, PR-TEST-0259, PR-TEST-0263 | Trace operation discriminants, consequence counters, Restore tokens, Capture result atomicity and exact payload closure into V11; retain no-byte-copy Run inspection and corruption refusal. Do not apply old V5-only ranks to later operations. |

Additional explicit scope boundaries:

| Boundary | Owner, trigger and treatment |
| --- | --- |
| Retired PR-REQ-0137/0139/0140 | Architecture / Historical scope: keep retirement and M8 rejection; do not mark them Pending or reintroduce install-time Recipes. Public Candidate API needs concrete demand and separate approval. |
| OS sandbox/WASI/container isolation | Recovery: PR-REQ-0171 remains conditional. Trigger is an approved enforced-isolation design; mediated Session authority alone is not enforcement. |
| Encrypted Secret/Snapshot storage, registry/signing/trust enforcement | Formal baseline scope review: excluded by the remaining plan; trigger is separately approved product scope. Current local trust remains descriptive, not enforcement. |
| Broader external resources, LocalInstall/metadata history, Stack semantics, cross-Package adoption | Architecture / Identity: explicitly excluded by the remaining plan; none is silently owned by E implementation. |
| Full user/Pack/Hook usage guides | Documentation: deferred to an explicitly scoped task. Required Spec/help/acceptance maintenance is not excluded. |
| Pages workflows, formal release and publication | Release owner: separate authorization. Existing untracked workflow/archive files are untouched; this audit does not activate internal trials, change version or publish anything. |

## Historical wording requiring a scoped follow-up

Current entry navigation is corrected by this task. The following Spec text is
recorded, not rewritten or implicitly retired: PR-REQ-0115's “later milestone”
commands, PR-REQ-0084's “future bundle” text, PR-REQ-0142 and PR-REQ-0040's
future-version service references, PR-REQ-0218's original wire-only coverage and
PR-REQ-0308's V6-writer scope. Use the current owning contracts and closeout records
when correcting these summaries. Preserve old format guarantees and distinguish
true unsupported semantics from stale scheduling language. Any semantic ambiguity
requires the normal owning-contract decision rather than a cosmetic rewrite.

## Evidence source index

Paths are repository-relative source locations, not links to generated website
content. IDs are existing declarations, not newly assigned tests. Candidate tests
may own more specific requirements than the umbrella rows above.

| Source | Referenced tests |
| --- | --- |
| `src/application.rs` | PR-TEST-0088, PR-TEST-0089, PR-TEST-0090, PR-TEST-0115 |
| `src/application/installation.rs` | PR-TEST-0072 |
| `src/authoring_v2.rs` | PR-TEST-0336, PR-TEST-0494 |
| `src/authoring.rs` | PR-TEST-0068, PR-TEST-0069 |
| `src/cli.rs` | PR-TEST-0078 |
| `src/cli/capability_presentation.rs` | PR-TEST-0573 |
| `src/cli/catalog_tests.rs` | PR-TEST-0531, PR-TEST-0532 |
| `src/cli/execution_presentation.rs` | PR-TEST-0567, PR-TEST-0572 |
| `src/cli/export_paths_tests.rs` | PR-TEST-0550, PR-TEST-0551, PR-TEST-0552 |
| `src/cli/json_tests.rs` | PR-TEST-0557, PR-TEST-0558, PR-TEST-0559, PR-TEST-0560, PR-TEST-0561, PR-TEST-0562, PR-TEST-0564, PR-TEST-0566, PR-TEST-0569 |
| `src/cli/machine_delivery_tests.rs` | PR-TEST-0578, PR-TEST-0579, PR-TEST-0580, PR-TEST-0581, PR-TEST-0582, PR-TEST-0584 |
| `src/cli/presentation.rs` | PR-TEST-0553, PR-TEST-0554, PR-TEST-0555, PR-TEST-0568 |
| `src/cli/retirement_tests.rs` | PR-TEST-0563 |
| `src/cli/schema_tests.rs` | PR-TEST-0556 |
| `src/cli/short_id_tests.rs` | PR-TEST-0586, PR-TEST-0587, PR-TEST-0589 |
| `src/domain/execution.rs` | PR-TEST-0080 |
| `src/domain/identity.rs` | PR-TEST-0039 |
| `src/domain/migration_tests.rs` | PR-TEST-0277, PR-TEST-0278, PR-TEST-0280, PR-TEST-0281 |
| `src/domain/revision_metadata.rs` | PR-TEST-0065 |
| `src/domain/run.rs` | PR-TEST-0104 |
| `src/domain/service_migration_tests.rs` | PR-TEST-0371, PR-TEST-0373 |
| `src/hook/delivery.rs` | PR-TEST-0583 |
| `src/hook/diagnostic_tests.rs` | PR-TEST-0525 |
| `src/hook/migration_protocol_tests.rs` | PR-TEST-0323 |
| `src/hook/migration_tests.rs` | PR-TEST-0312, PR-TEST-0313, PR-TEST-0317, PR-TEST-0321, PR-TEST-0322 |
| `src/hook/protocol_v2_tests.rs` | PR-TEST-0362 |
| `src/hook/restore_tests.rs` | PR-TEST-0258, PR-TEST-0259, PR-TEST-0263 |
| `src/hook/service_runtime_tests.rs` | PR-TEST-0369, PR-TEST-0385 |
| `src/hook/shell_loader.rs` | PR-TEST-0491, PR-TEST-0576 |
| `src/hook/startup.rs` | PR-TEST-0570, PR-TEST-0571, PR-TEST-0577 |
| `src/hook/tests.rs` | PR-TEST-0096, PR-TEST-0328 |
| `src/pack_transport_tests.rs` | PR-TEST-0538, PR-TEST-0539, PR-TEST-0540, PR-TEST-0541, PR-TEST-0542, PR-TEST-0543, PR-TEST-0544, PR-TEST-0546, PR-TEST-0547, PR-TEST-0548, PR-TEST-0549 |
| `src/persistence/runtime_content_store.rs` | PR-TEST-0051 |
| `src/persistence/schema_v5_contract_tests.rs` | PR-TEST-0183 |
| `src/persistence/sqlite_deletion_tests.rs` | PR-TEST-0396, PR-TEST-0397 |
| `src/persistence/sqlite_instances.rs` | PR-TEST-0075, PR-TEST-0076 |
| `src/persistence/sqlite_managed_lifecycle_tests.rs` | PR-TEST-0225, PR-TEST-0226, PR-TEST-0229 |
| `src/persistence/sqlite_migration_run_tests.rs` | PR-TEST-0301, PR-TEST-0302, PR-TEST-0303, PR-TEST-0304, PR-TEST-0305 |
| `src/persistence/sqlite_migrations.rs` | PR-TEST-0285 |
| `src/persistence/sqlite_recovery_tests.rs` | PR-TEST-0325, PR-TEST-0326, PR-TEST-0327, PR-TEST-0330 |
| `src/persistence/sqlite_revision_metadata.rs` | PR-TEST-0062, PR-TEST-0064, PR-TEST-0066, PR-TEST-0067 |
| `src/persistence/sqlite_revision_store.rs` | PR-TEST-0053, PR-TEST-0054, PR-TEST-0339 |
| `src/persistence/sqlite_runs.rs` | PR-TEST-0085 |
| `src/persistence/sqlite_service_storage_tests.rs` | PR-TEST-0346 |
| `src/persistence/sqlite_v6_tests.rs` | PR-TEST-0294, PR-TEST-0295, PR-TEST-0296, PR-TEST-0297, PR-TEST-0298 |
| `src/revision_core_v1/mod.rs` | PR-TEST-0040, PR-TEST-0041, PR-TEST-0044, PR-TEST-0045 |
| `src/revision_core_v2_tests.rs` | PR-TEST-0331, PR-TEST-0334 |
| `src/revision_core_v3.rs` | PR-TEST-0493 |
| `src/snapshot_bundle_tests.rs` | PR-TEST-0206 |
| `src/workflow.rs` | PR-TEST-0081, PR-TEST-0366 |
| `tests/m5_path_selection.rs` | PR-TEST-0290, PR-TEST-0307, PR-TEST-0311, PR-TEST-0565 |
| `tests/m7_retirement_cli.rs` | PR-TEST-0436, PR-TEST-0437 |
| `tests/oracles/hook_protocol_v1.mjs` | PR-TEST-0031 |
| `tests/pack_transport_cli.rs` | PR-TEST-0545 |
| `tests/system/common.rs` | PR-TEST-0142, PR-TEST-0143, PR-TEST-0147 |
| `tests/system/initialization.rs` | PR-TEST-0574, PR-TEST-0575, PR-TEST-0585, PR-TEST-0588 |
| `tests/system/matrix.rs` | PR-TEST-0148, PR-TEST-0150, PR-TEST-0156, PR-TEST-0167, PR-TEST-0168, PR-TEST-0173, PR-TEST-0175 |
| `xtask/src/hook_protocol_v1.rs` | PR-TEST-0025, PR-TEST-0026, PR-TEST-0028, PR-TEST-0029, PR-TEST-0030, PR-TEST-0032 |

## Verification of this inventory

This is documentation-only work. Runtime evidence above is historical/source-
inspected, not a fresh product-suite pass. No mechanically verifiable product
requirement is declared complete merely because this inventory is complete.

| Check | Result and scope |
| --- | --- |
| Inventory and source declaration integrity | **Passed:** all 370 rule definitions, 76 Pending rows, 17 later rules and referenced test locations reconcile. The PR-REQ-0091 reverse-only link is explicitly recorded, not silently accepted as complete coverage. |
| Documentation/link/Spec checks | **Passed:** 25 tests on Windows and on the configured remote, including text links, catalog ownership and unchanged embedded SQL. |
| Site typecheck and production build | **Passed** on the configured remote with Node 24.19.0 and pnpm 11.21.0; 169 text-edition documents generated. |
| Source provenance | All 415 selected tracked-source/new-audit inputs match the per-file SHA-256 manifest before and after remote verification. No unrelated untracked workflow/archive or connection file is included. |
| Rust/full product CI | **Not run:** no runtime, Spec contract, DDL, fixture, Cargo or dependency-lock input changed. |

Local audit scripts, input manifests and verification logs are retained under
`target/obligation-audit/` (ignored, not proposed for commit). Remote verification
used an isolated source/dependency directory; the earlier verified workspace was
not modified. Initial dependency reuse/offline-cache attempts did not pass the
site preflight; isolated frozen-lockfile installation resolved that environment
issue before the successful checks. No preview server or deployment was started.
