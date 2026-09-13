---
title: Spec Migration Review
---

# Spec migration review

**Status: Documentation source closeout; the validation and integration gates
below apply. This is not product milestone implementation approval.**

## Review these four things

1. [Spec map](../spec/index.md): can you locate the owning contract without
   assembling an answer from old handoffs and milestone summaries?
2. [Vocabulary](../spec/glossary.md) and a contract's reading map: is the purpose
   and required context clear without turning this into a beginner tutorial?
3. [Development paths](./reading-paths.md): can an implementer or agent find the
   relevant behavior, wire, persistence, and verification references?
4. [M5 handoff](./next-milestone.md): are the existing decisions and remaining
   approval/design work distinguishable? This migration does not approve M5.

## Scope and preservation

- All 30 existing requirement-bearing pages are now in Spec; the 302 existing
  requirement IDs are retained. These are migration counts, not permanent limits.
- Original normative bodies, unnumbered constraints, definitions, exceptions,
  statuses, pending declarations, and test references are retained. Relative
  links are rewritten and explicitly informative reading maps are inserted.
- Informative syntheses and planning/implementation records moved to Development.
  The normative M4 execution boundary and older schema contracts remain in Spec.
- Old paths retain forwarding entries and original section anchors, not duplicate
  definitions. The earlier resource-model page remains a compatibility entry.
- User and Pack-author guides, including usage-oriented agent pages, are
  placeholders. The first-edition concept sample is not a current usage guide.
- Existing product runtime behavior, test vectors, and product test declarations
  are unchanged. Two existing tests embed Markdown: their include paths now point
  to Spec, while their IDs, assertions, and expected results are preserved. New
  checks protect document organization and publication.

## File migration ledger

