---
title: Development
slug: /development
---

# Developing Pactrun

**Status: Informative development entry.** This section helps implementers work
from the specification; it does not replace the product rules or an approved
milestone scope.

## Start a development task

1. Read [the Spec map](../spec/index.md) and, if needed, the
   [developer product overview](./product-overview.md).
2. Choose a [task reading path](./reading-paths.md) rather than reading all files.
3. Check [the current baseline and next milestone](./next-milestone.md).
4. Follow [development and verification policy](./development-and-verification.md).
5. Report changed behavior, rule/test evidence, verification results, and limits.

The [roadmap](./implementation-roadmap.md) controls work order. The Spec controls
product semantics. A design summary may help locate a rule but cannot override
it. The [M4 closeout](./m4-implementation-status.md) records integrated work, not
a release or a fresh claim that every test has just run.

The historical work-order entry is the [ServiceStorage staged alignment](./design-notes/service-storage-staged-design-alignment.md).
The [M6 bounded baseline](./design-notes/m6-recovery-implementation-baseline.md)
records the scope of the now-integrated recovery milestone.
The approved work order places ServiceStorage in M6.5 between M6 recovery and
M7 Cleanup/deletion. Bounded M6 S0-S4 is complete and integrated; its
[implementation record](./m6-implementation-status.md) records evidence and integration.
M6.5's separate approval and integration are recorded below; they do not approve M7.

The [M6.5 baseline](./design-notes/m6-5-servicestorage-baseline.md) links the
approved design direction. M6.5 S1-S7 is implemented and integrated into develop;
the [implementation record](./m6-5-implementation-status.md) separates runtime
support, final-source verification and Git integration. [M7](./m7-implementation-status.md)
is implemented and integrated; [M8 is rejected and archived](./history/m8-recipes-rejected.md).
The [product completion milestones](./product-completion-milestones.md) now
start with managed-object lifecycle/GC and end with versioning/baseline
consolidation; the intermediate scopes and rationale are recorded there.
The [lifecycle implementation record](./managed-object-lifecycle-status.md) tracks
the separately approved design and all lifecycle slices on V9, with final
acceptance passed and local develop integration completed. Snapshot Capacity and
Restore Workflow is now [implemented, verified and integrated into local develop](./snapshot-capacity-and-restore-status.md);
Shell Adapter / Loader is [implemented, verified and integrated into local develop](./shell-adapter-loader-status.md).
Its Hook diagnostic presentation gap is now assigned to A in the
[remaining capability plan approved on 2026-09-19](./remaining-capability-milestones.md).
Execution diagnostics/Instance observability is integrated into local develop.
[Object catalog/history/metadata](./object-catalog-history-metadata-status.md) is
also implemented, verified and integrated. [Portable Pack transport](./pack-transport-status.md)
is implemented, verified and integrated into local develop.
Machine-readable output follows C under its [approved S0-S4 baseline](./design-notes/cli-presentation-baseline.md); see the [implementation record](./cli-presentation-status.md). Final consolidation remains after D. This excludes
full usage-guide writing. No retired milestone number is reused.
The [format review](./design-notes/m6-5-format-activation-review.md)
records the independent Core/Hook V2 Freeze gate.

## Release readiness is a separate planning dimension

The [release-readiness checklist](./release-readiness.md) records approved work
that must be accepted before formal publication, with relative work order now
recorded in the completion plan and calendar timing left unassigned.
It does not start automatically after M7 or the retirement of M8.
The [owning compatibility policy](../spec/foundations/product-versioning-and-compatibility.md)
separates this future product commitment from today's implemented format checks.

## Change discipline

For behavior-preserving work, choose local implementation details without
inventing new product concepts. For a real conflict, identify the owning rules,
observable consequences, and proposed decision before changing semantics.
A passing test is evidence of an exercised contract, not permission to weaken it.

The [migration review](./spec-migration-review.md) records the document move and
its preservation checks. User, Pack-author, and operation-oriented agent guides
remain reserved for a later documentation phase.

Repository instructions and the task determine execution permissions. Personal
communication and machine preferences are not part of the Pactrun Spec.
