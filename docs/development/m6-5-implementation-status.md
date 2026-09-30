---
title: M6.5 Implementation Status
---

# M6.5 implementation status

**Status: Complete. M6.5 S1-S7 is implemented and integrated into local develop.
The delivery report records final post-integration exact-source verification.**

The operator approved continued S1-S7 implementation on 2026-09-15 and confirmed
mapped-source consumption. Successful mappings atomically replace their source
associations; only unmapped source-only associations remain retained. Neither
consumption nor an unsuccessful transformation authorizes deleting live bytes.
No alias-lineage registry, automatic service rollback or Hook replay is added.

## Implemented slices

| Slice | Implemented behavior | Principal evidence |
| --- | --- | --- |
| S1 | Typed Core V2, strict JSON/YAML projection, independent V2 framing, version-neutral content and production installer dispatch | PR-TEST-0331 through PR-TEST-0339, PR-TEST-0379, PR-TEST-0383, PR-TEST-0384 |
| S2 | Exact V7 initialization and V6 upgrade, eager Instance-scoped storage, no-replace native allocation, conservative preparation maintenance and protected custody | PR-TEST-0340 through PR-TEST-0352 |
| S3 | Active/retained contract views, human list/show/observe/locate, no-follow observation and distinct read/direct/operation disclosure policies | PR-TEST-0353 through PR-TEST-0359, PR-TEST-0383 |
| S4 | Independently selected Hook V2, exact Session authorities and prerequisites, durable allocation pins, real Action/Capture/Restore over live resources | PR-TEST-0360 through PR-TEST-0370, PR-TEST-0385 |
| S5 | Whole-path symbolic mappings, compatible reuse, explicit retained reattachment, source consumption and atomic per-edge association publication | PR-TEST-0371 through PR-TEST-0376, PR-TEST-0383 |
| S6 | Transform target proposals, receipt distinct from success, owner-held publication after zero exit/tree termination and output acquisition/cleanup, atomic risk/Revision/Input/service commit | PR-TEST-0377, PR-TEST-0378, PR-TEST-0380 through PR-TEST-0382 |
| S7 | Cross-process crash, upgrade, platform and conformance evidence; reviewed Git integration and exact-source final verification | PR-TEST-0386 through PR-TEST-0388 and the source-manifest/CI gate below |

The [format review](./design-notes/m6-5-format-activation-review.md) records the
Core/Hook V2 Freeze gate. YAML V1/V2 remain explicitly versioned Candidate
non-Frozen authoring contracts. V7 is an internal non-Frozen schema, not a public
persistence compatibility promise. V1 identity bytes and Hook meanings do not
change when source_format 2 or Hook protocol_version 2 is selected.

## Runtime and failure evidence

- Instance creation allocates isolated empty storage roots, not named service
  files/directories. Missing or replaced protected allocations are refused,
  never recreated or adopted. Existing unproven preparation objects are
  preserved; only definitively absent unreferenced preparations are retired
  after confirmed owner loss under writer exclusion.
- Read-only views do not allocate, reconcile, scan live contents or create Runs.
  Unknown never becomes Absent. Hidden paths, retained writes, unsafe path
  traversal and invalid object kinds are refused under the owning contracts.
- PR-TEST-0370 uses the production YAML V2 installer and executes acquired
  immutable Hook bytes after the original executable source is removed. Actions,
  Capture and Restore preserve live-state versus Input/Snapshot ownership and
  the existing Restore-only guard-clear boundary.
- PR-TEST-0377/0378 install real transforming Hooks through the production source
  path. A receipt never commits or clears risk; nonzero exit, extra messages,
  Hook failure and owner loss preserve the old published associations and
  protected target bytes. No persisted proposal flag permits replay.
- PR-TEST-0380 preserves an intermediate committed transform's Managed Input
  output and service boundary when a later edge crashes. PR-TEST-0381 covers
  explicit cancellation and timeout after an observed receipt, plus cleanup
  failure before publication.
- PR-TEST-0382 splits a live resource and then merges it through two real Hooks.
  A failed second Hook preserves the first commit, Open risk and inspectable
  uncommitted allocation provenance. Success consumes mapped sources without
  phantom aliases, while untouched source-only contracts remain retained.
- PR-TEST-0383 uses the real CLI for V1-to-V2 allocation, V2-to-V1 retention and
  explicit V2 reattachment. Reinstallation after live-byte changes preserves the
  Revision digest and the same physical allocation is reattached.
- PR-TEST-0384 rejects invalid/ambiguous source versions and invalid V2 schemas
  before durable blob/Revision publication. PR-TEST-0369 and PR-TEST-0385 prove
  both independent version combinations: Core V1/Hook V2 and Core V2/Hook V1.
- PR-TEST-0386 covers real file-to-directory transformation with nested target
  grants; the Hook creates the service parent, not Pactrun. PR-TEST-0387 exercises
  actual Linux permission denial without inferring absence. PR-TEST-0388 covers
  all present/absent/any resource-create combinations over live unassociated
  content, including read-only planning and failed-predicate non-publication.

The [S0 scenario audit](./design-notes/m6-5-servicestorage-baseline.md#implementation-audit-of-the-s0-scenarios)
maps every D65 case to concrete evidence and preserves the explicit M7 exclusion.

## Verification history and final delivery gate

The pre-activation snapshot passed complete remote `cargo xtask ci` and an
additional post-run check of every source-manifest entry. Its manifest SHA-256
is `08ec33c273a2433e9b131a488349f65947ccae93e788764e988ad4e3d17f68ef`.
This includes the original M6.5 runtime work through PR-TEST-0381, but not later
activation or test additions. It is not proof of final milestone completion.

Windows focused runs passed the service persistence/runtime suites, including
installer activation, cross-version reattachment, nested transformation,
presence predicates and failed output acquisition. Core V2/Hook V1 execution
also passed separately. Linux focused runs passed real permission denial,
nested transformation, all creation predicates and the expanded transform
failure matrix, with pre/post source checks. Core V2 Node/Rust conformance,
Clippy and traceability checks passed. Each result covers its tested snapshot,
not arbitrary later edits.

The delivery report must record complete remote `cargo xtask ci` and pre/post
source-manifest checks for the final edited source. Do not substitute a focused
test, earlier full run, compilation or source synchronization for that gate.
The S0 scenario audit and the owning requirements provide the completion
checklist; the delivery report supplies the final-source execution result.

## Authorized integration

The operator explicitly authorized Git integration without push. The reviewed
implementation commit is `ec4ef91e92fd8279dcf2bd5d6f917a3159666083`; merge commit
`cf7562d77201d1e0cf29f298101cc4116a51a72b` integrated it into local develop using
Git Flow. The merge tree was verified identical to the feature tree. Unrelated
workflow and archive files were excluded and preserved.

Before integration, the complete final-source `cargo xtask ci` passed with 336
Pactrun library tests, the remaining workspace/integration suites, documentation
typechecking and the production build. Source checks passed before and after;
the source-manifest SHA-256 was
`0925040ef04572adcc0a3c093f1b3a4bbac28bf4d9c6d267d331b50a559554eb`.
This is pre-integration evidence, not a substitute for rerunning full CI after
these closeout edits. The delivery report must identify the final develop HEAD,
its source manifest and the successful post-integration run. Integration is not
a release, push or public documentation deployment.

M7 Cleanup/finalization/abandonment and broader non-ServiceStorage resource
taxonomy remain excluded. V7 preserves allocation custody; M7 must introduce
its own explicit versioned durable receipts before destructive operations.
No commit, merge, push, workflow change or deployment is implicit in this work.
