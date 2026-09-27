---
title: Pre-M2 Installation, Instance, and Binding Baseline
---

# Pre-M2 Installation, Instance, and Binding Baseline

**Status: Informative canonical synthesis and navigation entry point.**

This page summarizes the closed Pre-M2 design. It does not create independent
requirements. The linked requirement-bearing specification pages are the sole
normative authority, and their text wins if a summary here is ambiguous.

## Closed authoring and installation model

| Topic | Synthesis | Normative authority |
| --- | --- | --- |
| Minimal source | Candidate `PackSourceYamlV1` is a strict, versioned YAML frontend with an explicit Package lineage and complete `RevisionCoreV1` projection. | [PR-REQ-0258](../../spec/contracts/pack-source.md#pr-req-0258---packsourceyamlv1-schema-numbers-and-package-lineage) |
| Scalar decoding | Scalar meaning is schema-directed, not YAML-resolver-directed. Only raw JSON-number lexical tokens are numeric; all explicit tags are rejected. | [PR-REQ-0258](../../spec/contracts/pack-source.md#pr-req-0258---packsourceyamlv1-schema-numbers-and-package-lineage) |
| Portable metadata | Reference labels, current presentation, and provenance have closed source unions; the installed Revision target is derived rather than authored. | [PR-REQ-0258](../../spec/contracts/pack-source.md#pr-req-0258---packsourceyamlv1-schema-numbers-and-package-lineage), [PR-REQ-0260](../../spec/contracts/pack-source.md#pr-req-0260---revisioncandidate-projection-and-metadata-boundary) |
| Source files | Portable lexical paths combine with Windows no-reparse/exact-long-name acquisition or Linux no-follow/no-mount-crossing exact-object acquisition. Manifest and content use the same opened-object rule. | [PR-REQ-0259](../../spec/contracts/pack-source.md#pr-req-0259---sourcerelativepathv1-and-safe-source-acquisition) |
| Candidate boundary | Source normalizes into typed Domain values, then projects to the unchanged Frozen core, runtime-content closure, and derived-target portable metadata plan. | [PR-REQ-0260](../../spec/contracts/pack-source.md#pr-req-0260---revisioncandidate-projection-and-metadata-boundary) |
| Installation | Durable blobs precede one atomic Revision/reference/metadata database publication; metadata keeps M1-D conflict semantics. | [PR-REQ-0261](../../spec/behavior/packages-revisions-and-instances.md#pr-req-0261---revision-installation-publication) |
| Migration relations | `NotEvaluated`, `Valid`, and `Invalid` are derived repository-context states, not identity or installation prerequisites. | [PR-REQ-0262](../../spec/behavior/packages-revisions-and-instances.md#pr-req-0262---migration-relational-installation-policy) |

## Closed Instance and binding model

| Topic | Synthesis | Normative authority |
| --- | --- | --- |
| Instance identity | `InstanceName` is an exact bounded Unicode human reference; `InstanceId` remains the durable identity. | [PR-REQ-0263](../../spec/behavior/packages-revisions-and-instances.md#pr-req-0263---instance-name-and-inspection-order) |
| Payload model | One binding registry points to immutable, random, instance-scoped, non-deduplicated payloads represented by validated chunks. | [PR-REQ-0264](../../spec/behavior/inputs-secrets-and-readiness.md#pr-req-0264---managed-input-payload-and-binding-registry) |
| Creation and staging | File and stdin acquisition use bounded-memory, file-backed staging beneath Pactrun's dedicated root; complete acquisition precedes atomic Instance publication. | [PR-REQ-0265](../../spec/behavior/inputs-secrets-and-readiness.md#pr-req-0265---instance-creation-and-transient-payload-staging) |
| Mutation and export | Active/retained rules govern mutation. Export stages one exact observation without blocking Mutate, then publishes a file atomically without clobber or streams non-atomically to stdout. | [PR-REQ-0266](../../spec/behavior/inputs-secrets-and-readiness.md#pr-req-0266---managed-input-mutation-export-and-reclamation) |
| Concurrency | Every mutation has strict token-first state-version CAS; Observe never becomes a reader guard, while same-Instance Mutate is exclusive. | [PR-REQ-0267](../../spec/execution/execution-and-concurrency.md#pr-req-0267---m2-instance-cas-and-mutation-guard) |
| Secret boundary | Payload protection is a persisted sticky floor. Active bindings combine it with the declaration; retained bindings use the stored floor. This is disclosure protection, not encryption at rest. | [PR-REQ-0034](../../spec/foundations/identity-and-state.md#pr-req-0034---secret-protection), [PR-REQ-0268](../../spec/behavior/inputs-secrets-and-readiness.md#pr-req-0268---m2-secret-storage-and-disclosure-boundary) |

## Persistence and interface boundary

| Topic | Synthesis | Normative authority |
| --- | --- | --- |
| Internal schema | The implemented internal `PersistenceSchemaV3` preserves exact V2 and adds Instances, payload headers/chunks, and bindings with canonical representation. | [PR-REQ-0269](../../spec/persistence/persistence-baseline.md#pr-req-0269---exact-persistenceschemav3) |
| Migration | Pristine, V1, and V2 databases converge transactionally on exact V3; drifted, partial, foreign, and newer databases fail. | [PR-REQ-0270](../../spec/persistence/persistence-baseline.md#pr-req-0270---persistence-migration-to-v3) |
| Interface | The application contract is crate-private. The fixed M2 human CLI is narrow and is normative without becoming a Frozen format. | [PR-REQ-0271](../../spec/behavior/command-and-output-reference.md#pr-req-0271---m2-application-and-minimal-human-cli) |
| Scope gate | M2 cannot use Inputs, metadata, transient staging, or schema rows as a ServiceStorage, execution, recovery, or stable-public-API backdoor. | [PR-REQ-0272](../../spec/foundations/identity-and-state.md#pr-req-0272---m2-scope-and-anti-backdoor-boundary) |

## Important boundary distinctions

- Source staging acquires immutable Revision runtime bytes; transient payload
  staging bounds memory and protects an in-flight operation; persistent
  `ManagedInputPayload` rows hold Instance data; M3 pins retain accepted
  execution context; Workspace is execution scratch; ServiceStorage is future
  service-authoritative live state. None is an alias for another.
- An export's SQLite snapshot protects its selected payload only until the
  exact bytes reach private staging. That operation-local observation is not a
  durable pin, retry record, or recovery state.
- The exact M2 CLI has no installation-time local-metadata options, no generic
  force/overwrite export option, and no stable machine-readable envelope.
- Only a deletion already allowed by the binding mutation contract ends sticky
  binding continuity. Bound active required Inputs remain non-deletable; a
  later allowed bind is newly acquired data, not declassification of the old
  payload.
- Candidate authoring and persistence contracts are implementation-ready but
  remain non-Frozen and non-public. Their integration does not alter any
  Frozen identity, wire, error, or vector contract.

## Deferred work

M3 Action execution and durable pins, M4 Snapshot runtime, M5 Migration
execution, M6 recovery, M7 Instance deletion, M8 advanced authoring,
ServiceStorage representation, stable machine APIs, new Frozen error codes,
`LocalInstall` history, and cryptographic Secret storage remain deferred.