| Previous location under docs | Current location under docs |
| --- | --- |
| pactrun-developers/architecture/system-model.md | [spec/foundations/system-model.md](../spec/foundations/system-model.md) |
| pactrun-developers/architecture/identity-and-state.md | [spec/foundations/identity-and-state.md](../spec/foundations/identity-and-state.md) |
| pactrun-developers/architecture/resources-and-versioning.md | [spec/foundations/resources-and-versioning.md](../spec/foundations/resources-and-versioning.md) |
| pactrun-developers/architecture/execution-and-concurrency.md | [spec/execution/execution-and-concurrency.md](../spec/execution/execution-and-concurrency.md) |
| pactrun-developers/architecture/recovery-and-reconciliation.md | [spec/execution/recovery-and-reconciliation.md](../spec/execution/recovery-and-reconciliation.md) |
| pactrun-developers/architecture/m4-snapshot-lifecycle-approval-baseline.md | [spec/execution/m4-snapshot-lifecycle-approval-baseline.md](../spec/execution/m4-snapshot-lifecycle-approval-baseline.md) |
| pactrun-developers/architecture/persistence-schema-v2.md | [spec/persistence/persistence-schema-v2.md](../spec/persistence/persistence-schema-v2.md) |
| pactrun-developers/architecture/persistence-schema-v3.md | [spec/persistence/persistence-schema-v3.md](../spec/persistence/persistence-schema-v3.md) |
| pactrun-developers/architecture/persistence-schema-v4.md | [spec/persistence/persistence-schema-v4.md](../spec/persistence/persistence-schema-v4.md) |
| pactrun-developers/architecture/persistence-schema-v5.md | [spec/persistence/persistence-schema-v5.md](../spec/persistence/persistence-schema-v5.md) |
| pactrun-developers/architecture/error-taxonomy-v1.md | [spec/contracts/error-taxonomy-v1.md](../spec/contracts/error-taxonomy-v1.md) |
| pactrun-developers/architecture/m3-action-execution-approval-baseline.md | [development/design-notes/m3-action-execution-approval-baseline.md](design-notes/m3-action-execution-approval-baseline.md) |
| pactrun-developers/architecture/non-identity-metadata-semantic-baseline.md | [development/design-notes/non-identity-metadata-semantic-baseline.md](design-notes/non-identity-metadata-semantic-baseline.md) |
| pactrun-developers/architecture/pre-m2-installation-instance-binding-baseline.md | [development/design-notes/pre-m2-installation-instance-binding-baseline.md](design-notes/pre-m2-installation-instance-binding-baseline.md) |
| pactrun-developers/architecture/service-storage-semantic-baseline.md | [development/design-notes/service-storage-semantic-baseline.md](design-notes/service-storage-semantic-baseline.md) |
| pactrun-developers/engineering/design-references.md | [development/design-references.md](design-references.md) |
| pactrun-developers/engineering/development-and-verification.md | [development/development-and-verification.md](development-and-verification.md) |
| pactrun-developers/engineering/implementation-guidance.md | [development/implementation-guidance.md](implementation-guidance.md) |
| pactrun-developers/engineering/implementation-roadmap.md | [development/implementation-roadmap.md](implementation-roadmap.md) |
| pactrun-developers/engineering/m4-implementation-status.md | [development/m4-implementation-status.md](m4-implementation-status.md) |
| pactrun-developers/package-contracts/actions-inputs-and-parameters.md | [spec/contracts/actions-inputs-and-parameters.md](../spec/contracts/actions-inputs-and-parameters.md) |
| pactrun-developers/package-contracts/authoring-model.md | [spec/contracts/authoring-model.md](../spec/contracts/authoring-model.md) |
| pactrun-developers/package-contracts/hook-protocol-v1.md | [spec/contracts/hook-protocol-v1.md](../spec/contracts/hook-protocol-v1.md) |
| pactrun-developers/package-contracts/hooks-recovery-and-cleanup.md | [spec/contracts/hooks-recovery-and-cleanup.md](../spec/contracts/hooks-recovery-and-cleanup.md) |
| pactrun-developers/package-contracts/migrations.md | [spec/contracts/migrations.md](../spec/contracts/migrations.md) |
| pactrun-developers/package-contracts/pack-source-yaml-v1.md | [spec/contracts/pack-source-yaml-v1.md](../spec/contracts/pack-source-yaml-v1.md) |
| pactrun-developers/package-contracts/recipes-and-runtime-content.md | [spec/contracts/recipes-and-runtime-content.md](../spec/contracts/recipes-and-runtime-content.md) |
| pactrun-developers/package-contracts/revision-core-format-v1.md | [spec/contracts/revision-core-format-v1.md](../spec/contracts/revision-core-format-v1.md) |
| pactrun-developers/package-contracts/snapshot-bundle-v1.md | [spec/contracts/snapshot-bundle-v1.md](../spec/contracts/snapshot-bundle-v1.md) |
| pactrun-developers/package-contracts/snapshot-integrity-format-v1.md | [spec/contracts/snapshot-integrity-format-v1.md](../spec/contracts/snapshot-integrity-format-v1.md) |
| pactrun-developers/package-contracts/snapshot-integrity-format-v2.md | [spec/contracts/snapshot-integrity-format-v2.md](../spec/contracts/snapshot-integrity-format-v2.md) |
| pactrun-developers/package-contracts/snapshots-and-managed-data.md | [spec/contracts/snapshots-and-managed-data.md](../spec/contracts/snapshots-and-managed-data.md) |
| pactrun-developers/product-behavior/actions-plans-and-runs.md | [spec/behavior/actions-plans-and-runs.md](../spec/behavior/actions-plans-and-runs.md) |
| pactrun-developers/product-behavior/command-and-output-reference.md | [spec/behavior/command-and-output-reference.md](../spec/behavior/command-and-output-reference.md) |
| pactrun-developers/product-behavior/inputs-secrets-and-readiness.md | [spec/behavior/inputs-secrets-and-readiness.md](../spec/behavior/inputs-secrets-and-readiness.md) |
| pactrun-developers/product-behavior/m4-runtime-capabilities.md | [spec/behavior/m4-runtime-capabilities.md](../spec/behavior/m4-runtime-capabilities.md) |
| pactrun-developers/product-behavior/m4-snapshot-command-reference.md | [spec/behavior/m4-snapshot-command-reference.md](../spec/behavior/m4-snapshot-command-reference.md) |
| pactrun-developers/product-behavior/snapshots-migrations-and-recovery.md | [spec/behavior/snapshots-migrations-and-recovery.md](../spec/behavior/snapshots-migrations-and-recovery.md) |
| engineering/documentation-edition-1.md | [development/history/documentation-edition-1.md](history/documentation-edition-1.md) |
| pactrun-developers/product-behavior/packages-revisions-and-instances.md | [spec/behavior/packages-revisions-and-instances.md](../spec/behavior/packages-revisions-and-instances.md) |

