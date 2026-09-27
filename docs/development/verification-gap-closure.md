---
title: Verification Gap Closure
---

# Verification gap closure

**Status: Historical first closure batch: 53 original Pending blocks closed.**

The subsequent [pre-E readiness review](./pre-e-readiness.md) owns the current
17-follow-up dispositions, final validation and local integration status.
Counts and evidence below describe this first batch, not the current unresolved
census. Preserve this distinction when reusing its verification evidence.

This is the operator-authorized follow-up to the
[2026-09-23 obligation audit](./obligation-audit-2026-09-23.md), on the same
uncommitted topic branch based on develop `f81c308`. It strengthens tests and
repairs evidence links for existing contracts. It does not approve E S0, change
product semantics, activate a format, add compatibility behavior, or authorize
Git integration/publication. The original audit remains a dated pre-change
snapshot, not today's unresolved-count authority.

## Result and limits

- 53 of the original 76 Pending-containing blocks have clause-backed evidence
  mapped below, including the two historical-scope records. Their owning Spec
  declarations and actual source tests now point both ways. The complete remote
  gate and affected Windows checks passed on the inputs recorded below.
- 23 Pending-containing blocks remain: 17 bounded existing-contract follow-ups,
  five unimplemented E versioning/baseline rules, and one conditional isolation
  rule. A remaining proof obligation is not a confirmed runtime defect.
- The additional reverse-only link from PR-TEST-0090 to PR-REQ-0091 is repaired.
- 11 new Rust regressions (PR-TEST-0590 through PR-TEST-0594 and PR-TEST-0599 through
  PR-TEST-0604), four existing tests given stable IDs (PR-TEST-0595 through
  PR-TEST-0598), and three strengthened existing tests (PR-TEST-0338,
  PR-TEST-0558, PR-TEST-0560) provide the missing direct assertions.
- A cheap documentation gate now checks actual Rust/Node declarations against
  owning verification citations in both directions, with negative fixtures for
  omitted links, wrong owners and duplicate IDs. This replaces reliance on the
  one-off audit script when only documentation checks are selected. It accepts
  legitimate ID gaps and the existing prose-style verification declaration.
- No product runtime behavior has been changed. New Hook modes are test-fixture
  behavior only. No Pending marker is removed merely because a nearby test has
  a promising name; the table records its exercised clause.

## New and strengthened acceptance

| Tests | Actual boundary exercised |
| --- | --- |
| PR-TEST-0590, PR-TEST-0599 | Short reads, invalid/duplicate initial binding requests and public file-acquisition failure leave no Instance, payload subset or Run; incomplete creation and empty required bindings remain legal. |
| PR-TEST-0591 | Repeated values cannot restore an old state token; identity remains stable, one registry entry is replaced, deletion removes its reference and no management Run is invented. |
| PR-TEST-0592 | Closed authoring rejects Stack, catch-all capabilities and raw argv parameters; familiar Action names do not infer built-in lifecycle behavior. |
| PR-TEST-0593 | Valid public command shapes reject generic force/yes and arbitrary Input protection switches before storage or export effects. Purpose-specific positive authorization remains covered by existing tests. |
| PR-TEST-0594, PR-TEST-0600 | All supported Core versions preserve distinct lineage/content tuples through actual transport; installed digests respond to logical role/path/executable changes but not source naming or portable metadata. |
| PR-TEST-0595 through PR-TEST-0598 | Existing detached-plan and typed/defaulted-parameter tests now have stable bidirectional requirement identities; assertions were not weakened. |
| PR-TEST-0601, PR-TEST-0602 | Production protocol parsing rejects extra risk depth/severity fields; real native post-ACK effects remain after a resolved-risk failed operation without a recovery guard. |
| PR-TEST-0603 | A real Action Session exposes active bindings only while retained managed state remains intact. Existing Capture, Migration and Cleanup tests supply the other context cases. |
| PR-TEST-0604 | The actual executable reports incomplete Migration as success plus readiness=false in human and JSON modes, without changing Instance identity or inventing a recovery guard. |
| PR-TEST-0338 | Strict codec dispatch now checks canonical V1/V2/V3 and rejects unsupported full envelopes through both Core and content entry points. |
| PR-TEST-0558, PR-TEST-0560 | Real management/metadata operations create no Run or independent token mutation; invocation parameters and Run/Artifact publication do not become managed Input mutations. |

## Clause-backed closure map

Test IDs refer to executable source declarations and the owning Spec citations,
not new requirement definitions. These are bounded acceptance tests, not a formal
proof against every future code change or an independent review claim.

