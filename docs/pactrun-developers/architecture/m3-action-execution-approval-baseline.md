---
title: M3 Action Execution Approval Baseline
---

# M3 Action Execution Approval Baseline

**Status: Informative approved planning baseline and navigation entry point.**

This page records the completed M3 scope and dependency review. It does not
create independent product requirements. The linked requirement-bearing pages
are the sole normative authority, and their text wins if a summary here is
ambiguous.

M3 is approved as the next implementation milestone. Approval permits bounded
implementation of Action execution on an isolated feature branch; it is not a
claim that Action execution exists, that M3 is complete, or that an unresolved
human or machine interface has become stable.

## Approved managed-execution path

| Boundary | Approved M3 meaning | Normative authority |
| --- | --- | --- |
| Human intent | Resolve an exact Instance and Action and normalize typed invocation parameters before compilation. | [PR-REQ-0038](./execution-and-concurrency.md#pr-req-0038---resolver-ownership), [PR-REQ-0086](../product-behavior/packages-revisions-and-instances.md#pr-req-0086---exact-resolution-before-operation), [PR-REQ-0094](../product-behavior/actions-plans-and-runs.md#pr-req-0094---action-invocation), [PR-REQ-0133](../package-contracts/actions-inputs-and-parameters.md#pr-req-0133---invocation-parameter-lifecycle) |
| Compiler input | Use the closed `InvokeAction` intent; do not pass CLI spelling, mutable authoring input, or raw argument syntax into Domain compilation. | [PR-REQ-0037](./execution-and-concurrency.md#pr-req-0037---closed-resolved-intent), [PR-REQ-0008](./system-model.md#pr-req-0008---compiler-and-executor-ownership) |
| Compilation | Produce one side-effect-free, immutable, typed, sequential Plan bound to exact identities and one expected `InstanceStateVersion`. A preview creates no Run or lifetime reservation. | [PR-REQ-0039](./execution-and-concurrency.md#pr-req-0039---side-effect-free-compilation), [PR-REQ-0040](./execution-and-concurrency.md#pr-req-0040---typed-immutable-plan), [PR-REQ-0041](./execution-and-concurrency.md#pr-req-0041---plan-references-are-not-pins), [PR-REQ-0095](../product-behavior/actions-plans-and-runs.md#pr-req-0095---plan-preview) |
| Acceptance | Create the durable Run after successful compilation and before Admission. Resolution and compilation failures create no Run. | [PR-REQ-0007](./system-model.md#pr-req-0007---managed-execution-pipeline), [PR-REQ-0049](./execution-and-concurrency.md#pr-req-0049---run-creation-boundary) |
| Admission | Revalidate the exact state token, Revision, bindings, readiness, policy, environment, Hook runtime, and launcher; establish durable pins before the first workflow side effect. Never silently re-resolve or recompile a stale Plan. | [PR-REQ-0042](./execution-and-concurrency.md#pr-req-0042---admission-linearization), [PR-REQ-0043](./execution-and-concurrency.md#pr-req-0043---plan-invalidation), [PR-REQ-0091](../product-behavior/inputs-secrets-and-readiness.md#pr-req-0091---readiness-admission), [PR-REQ-0194](../package-contracts/revision-core-format-v1.md#pr-req-0194---runtime-launcher-integration) |
| Concurrency | Observe Actions may coexist with Observe and Mutate. Mutate Actions use the same per-Instance mutation boundary as management mutations. Pins preserve the accepted Pactrun-owned view without becoming a guard. | [PR-REQ-0045](./execution-and-concurrency.md#pr-req-0045---observe-and-mutate-access), [PR-REQ-0046](./execution-and-concurrency.md#pr-req-0046---immutable-pactrun-owned-admitted-context), [PR-REQ-0048](./execution-and-concurrency.md#pr-req-0048---pin-and-guard-separation), [PR-REQ-0278](./execution-and-concurrency.md#pr-req-0278---action-admission-exclusivity), [PR-REQ-0279](./execution-and-concurrency.md#pr-req-0279---admission-refusal-precedence) |
| Lifetime | The accepted Run durably pins its exact Revision, runtime content, Inputs, and Secrets until terminal execution and recovery obligations release their last strong reference. | [PR-REQ-0047](./execution-and-concurrency.md#pr-req-0047---durable-execution-pins), [PR-REQ-0074](./resources-and-versioning.md#pr-req-0074---checkpoint-and-pin-lifetime), [PR-REQ-0076](./resources-and-versioning.md#pr-req-0076---physical-content-reachability), [PR-REQ-0275](./persistence-schema-v4.md#pr-req-0275---exact-persistenceschemav4) |

## Approved Hook Runtime boundary

| Boundary | Approved M3 meaning | Normative authority |
| --- | --- | --- |
| Process selection | Direct launch uses the exact materialized owned executable. Interpreter lookup follows normal host pathname resolution and platform eligibility, preserves the exact selected candidate path without object identity, repeats the same ordered selection at Admission, and never searches or falls back in the Executor. | [PR-REQ-0189](../package-contracts/revision-core-format-v1.md#pr-req-0189---identity-bearing-hook-launch), [PR-REQ-0194](../package-contracts/revision-core-format-v1.md#pr-req-0194---runtime-launcher-integration), [PR-REQ-0274](../package-contracts/revision-core-format-v1.md#pr-req-0274---host-launcher-candidate-eligibility) |
| Protocol | Production execution uses the exact Frozen HookProtocolV1 framing, strict message profile, version confirmation, ordering, completion, cancellation, and error state machine. | [PR-REQ-0204](../package-contracts/hook-protocol-v1.md#pr-req-0204---candidate-and-frozen-protocol-contract) through [PR-REQ-0218](../package-contracts/hook-protocol-v1.md#pr-req-0218---authority-is-not-isolation-and-verification-is-layered), [PR-REQ-0280](../package-contracts/hooks-recovery-and-cleanup.md#pr-req-0280---hook-protocol-runtime-transport-discovery) |
| Session authority | Construct only the exact Action Session: typed parameters, execution Workspace, active pinned bindings, authorized Action output slots, and the declared terminal contract. Authority is Pactrun-mediated capability, not an OS sandbox claim. | [PR-REQ-0169](../package-contracts/hooks-recovery-and-cleanup.md#pr-req-0169---session-authority), [PR-REQ-0170](../package-contracts/hooks-recovery-and-cleanup.md#pr-req-0170---session-authority-is-not-host-isolation), [PR-REQ-0210](../package-contracts/hook-protocol-v1.md#pr-req-0210---workspace-and-authority-boundaries), [PR-REQ-0211](../package-contracts/hook-protocol-v1.md#pr-req-0211---operation-specific-binding-visibility) |
| Terminal I/O | Keep terminal streams separate from the protocol stream and enforce the declared `none | output | interactive` contract. Structured output never wraps raw or interactive Hook channels. | [PR-REQ-0172](../package-contracts/hooks-recovery-and-cleanup.md#pr-req-0172---terminal-and-protocol-channels), [PR-REQ-0120](../product-behavior/command-and-output-reference.md#pr-req-0120---structured-output-boundary) |
| Outputs | Preallocate only declared Action output slots. A completion may submit a valid subset; managed publication and Run Artifact retention remain Pactrun-owned after protocol acceptance. | [PR-REQ-0212](../package-contracts/hook-protocol-v1.md#pr-req-0212---managed-output-authorities-and-completion), [PR-REQ-0073](./resources-and-versioning.md#pr-req-0073---run-artifact-and-workspace-lifetime) |
| Sensitive values | Do not persist sensitive parameters, Secret bytes, revealing digests, or default full interactive transcripts in Runs or ordinary diagnostics. | [PR-REQ-0098](../product-behavior/actions-plans-and-runs.md#pr-req-0098---sensitive-run-data), [PR-REQ-0135](../package-contracts/actions-inputs-and-parameters.md#pr-req-0135---sensitive-parameter-channel), [PR-REQ-0214](../package-contracts/hook-protocol-v1.md#pr-req-0214---structured-hook-authored-text) |

## Action recovery required in M3

HookProtocolV1 makes recovery-risk requests a mandatory facility rather than an
optional feature. M3 therefore includes the minimum durable Action recovery
slice needed to implement that Frozen contract correctly:

- durable open-risk state and an Action-specific recovery directive must be
  published before acknowledging `enter_recovery_risk`;
- risk must be durably clear before acknowledging `resolve_recovery_risk`;
- every Running Action Run must have one execution owner, and confirmed owner
  loss must finalize the Run without Hook replay or inferred compensation;
- clear-risk owner loss may finish `Interrupted` without creating a manual
  recovery obligation; open-risk failure or owner loss must apply the durable
  consequence and place the Instance in `ManualRecoveryRequired`;
- a success report with open risk is a protocol failure and also produces the
  required manual-recovery consequence;
- pins and Workspace cleanup remain retained while an unresolved execution or
  recovery reference still needs them.

These are existing requirements in
[Recovery and Reconciliation](./recovery-and-reconciliation.md) and
[Hook Protocol V1](../package-contracts/hook-protocol-v1.md); M3 does not invent
a second Action-only recovery model. M6 remains responsible for completing and
generalizing recovery across later Snapshot, Migration, Restore, and Cleanup
managed executions, their commit boundaries, richer reconciliation, and all
remaining recovery interfaces.

## Planned implementation slices

M3 implementation is ordered as these reviewable slices:

1. typed resolution, parameter binding, `InvokeAction`, and side-effect-free
   Plan compilation;
2. exact internal persistence migration for Runs, durable pins, Action recovery
   state, ownership, output metadata, and their strong-reference lifetimes;
3. transactional Run acceptance, Admission revalidation, guard and pin
   coordination, and crash/failure injection;
4. cross-platform Hook process launch, Workspace and binding materialization,
   exact HookProtocolV1 runtime, terminal I/O, cancellation, timeout, and process
   loss;
5. managed Action output publication, Run finalization and inspection, Action
   recovery reconciliation, and end-to-end positive and negative tests;
6. the minimal human invocation, plan, Run inspection, cancellation, sensitive
   parameter, and manual-recovery spellings required to complete M3.

The internal persistence encoding and platform mechanisms remain implementation
decisions, but they must preserve the linked durable and crash-visible meaning.
Exact new CLI spelling is externally observable and must be closed in a
requirement before M3 completion; this approval does not guess or freeze it.

## Explicit exclusions

M3 does not implement Snapshot Capture or Restore, Migration execution, Cleanup
or Instance deletion, ServiceStorage or Managed Service Resource declaration or
authority, a public Rust API, a stable machine-readable CLI envelope, a generic
DAG engine, an OS sandbox, scheduler behavior, Hook replay, or compensation.

Workspace remains execution-scoped scratch. Inputs and Secrets remain detached
Pactrun-authoritative bindings. Neither one may be used to simulate persistent
service-owned live state, and an execution pin or `InstanceStateVersion` never
claims to freeze or linearize such state.