The source outside Spec contains no numbered product-rule definitions. Human
navigation presents Spec and Development first, with usage placeholders clearly
separated. Agent instructions remain text-only. The generated catalog is an
informative index and is checked against its owning contract statuses.

## Validation interpretation

The migration audit compares the pre-move document bodies after normalizing only
relative links and removing marked informative navigation. This protects
unnumbered text as well as IDs. Requirement/test relationships and pending
metadata are separately compared. Existing coverage gaps are preserved, not
silently reported as solved.

The shared verification entry remains `cargo xtask ci`. The website build checks
Spec-only definitions, catalog consistency, guide placeholders, links, and text
publication. Tests for engineering processes do not acquire artificial product
requirement/test IDs. Final execution results belong to the task report; this
page alone does not prove CI or every platform-specific test passed.

## Source closeout and local Git integration

This closeout was prepared on September 13, 2026 from the integrated M4 base
`1bbef042584b5593a31ba31ec100e593c5b3f320`. It covers the complete Spec and
development-document migration, the usage-guide deferral, and the correction of
the roadmap's historical V4 wording. V4 was the M3-stage schema; V5 is the current
M4 schema. No schema implementation or compatibility rule changes with that
wording correction.

The reviewed source is committed on `feature/docs-edition-1` and integrated into
`develop` with a no-fast-forward merge only after the following gates pass:

| Gate | Required closeout evidence |
| --- | --- |
| Rule preservation | All 30 existing requirement-bearing documents and 302 IDs retained; normative bodies, unnumbered constraints, pending declarations, and test relationships unchanged apart from documented link relocation |
| Test fixture relocation | The two embedded Markdown references point at Spec; existing test IDs, assertions, and expected results remain unchanged |
| Full verification | `cargo xtask ci` passes on the exact final source snapshot in the configured persistent POSIX workspace |
| Documentation publication | Types, source links, Spec-only ownership, catalog consistency, guide placeholders, and text-export checks pass |
| Preview | Root and project-prefix builds and browser checks pass; text and HTML derive from the same document sources |
| Git scope | Only reviewed project sources are staged; personal settings, credentials, preview outputs, temporary artifacts, and unrelated untracked work are excluded |
| Integration identity | The merge tree equals the verified source commit tree; Git history records the source and integration commit IDs without self-referential hashes in this file |

The integration commit and the accompanying task verification report record gate
completion. This record alone is not proof that a later edit has been tested.
Windows-native full-suite execution is not claimed by the POSIX result. Existing
pending product coverage is not retrospectively cleared by a passing migration
check or by Git integration.

M5 remains Proposed. The next product step is its bounded scope/dependency review,
not implementation inferred from this documentation closeout. Private preview
services may continue under the separately authorized preview scope; neither
their availability nor this merge is a public deployment or release.

## Explicit non-goals

No product behavior, Frozen format, CLI promise, schema, or roadmap milestone
approval is changed. No usage tutorials are completed. Personal workspace
preferences, tracked agent-loader policy, remote pushes, release tags, and public
deployment are separate scopes. This closeout does not delete the feature branch
or alter unrelated branches.
