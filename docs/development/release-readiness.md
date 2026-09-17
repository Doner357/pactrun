---
title: Release Readiness
---

# Release readiness

**Status: Approved pre-release work plan. Required before formal release; relative work order assigned, implementation pending.**

The operator confirmed this direction on 2026-09-16. This is a readiness
checklist, not the next numbered milestone or a fixed post-milestone sequence.
**Retirement of M8 does not trigger these tasks or a release.** See the
[rejection decision](./history/m8-recipes-rejected.md). Additional
development or evaluation may occur before they are scheduled. Work items may
start at suitable times, overlap, and return to design when testing finds gaps.

On 2026-09-17 the operator selected the
[five-milestone completion sequence](./product-completion-milestones.md).
Versioning and Baseline Consolidation combines the first two readiness categories
below and comes last in that sequence. Lifecycle/GC (including Artifact export),
Snapshot capacity/restore convenience, shell loader and machine-readable output
precede it. This establishes relative order, not calendar dates, completed designs
or an instruction to start runtime work. Publication remains separately authorized.

The [product versioning and compatibility policy](../spec/foundations/product-versioning-and-compatibility.md)
owns the observable rules. This plan organizes delivery and evidence without
creating a second specification. M7 is integrated and M8 is rejected; their
records remain in the [roadmap](./implementation-roadmap.md).

## Required readiness work

List order is an organizational grouping, not a mandatory milestone-to-release chain.

| Work item | Deliverable | Acceptance evidence |
| --- | --- | --- |
| Versioning mechanism design and implementation | Concrete version domains, same-Major compatibility enforcement, declared support/requirement checks, internal conversion and upgrade responsibilities | Approved representations and direct tests for the policy's supported/rejected version combinations, including unchanged old meaning |
| Baseline reorganization and consolidation | Current product-oriented Spec, separated history, a coherent first formal schema/format baseline and removal of shipping development-only compatibility code | Rule inventory with no lost obligations, final-contract acceptance/rejection independent of provenance, and no legacy-only interpretation paths |
| Internal testing, evaluation and correction | Real end-to-end use of the reorganized program, Packs and documentation; correctness, diagnostics, usability and operational assessment | Recorded exact builds/baselines, automated and actual-use findings, resolved blockers and reruns after relevant corrections |
| Release mechanism design and implementation | Repeatable procedures for building supported targets, identifying/checking artifacts, version/source/tag consistency, release review and distribution | Successful rehearsal using traceable artifacts and release checks; no implied public publication |

Formal publication is a separate controlled action after readiness acceptance,
not another prerequisite that must somehow be completed before publication.
The publication step labels and validates the actual 1.0.0 artifact, then
publishes the checked artifact rather than silently replacing it with an
untested rebuild. The exact platforms, distribution channels, signing and
pipeline implementation remain release-mechanism design work.

## Dependencies, not an automatic schedule

- Versioning and baseline consolidation are one milestone placed last after the
  four capability/interface milestones. No preliminary Versioning milestone is
  required. Earlier work obeys current contracts without claiming that each
  experimental interface is a prior formal release with permanent compatibility.
- Internal evaluation must ultimately cover that final baseline. Tests of the
  old development formats are not a substitute. Contract changes discovered
  during evaluation return to the owning design and verification work.
- Release-mechanism preparation can overlap internal testing. Passing a build
  job or retiring M8 does not authorize a formal release.
- Before publication, all readiness items and required corrections must be
  accepted. The real versioned release candidate must be checked again after
  the product-version change; simulated compatibility tests alone are not enough.

Product-version and first-baseline behavior is owned by PR-REQ-0332 and
PR-REQ-0333. Consolidation does not itself turn 0.1.0 into a published 1.0.0
product. The shared version label is not proof of conformance: preexisting data
that fully conforms to the final contract is usable, regardless of development
origin. Nonconforming data is refused without developmental compatibility paths.
No separate development-generation rejection mechanism is required.

## Documentation and code reorganization scope

- Keep the active Spec organized by current product concepts and contracts,
  not by the order of milestones, slices or prior approval sessions.
- Extract any still-authoritative rule from a milestone document before moving
  that document to history. Remove duplicate explanations without deleting
  unique requirements or changing their meaning accidentally.
- Separate user/Pack/Hook guidance, contributor instructions and historical
  decisions. Generated HTML/text remains presentation, never a second authority.
- Preserve or explicitly retire requirement/test identities under the existing
  contributor policy. Cosmetic cleanup is not permission to renumber unrelated
  IDs, reuse retired IDs or discard still-applicable regression cases.
- Retire development-only compatibility implementations under the consolidation
  policy. Keep strict Freeze/evolution practices and superseded decisions in
  development history as reference, not shipping compatibility obligations.
  Preserve useful invariant, negative, crash and conformance tests.
- Remove legacy milestone markers from functions/tests during consolidation.
  New work follows the product-oriented naming and rationale rules immediately;
  see the [contributor policy](./development-and-verification.md).
- Do not copy Secrets, connection credentials or service-owned datasets into
  source history merely to record development experience.

This section scopes future reorganization; it does not perform that reorganization
or retire today's Frozen formats. Their current constraints continue until the
dedicated consolidation design and implementation are ready.

## Design details still to close

- Exact product-version requirements/capability declarations and how an older
  reader identifies an unsupported contract before acting; no field name or
  version-range syntax has been approved yet.
- The final baseline's identifiers, encodings and full conformance checks;
  there is no open requirement to distinguish or reject data solely by
  development provenance, and no marker is added merely for that purpose.
- The formal supported-format and persistence-upgrade matrices. Old-binary
  access to newer data is not implied by new-binary backward compatibility.
- Release artifacts, target matrix, distribution/signing procedures and the
  explicit publication approval boundary.

The same-Major/cross-Major policy and the decision not to ship development-era
compatibility are settled direction, not open alternatives to reconsider while
choosing these representations. No permanent cross-Major compatibility promise
or broad CMake-style OLD/NEW policy-switch framework is adopted.

## Evidence and current authorization

This documentation task does not implement versioning, change Cargo's product
version, reset data, remove codecs/migrations, start internal trials, create
release workflows or publish anything. Documentation and regression CI results
must be reported as such, not as proof that readiness work is already complete.
Once implementation is scheduled, keep stable requirement/test traceability and
follow the [risk-based validation policy](./development-and-verification.md#risk-based-validation-scope).
Focused iteration does not remove its full-CI integration and formal-release
gates; documentation-only updates do not trigger the full product suite.

Commit/merge/push and actual public publication remain separately controlled
actions. The unrelated existing publication workflow is not adopted or modified
by recording this plan.

## Reference boundary

Conan's client stability policy and Semantic Versioning informed the selected
Major boundary; see [design references](./design-references.md). References do
not override the owning Pactrun policy, import Conan's binary-ABI model, or
automatically adopt another project's bug-fix exceptions or support cadence.