| Owning requirement | Evidence | Exercised clause |
| --- | --- | --- |
| [PR-REQ-0002 - Package taxonomy](../spec/foundations/system-model.md#pr-req-0002---package-taxonomy) | PR-TEST-0068, PR-TEST-0592 | Closed Pack parsing accepts the supported shape and refuses Stack and catch-all authoring without inferring another package model. |
| [PR-REQ-0006 - Distinct operation concepts](../spec/foundations/system-model.md#pr-req-0006---distinct-operation-concepts) | PR-TEST-0069, PR-TEST-0592, PR-TEST-0585 | Separate authoring fields and actual Action/Capture/Restore/Migration/Cleanup completions preserve operation identity; arbitrary Action names do not select built-ins. |
| [PR-REQ-0011 - Stable Package lineage](../spec/foundations/identity-and-state.md#pr-req-0011---stable-package-lineage) | PR-TEST-0069, PR-TEST-0067, PR-TEST-0594, PR-TEST-0600, PR-TEST-0142 | Explicit PackageId survives source/metadata/content changes and transport; an explicitly different PackageId remains a distinct lineage; ID generation has no storage effects. |
| [PR-REQ-0012 - Operational content digest](../spec/foundations/identity-and-state.md#pr-req-0012---operational-content-digest) | PR-TEST-0044, PR-TEST-0331, PR-TEST-0493, PR-TEST-0067, PR-TEST-0594, PR-TEST-0600, PR-TEST-0346 | Independent canonical vectors and actual installed-digest matrices separate operational content from PackageId, source/portable/local metadata and live service bytes. |
| [PR-REQ-0013 - Exact Revision identity](../spec/foundations/identity-and-state.md#pr-req-0013---exact-revision-identity) | PR-TEST-0053, PR-TEST-0054, PR-TEST-0594 | Exact tuple identity distinguishes equal digests in different Packages and changed digests in one Package, with idempotent import and immutable storage. |
| [PR-REQ-0014 - RevisionCore projection](../spec/foundations/identity-and-state.md#pr-req-0014---revisioncore-projection) | PR-TEST-0040, PR-TEST-0041, PR-TEST-0331, PR-TEST-0493, PR-TEST-0069, PR-TEST-0600 | Explicit normalized projection matches independent complete-format vectors; canonical components exclude portable metadata and retain version-specific semantic fields. |
| [PR-REQ-0015 - Runtime content closure identity](../spec/foundations/identity-and-state.md#pr-req-0015---runtime-content-closure-identity) | PR-TEST-0044, PR-TEST-0045, PR-TEST-0069, PR-TEST-0600 | Canonical descriptors carry supported object kind, blob, logical ID/path and executable semantics; equal bytes at changed roles differ, moved source bytes do not. |
| [PR-REQ-0016 - Semantic normalization](../spec/foundations/identity-and-state.md#pr-req-0016---semantic-normalization) | PR-TEST-0041, PR-TEST-0045, PR-TEST-0068, PR-TEST-0333, PR-TEST-0337, PR-TEST-0494 | Typed scalar/default normalization precedes canonical projection; strict negative vectors reject duplicates and preserve ordered versus semantic-set representations. |
| [PR-REQ-0017 - RevisionCoreFormatV1 hash contract](../spec/foundations/identity-and-state.md#pr-req-0017---revisioncoreformatv1-hash-contract) | PR-TEST-0001, PR-TEST-0012, PR-TEST-0044, PR-TEST-0045 | Production and independent oracle verify exact V1 JCS bytes, hash framing/domain, digest and negative vectors. |
| [PR-REQ-0018 - Stable published identity](../spec/foundations/identity-and-state.md#pr-req-0018---stable-published-identity) | PR-TEST-0044, PR-TEST-0331, PR-TEST-0493, PR-TEST-0338, PR-TEST-0339, PR-TEST-0494 | Supported Core bytes remain exact across adapters; the dispatcher now checks V1/V2/V3 and rejects unsupported full Core envelopes without fallback. |
| [PR-REQ-0023 - Stable Instance identity](../spec/foundations/identity-and-state.md#pr-req-0023---stable-instance-identity) | PR-TEST-0591, PR-TEST-0604, PR-TEST-0258, PR-TEST-0436 | Input mutation, Migration and Restore retain InstanceId; retirement/name reuse creates distinct identity. |
| [PR-REQ-0025 - Opaque InstanceStateVersion](../spec/foundations/identity-and-state.md#pr-req-0025---opaque-instancestateversion) | PR-TEST-0591, PR-TEST-0259, PR-TEST-0301, PR-TEST-0330, PR-TEST-0346 | Restoring earlier Input values does not reuse tokens; authoritative restore/migration/guard commits advance tokens while live service writes do not. |
| [PR-REQ-0026 - State-version publication](../spec/foundations/identity-and-state.md#pr-req-0026---state-version-publication) | PR-TEST-0591, PR-TEST-0301, PR-TEST-0263, PR-TEST-0330, PR-TEST-0377 | Input, per-edge Migration, Restore, recovery guard/manual resolution and service-target publication exercise atomic state-version boundaries and rollback. |
| [PR-REQ-0027 - Non-versioned observations](../spec/foundations/identity-and-state.md#pr-req-0027---non-versioned-observations) | PR-TEST-0558, PR-TEST-0560, PR-TEST-0246, PR-TEST-0346, PR-TEST-0602 | Catalog/local metadata, successful Action/Artifact and Capture publication, direct service mutation and resolved-risk failure preserve authoritative Instance tokens. |
| [PR-REQ-0029 - One managed binding registry](../spec/foundations/identity-and-state.md#pr-req-0029---one-managed-binding-registry) | PR-TEST-0280, PR-TEST-0285, PR-TEST-0591 | Active/retained roles derive from one registry, and repeated binding replacement leaves one binding rather than parallel stores/history identities. |
| [PR-REQ-0031 - RequiredInputsSatisfied](../spec/foundations/identity-and-state.md#pr-req-0031---requiredinputssatisfied) | PR-TEST-0590, PR-TEST-0599, PR-TEST-0225, PR-TEST-0321, PR-TEST-0525 | Incomplete creation remains legal; empty required bytes count as present; missing/readiness state is derived and actual incomplete Migration succeeds. |
| [PR-REQ-0032 - Initial binding acquisition and Instance publication](../spec/foundations/identity-and-state.md#pr-req-0032---initial-binding-acquisition-and-instance-publication) | PR-TEST-0590, PR-TEST-0599, PR-TEST-0147 | Public acquisition failure and transactional short-read/invalid/duplicate Input cases publish no Instance or partial registry; explicit empty bytes are retained. |
| [PR-REQ-0035 - Management operations](../spec/execution/execution-and-concurrency.md#pr-req-0035---management-operations) | PR-TEST-0558, PR-TEST-0591, PR-TEST-0088, PR-TEST-0208, PR-TEST-0210, PR-TEST-0545, PR-TEST-0469 | Actual management/query/transport/lifecycle families bypass execution; Instance mutations retain CAS/guard rules, and import/recovery/metadata acceptance explicitly checks no new Run. |
| [PR-REQ-0038 - Resolver ownership](../spec/execution/execution-and-concurrency.md#pr-req-0038---resolver-ownership) | PR-TEST-0115, PR-TEST-0586, PR-TEST-0587, PR-TEST-0532, PR-TEST-0593, PR-TEST-0596, PR-TEST-0598 | Resolver fixes exact identities and typed parameters before compilation; ambiguous selections and security spelling errors cannot acquire input or mutate storage. |
| [PR-REQ-0040 - Typed immutable plan](../spec/execution/execution-and-concurrency.md#pr-req-0040---typed-immutable-plan) | PR-TEST-0595, PR-TEST-0596, PR-TEST-0366, PR-TEST-0231, PR-TEST-0284, PR-TEST-0396 | Detached typed Action plans plus Snapshot/Migration/deletion compilation checks fix exact context and requirements without launch or live-byte snapshot claims; no generic DAG is introduced. |
| [PR-REQ-0044 - Accepted execution continuity](../spec/execution/execution-and-concurrency.md#pr-req-0044---accepted-execution-continuity) | PR-TEST-0301, PR-TEST-0305, PR-TEST-0258 | An accepted owner continues across its own committed Migration boundaries and successful Restore rather than invalidating itself. |
| [PR-REQ-0077 - Separate version domains](../spec/foundations/resources-and-versioning.md#pr-req-0077---separate-version-domains) | PR-TEST-0338, PR-TEST-0369, PR-TEST-0385, PR-TEST-0183, PR-TEST-0538, PR-TEST-0557 | Mixed Core/Hook combinations, independent Snapshot representation, transport and CLI format selection prove version domains are not one generalized schema. |
| [PR-REQ-0079 - Revision Core format ownership](../spec/foundations/resources-and-versioning.md#pr-req-0079---revision-core-format-ownership) | PR-TEST-0044, PR-TEST-0045, PR-TEST-0331, PR-TEST-0333, PR-TEST-0493 | Each currently supported Core has exact positive/negative canonical vectors, framing and independent digest checks; no new formal support policy is inferred. |
| [PR-REQ-0082 - Hook Protocol version](../spec/foundations/resources-and-versioning.md#pr-req-0082---hook-protocol-version) | PR-TEST-0020, PR-TEST-0022, PR-TEST-0023, PR-TEST-0026, PR-TEST-0028, PR-TEST-0030, PR-TEST-0031, PR-TEST-0032, PR-TEST-0362, PR-TEST-0491 | Language-neutral wire versions cover handshake, contexts, risk/control/EOF and completion; real V2 and shell helper tests retain adapter boundaries. |
| [PR-REQ-0084 - Revision import identity](../spec/foundations/resources-and-versioning.md#pr-req-0084---revision-import-identity) | PR-TEST-0538, PR-TEST-0539, PR-TEST-0540, PR-TEST-0544, PR-TEST-0594 | Actual Revision transport preserves canonical content and portable metadata; all tuple identity/idempotence cases and malformed reimport are covered. |
| [PR-REQ-0085 - Snapshot import identity](../spec/foundations/resources-and-versioning.md#pr-req-0085---snapshot-import-identity) | PR-TEST-0206, PR-TEST-0208, PR-TEST-0209, PR-TEST-0213, PR-TEST-0269 | Both Snapshot versions retain canonical provenance/bindings/content through export/import; same-ID/different-digest and corrupted repeat import cannot replace data. |
| [PR-REQ-0086 - Exact resolution before operation](../spec/behavior/packages-revisions-and-instances.md#pr-req-0086---exact-resolution-before-operation) | PR-TEST-0062, PR-TEST-0066, PR-TEST-0532, PR-TEST-0586, PR-TEST-0587, PR-TEST-0588 | Reference labels deduplicate exact targets and use typed ordering; ambiguous/short references fail or resolve once before public mutation/acquisition. |
| [PR-REQ-0090 - Ordinary execution context](../spec/behavior/inputs-secrets-and-readiness.md#pr-req-0090---ordinary-execution-context) | PR-TEST-0603, PR-TEST-0249, PR-TEST-0312, PR-TEST-0322, PR-TEST-0424 | Real Action and Capture Sessions exclude retained authority; Migration and Cleanup receive their separately typed retained contexts. |
| [PR-REQ-0105 - Declared Migration path](../spec/behavior/snapshots-migrations-and-recovery.md#pr-req-0105---declared-migration-path) | PR-TEST-0277, PR-TEST-0278, PR-TEST-0290 | Exact same-lineage target-owned paths are selected; missing/invalid/cyclic edges do not invent direct routes. |
| [PR-REQ-0106 - Chained progress](../spec/behavior/snapshots-migrations-and-recovery.md#pr-req-0106---chained-progress) | PR-TEST-0303, PR-TEST-0304, PR-TEST-0317 | Later cancellation and process loss preserve earlier committed edges without replay or rollback of an earlier boundary. |
| [PR-REQ-0107 - Incomplete Migration result](../spec/behavior/snapshots-migrations-and-recovery.md#pr-req-0107---incomplete-migration-result) | PR-TEST-0301, PR-TEST-0321, PR-TEST-0604 | Incomplete intermediate progress and successful incomplete final states are covered, including actual human/JSON success and readiness reporting without a recovery guard. |
| [PR-REQ-0108 - Declassification authorization](../spec/behavior/snapshots-migrations-and-recovery.md#pr-req-0108---declassification-authorization) | PR-TEST-0281, PR-TEST-0311, PR-TEST-0280 | Declassification requires both declared transition and operator authority, and retained/discarded alternatives remain distinct binding dispositions. |
| [PR-REQ-0115 - Instance operations](../spec/behavior/command-and-output-reference.md#pr-req-0115---instance-operations) | PR-TEST-0599, PR-TEST-0525, PR-TEST-0604, PR-TEST-0436, PR-TEST-0437 | Real create/show/migrate/delete/abandon commands preserve incomplete readiness and identity/trust presentation, with explicit abandonment. |
| [PR-REQ-0116 - Input operations](../spec/behavior/command-and-output-reference.md#pr-req-0116---input-operations) | PR-TEST-0076, PR-TEST-0147, PR-TEST-0280, PR-TEST-0558, PR-TEST-0593 | Input CRUD, role/protection/CAS rules and public raw bytes are exercised; arbitrary protection flags and generic confirmation are rejected before effects. |
| [PR-REQ-0119 - Security-sensitive authorization spelling](../spec/behavior/command-and-output-reference.md#pr-req-0119---security-sensitive-authorization-spelling) | PR-TEST-0078, PR-TEST-0311, PR-TEST-0173, PR-TEST-0437, PR-TEST-0270, PR-TEST-0593 | Purpose-specific positive authorization is paired with generic yes/force rejection for disclosure, declassification, recovery and abandonment. |
| [PR-REQ-0124 - Separate authoring capabilities](../spec/contracts/authoring-model.md#pr-req-0124---separate-authoring-capabilities) | PR-TEST-0069, PR-TEST-0336, PR-TEST-0592 | Separate capability projections are accepted and catch-all operation/Stack authoring is rejected without conflating private machinery. |
| [PR-REQ-0125 - Action semantics](../spec/contracts/actions-inputs-and-parameters.md#pr-req-0125---action-semantics) | PR-TEST-0069, PR-TEST-0148, PR-TEST-0168, PR-TEST-0592 | Action parameters/access/I/O/outputs remain declared semantics; names such as cleanup or capture still parse as ordinary Actions with no inferred capability. |
| [PR-REQ-0126 - Action is not Hook](../spec/contracts/actions-inputs-and-parameters.md#pr-req-0126---action-is-not-hook) | PR-TEST-0069, PR-TEST-0592, PR-TEST-0585 | Authoring capabilities and actual completions distinguish Actions from the shared Hook mechanism. |
| [PR-REQ-0128 - Input payload opacity](../spec/contracts/actions-inputs-and-parameters.md#pr-req-0128---input-payload-opacity) | PR-TEST-0075, PR-TEST-0147, PR-TEST-0346, PR-TEST-0590 | Empty/binary payloads round-trip and are detached from live storage; no payload parsing is needed for acquisition or mutation. |
| [PR-REQ-0129 - Required declaration meaning](../spec/contracts/actions-inputs-and-parameters.md#pr-req-0129---required-declaration-meaning) | PR-TEST-0590, PR-TEST-0225, PR-TEST-0231, PR-TEST-0321, PR-TEST-0424 | Missing/empty required bindings gate ordinary Action/Capture, not Instance legality or independent Migration/Cleanup requirements. |
| [PR-REQ-0130 - Active and retained contexts](../spec/contracts/actions-inputs-and-parameters.md#pr-req-0130---active-and-retained-contexts) | PR-TEST-0603, PR-TEST-0249, PR-TEST-0322, PR-TEST-0424 | Actual active-only ordinary Sessions coexist with explicit active/retained Migration and Cleanup views. |
| [PR-REQ-0132 - No implicit declassification](../spec/contracts/actions-inputs-and-parameters.md#pr-req-0132---no-implicit-declassification) | PR-TEST-0281, PR-TEST-0311, PR-TEST-0313 | Only explicit authorized declassification changes protection; invalid Hook-selected protection cannot publish an edge. |
| [PR-REQ-0133 - Invocation-parameter lifecycle](../spec/contracts/actions-inputs-and-parameters.md#pr-req-0133---invocation-parameter-lifecycle) | PR-TEST-0080, PR-TEST-0597, PR-TEST-0598, PR-TEST-0150, PR-TEST-0167, PR-TEST-0560, PR-TEST-0246 | Typed/defaulted single-invocation values reach real Hooks without new Input bindings, state-token mutation or implicit persistence of protected values. |
| [PR-REQ-0134 - Initial parameter types](../spec/contracts/actions-inputs-and-parameters.md#pr-req-0134---initial-parameter-types) | PR-TEST-0080, PR-TEST-0597, PR-TEST-0598, PR-TEST-0592 | Four primitive parameter types and sensitivity are covered; raw argument-vector declarations are rejected by the closed authoring schema. |
| [PR-REQ-0136 - Shared machinery is not shared identity](../spec/contracts/actions-inputs-and-parameters.md#pr-req-0136---shared-machinery-is-not-shared-identity) | PR-TEST-0069, PR-TEST-0231, PR-TEST-0246, PR-TEST-0258, PR-TEST-0270 | Shared parameter machinery produces operation-specific Action/Capture/Restore plans, contexts and completion authority. |
| [PR-REQ-0138 - Revision is the reproducibility boundary](../spec/contracts/recipes-and-runtime-content.md#pr-req-0138---revision-is-the-reproducibility-boundary) | PR-TEST-0044, PR-TEST-0051, PR-TEST-0600 | Exact canonical Core plus owned closure determine digest independently of source naming/presentation; no source reproducibility promise is added. |
| [PR-REQ-0141 - Pactrun-owned materialization](../spec/contracts/recipes-and-runtime-content.md#pr-req-0141---pactrun-owned-materialization) | PR-TEST-0143, PR-TEST-0346 | Actual installed execution survives source removal and service-owned live state stays separate from immutable runtime content. |
| [PR-REQ-0142 - Logical content roles](../spec/contracts/recipes-and-runtime-content.md#pr-req-0142---logical-content-roles) | PR-TEST-0069, PR-TEST-0600, PR-TEST-0331 | Source acquisition path is distinct from identity-bearing logical roles/path/executable metadata; Core V2 resource declarations do not hash live service bytes. |
| [PR-REQ-0173 - Risk-entry request](../spec/contracts/hooks-recovery-and-cleanup.md#pr-req-0173---risk-entry-request) | PR-TEST-0096, PR-TEST-0369, PR-TEST-0328 | No ACK precedes durable risk publication; actual post-ACK native side effects and persistence-failure continuation are exercised. |
| [PR-REQ-0174 - Risk-resolution request](../spec/contracts/hooks-recovery-and-cleanup.md#pr-req-0174---risk-resolution-request) | PR-TEST-0085, PR-TEST-0602 | Durable risk may be cleared while the operation finishes failed; live effects are not rolled back and no open-risk recovery guard is invented. |
| [PR-REQ-0175 - No nested risk taxonomy](../spec/contracts/hooks-recovery-and-cleanup.md#pr-req-0175---no-nested-risk-taxonomy) | PR-TEST-0028, PR-TEST-0085, PR-TEST-0104, PR-TEST-0601 | Closed Clear/Open states reject redundant/nested requests, extra depth/severity fields and success with open risk. |
| [PR-REQ-0218 - Authority is not isolation and verification is layered](../spec/contracts/hook-protocol.md#pr-req-0218---authority-is-not-isolation-and-verification-is-layered) | PR-TEST-0025, PR-TEST-0026, PR-TEST-0029, PR-TEST-0031, PR-TEST-0032, PR-TEST-0249, PR-TEST-0258, PR-TEST-0312, PR-TEST-0404, PR-TEST-0328 | Preserve the original wire-only verifier scope while linking later real runtime/materialization/commit tests; native mediated authority is not confinement. |
| [PR-REQ-0308 - V6 Migration persistence and upgrade boundary](../spec/persistence/persistence-baseline.md#pr-req-0308---v6-migration-persistence-and-upgrade-boundary) | PR-TEST-0294, PR-TEST-0295, PR-TEST-0296, PR-TEST-0297, PR-TEST-0298, PR-TEST-0304, PR-TEST-0327 | Historical V5/V6 fixtures prove exact upgrade/admission/crash preservation and Migration obligations; current ordinary V11 admission is not mislabeled as V6. |

## Existing-contract follow-ups at the first-batch boundary {#remaining-existing-contract-follow-ups}

These 17 rows retain a real owner and completion condition. They do not authorize
new features or relax the current Spec. A semantic ambiguity still returns to its
owning design decision; routine test/implementation repairs do not need invented
product choices. Author responsibilities and architectural claims must not be
misreported as automatically inferred runtime properties.

| Requirement | Owner | Existing partial evidence | Exact outstanding acceptance / review |
| --- | --- | --- | --- |
| [PR-REQ-0001 - Product boundary](../spec/foundations/system-model.md#pr-req-0001---product-boundary) | Architecture | PR-TEST-0142, PR-TEST-0143 | Deliver an explicit command/composition review for daemon, scheduling, orchestration, web and vault exclusions. One-shot CLI smoke tests prove an entry path, not the whole negative product boundary. |
| [PR-REQ-0003 - Source-to-Instance boundary](../spec/foundations/system-model.md#pr-req-0003---source-to-instance-boundary) | Architecture / installation | PR-TEST-0072, PR-TEST-0143 | Add a direct negative acceptance case for an uninstalled Candidate/exact Revision reaching execution, and map source acquisition through installed immutable publication without relying solely on the happy path. |
| [PR-REQ-0007 - Managed execution pipeline](../spec/foundations/system-model.md#pr-req-0007---managed-execution-pipeline) | Execution | PR-TEST-0089, PR-TEST-0225, PR-TEST-0302, PR-TEST-0397 | Complete one acceptance/admission-failure matrix across all five operation families, checking a durable Run exists for post-acceptance refusal and none for pre-acceptance failure. Current family-specific tests are not yet a single clause-complete map. |
| [PR-REQ-0008 - Compiler and Executor ownership](../spec/foundations/system-model.md#pr-req-0008---compiler-and-executor-ownership) | Architecture / Execution | PR-TEST-0595, PR-TEST-0090, PR-TEST-0115 | Detached Action plans are now directly linked. Review every operation compiler/executor entry for reinterpreting source/CLI policy and add a changed-source/changed-command counterexample beyond Action. |
| [PR-REQ-0009 - Modular-monolith dependency direction](../spec/foundations/system-model.md#pr-req-0009---modular-monolith-dependency-direction) | Architecture | No direct automated claim | Deliver a module/dependency ownership review and bounded forbidden-adapter-dependency regression. No runtime test is being invented as proof of global mutable-state or composition policy. |
| [PR-REQ-0010 - Policy and mechanism separation](../spec/foundations/system-model.md#pr-req-0010---policy-and-mechanism-separation) | Architecture / domain adapters | PR-TEST-0041, PR-TEST-0068 | Audit policy-versus-mechanism ownership across identity, lifecycle and compatibility adapters; typed YAML/projection assertions cover examples, not the full dependency boundary. |
| [PR-REQ-0028 - Stable Input identity and opaque bytes](../spec/foundations/identity-and-state.md#pr-req-0028---stable-input-identity-and-opaque-bytes) | Inputs / author contract | PR-TEST-0075, PR-TEST-0147, PR-TEST-0590 | Opaque empty/binary byte preservation is evidenced. Separate the author-owned same-ID/same-meaning obligation from checks Pactrun can enforce, with an explicit review/disposition rather than claiming strings reveal semantic intent. |
| [PR-REQ-0030 - Retained binding lifetime](../spec/foundations/identity-and-state.md#pr-req-0030---retained-binding-lifetime) | Inputs / lifecycle | PR-TEST-0280, PR-TEST-0312, PR-TEST-0437 | Add or identify a public retained-binding lifetime matrix covering reactivation, explicit Migration discard, operator delete and removal/abandonment; prove unrelated retention survives each non-selected disposition. |
| [PR-REQ-0093 - Secret deletion disclaimer](../spec/behavior/inputs-secrets-and-readiness.md#pr-req-0093---secret-deletion-disclaimer) | Inputs / presentation | PR-TEST-0591, PR-TEST-0147 | Managed-reference deletion is now directly linked. Audit product-owned help/errors and documentation for no secure-erasure guarantee; do not add a test purporting to erase SQLite pages/WAL/backups. |
| [PR-REQ-0110 - Recovery options](../spec/behavior/snapshots-migrations-and-recovery.md#pr-req-0110---recovery-options) | Recovery / CLI | PR-TEST-0156, PR-TEST-0173, PR-TEST-0259, PR-TEST-0437 | Exercise inspection, legal Input management, one-run override, resolution, exact Restore and abandonment from a guarded Instance under their separate preconditions, with denied ordinary execution in the same matrix. |
| [PR-REQ-0123 - Explicit normalized semantics](../spec/contracts/authoring-model.md#pr-req-0123---explicit-normalized-semantics) | Authoring / Migration | PR-TEST-0041, PR-TEST-0280, PR-TEST-0336 | Explicitly identify which shorthand the current frontend supports and compare its normalized installed transitions to the fully explicit form; test execution does not reapply source defaults. Do not introduce new shorthand. |
| [PR-REQ-0127 - Stable InputIdentity](../spec/contracts/actions-inputs-and-parameters.md#pr-req-0127---stable-inputidentity) | Identity / author contract | PR-TEST-0039, PR-TEST-0069 | Grammar/identity representation is covered. Record the author-owned semantic non-reuse obligation and independent frontend scope; do not claim a lexical validator can verify arbitrary changed meaning. |
| [PR-REQ-0131 - Secret is protection metadata](../spec/contracts/actions-inputs-and-parameters.md#pr-req-0131---secret-is-protection-metadata) | Inputs / security wording | PR-TEST-0076, PR-TEST-0147, PR-TEST-0346 | Protection-as-metadata and byte opacity are covered. Complete the no-vault/no-secure-erasure claims review, including Hook-facing text, without expanding security promises. |
| [PR-REQ-0143 - Workspace](../spec/contracts/snapshots-and-managed-data.md#pr-req-0143---workspace) | Execution / managed data | PR-TEST-0313, PR-TEST-0263, PR-TEST-0346 | Complete a real Workspace lifetime matrix for success, rejected completion, timeout and crash cleanup; explicitly prove leftover scratch never acquires persistent ServiceStorage or committed-output authority. |
| [PR-REQ-0159 - Declarative transition scope](../spec/contracts/migrations.md#pr-req-0159---declarative-transition-scope) | Migration / authoring | PR-TEST-0280, PR-TEST-0312, PR-TEST-0334 | Add a closed-authoring rejection matrix for declarative Input split/merge/arbitrary payload transformation, alongside positive typed Hook output and separately approved service transformations. |
| [PR-REQ-0166 - Package Migration responsibilities](../spec/contracts/migrations.md#pr-req-0166---package-migration-responsibilities) | Migration / Package responsibility | PR-TEST-0312, PR-TEST-0371, PR-TEST-0373 | Distinguish Package-owned service schema correctness from Pactrun guarantees; prove declarative paths neither launch an optional Hook nor implicitly parse/copy live service bytes. |
| [PR-REQ-0170 - Session authority is not host isolation](../spec/contracts/hooks-recovery-and-cleanup.md#pr-req-0170---session-authority-is-not-host-isolation) | Recovery / security wording | PR-TEST-0328, PR-TEST-0602 | Actual native writes outside Session scratch and no rollback are now directly linked. Complete the host-identity/network/process/other-CLI and no-confinement wording review; no isolation backend is authorized. |

PR-REQ-0329 through PR-REQ-0333 remain owned by separately approved E design and
implementation; their Pending markers are intentional. PR-REQ-0171 remains a
conditional future enforced-isolation contract, not an authorized backend task.
Retired Recipe requirements remain retired, and the original audit's other
explicit scope exclusions still apply.

## Verification and source provenance

The final runtime/test inputs passed the configured-remote full gate because
this acceptance work spans identity, persistence, execution and recovery. Windows
used affected focused tests and all-target Clippy, not a local full product run.
No tests, overflow checks, debug assertions or product timeouts are disabled to
obtain a pass; the four pre-existing explicit capacity/RSS ignores remain visible.

| Check | Result and scope |
| --- | --- |
| Windows focused tests | **Passed: 28 distinct tests**, including the actual executable Migration case, on the final Rust/test inputs. |
| Windows formatting and Clippy | **Passed:** workspace/all-target/all-feature Clippy with warnings denied, plus formatting. Rust 1.98.1; no local full product suite was run. |
| Configured-remote `cargo xtask ci` | **Passed:** all conformance, formatting, Clippy, workspace tests, documentation checks, site typecheck and production build. Rust 1.98.0 on persistent ZFS; isolated build outputs and an embedded source-path check establish the actual test workspace. |
| Linux product tests | **Passed:** library 522; actual system suite 74; other executable integration suites 24 (including Migration 12); xtask 40. Nested helper-process summaries are not added again. |
| Existing ignored tests | **Not run:** four unchanged explicit capacity/RSS cases (Snapshot large/medium/small comparison and large Pack transport). No new ignore or relaxed assertion/timeout was introduced. |
| Document/link and bidirectional declaration checks | **Passed: 27**; the final documentation-only closeout uses the same checks and a separate site typecheck/build. Node 24.19.0 and pnpm 11.21.0; 170 text-edition documents. |
| Source provenance | All 416 selected inputs matched their SHA-256 manifest before and after the complete gate. The 291 runtime/test/Spec inputs remain unchanged during documentation-only closeout. |

The full gate used test optimization level 1, debug information disabled, debug
assertions and overflow checks enabled, and one test thread. Explicit concurrency
tests retain their internal threads/processes. The source archive SHA-256 is
`47ed07b901706fafb6a9afc360686ee915ba153d1d94a3a5e54dd4c57abf4fa0`;
its per-file manifest SHA-256 is
`86b871e010789d618e07f0e289a9a48741deedc4e57dfceeda28e2ecb7f3a723`.

This status/result edit follows the successful complete gate. It changes only
this Development record; it does not change the verified runtime, tests, owning
Spec, dependencies, toolchains or build profile. The final document checks and
site build are rerun against the closeout inputs. Runtime evidence is reused;
this is not a second fresh all-stages CI invocation after the documentation-only
closeout.

Exact source lists, SHA-256 manifests, commands and logs are retained under
`target/verification-closeout/` locally and in the isolated persistent remote
verification workspace. Main evidence is `full-ci.log`,
`source-before-full.log`, `source-after-full.log`,
`runtime-source.sha256`, `windows-focused-final.log`,
`windows-clippy.log`, and the separate final documentation closeout logs.
These are ignored validation resources, not proposed repository files. Source,
build/dependency caches and logs are retained; no preview server, deployment,
commit, merge or push was performed. The unrelated workflow and archive files
remain untouched.
