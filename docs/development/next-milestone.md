---
title: Current baseline and milestone handoff
---

# Current baseline and milestone handoff

**Status: Informative current-state handoff.** Product behavior belongs to
[Spec](../spec/index.md). Use [task reading paths](./reading-paths.md) for
implementation context; verification belongs to the source-qualified delivery
records linked below. Integration does not authorize publication.

## Current baseline

The [Public Preview delivery](./public-preview-delivery.md) is published as
`v1.0.0-alpha.1` in the public MIT-licensed `Doner357/pactrun` repository. The
[alpha.2 delivery](./alpha2-publication.md) advances Preview while preserving
alpha.1's immutable artifacts and exact entries. The
[alpha.3 delivery](./alpha3-publication.md) adds the verified CLI/help and retirement
diagnostics improvements without changing format domains. Scoop and
Homebrew use its ordinary main-branch package catalog; the Docusaurus website is
published on GitHub Pages through verified CI artifacts. Public native acquisition
and website checks are recorded in that delivery ledger. Stable and formal 1.0.0
remain separate future gates. The alpha.1 Authentik evaluation was partially
verified; ordinary-user retirement failed in that evaluation. The corrected
alpha.2 evaluation Pack passed ordinary-user retirement with the released Linux
payload; this does not qualify all Authentik features or repair old data. See the
[evaluated boundary and deferred documentation work](./public-preview-delivery.md#authentik-black-box-follow-up).

The [E delivery ledger](./e-implementation-status.md) records local engineering
qualification and integration. The current published product is `1.0.0-alpha.3`; the
eight current format domains use `1.0-alpha.1`. These selections do not couple
the independent version domains or authorize formal promotion.

| Area | Effective owner and boundary |
| --- | --- |
| Installed Revision and Pack source | [Canonical Revision](../spec/contracts/revision-canonical.md) and [Pack source](../spec/contracts/pack-source.md); one complete baseline including service declarations and Shell Loader alternatives |
| Hook Sessions | [Hook protocol](../spec/contracts/hook-protocol.md); explicit granted authorities, versioned independently of Revision identity |
| Snapshots | [Integrity](../spec/contracts/snapshot-integrity.md) and [bundle](../spec/contracts/snapshot-bundle.md); the current string-marked baseline, with no development-era numeric readers |
| Persistence | [Persistence baseline](../spec/persistence/persistence-baseline.md); unsupported stores are refused without conversion or data deletion |
| CLI | [Versioned machine output](../spec/contracts/cli-machine-interface.md) and [ID selectors](../spec/contracts/cli-id-selectors.md); human and machine presentation use the same operation contracts |
| Pack transport | [Distribution baseline](../spec/contracts/pack-distribution.md); imported bytes must satisfy the complete current contract |

Development-era numeric formats and storage upgrade chains are retired. Updating
the executable does not relabel, rehash, or rebind existing objects. No old
milestone record authorizes restoring an obsolete reader or deleting real data.

## Current approved work: Alpha.3 Preview delivery

**State: Published and verified; public acquisition and Pages verified on 2026-10-03.**

The published product is `1.0.0-alpha.3`; the alpha.1/alpha.2 exact downloads
remain immutable. Public catalog/Pages checks are recorded in the delivery ledger.

The [alpha.3 delivery record](./alpha3-publication.md) owns candidate qualification,
main integration, prerelease publication, Preview package catalog and Pages
updates. Preserve alpha.1/alpha.2 immutable assets and exact entries. No Stable
promotion or documentation redesign is included.

## Previous work: Retirement failure diagnostics and Cleanup guidance

**State: Originally integrated into develop; subsequently shipped in alpha.3.**

The [retirement diagnostic milestone](./retirement-diagnostics.md) improves safe
reason classification, unresolved deletion-obligation guidance and Pack Cleanup
documentation. Existing retirement/recovery authority and behavior stay intact.
No main merge, public release, Pages publication or package update is included.

## Previous work: Migration CLI diagnostics and help

**State: Originally integrated into develop; subsequently shipped in alpha.3.**

On 2026-10-02 the owner authorized the
[Migration CLI diagnostics/help milestone](./migration-cli-ux.md), including
bounded structured diagnostics, guard guidance, readable symbolic plans and
descriptions in existing help. Machine facts must not be poorer than human
diagnostics; additions are demand-driven and remain subject to disclosure and
compatibility rules. Delivery ends at verified `develop` integration. Do not
merge this work to `main`, publish Pages or issue a new Release without a separate
instruction. The public alpha.2 artifacts remain unchanged.

## F — Documentation and Documentation Site {#next-milestone-f--documentation-and-documentation-site}

**State: Implementation complete; public Preview publication followed on 2026-09-30.**

The operator authorized F implementation, verification and remote preview on
port 3000. [F delivery](./f-documentation-status.md) owns its acceptance evidence.
The [scope record](./remaining-capability-milestones.md#f-documentation-and-documentation-site)
records English documentation, Docusaurus, reader/agent navigation, search and
maintenance checks. F's original local-only authorization was superseded for the
separate public Preview delivery on 2026-09-30; its publication evidence belongs
to that delivery ledger, not to F's original local acceptance.

On 2026-10-01 the operator authorized documentation corrections and verified
integration first, followed by a separate alpha.2 fix branch for the Authentik
black-box findings. The documentation correction does not claim a runtime fix.
Visual redesign and versioned documentation snapshots remain deferred.
The [alpha.2 correction record](./alpha2-retirement-fix.md) owns the implemented
fixes, qualification and integration receipts. After verified integration, stop
for the owner's next instruction; no deferred redesign starts automatically.

## What is already recorded as implemented

| Delivery | Evidence owner |
| --- | --- |
| A: diagnostics and Instance observability | [A record](./execution-diagnostics-observability-status.md) |
| B: object catalog, history and metadata | [B record](./object-catalog-history-metadata-status.md) and [approved design](./design-notes/object-catalog-history-metadata-baseline.md) |
| C: Pack transport | [C record](./pack-transport-status.md) and [approved design](./design-notes/pack-transport-baseline.md) |
| D: human and versioned machine presentation | [D record](./cli-presentation-status.md) |
| E: baseline consolidation and native delivery | [E record](./e-implementation-status.md) and [approved scope](./design-notes/e-versioning-s0-plan.md) |
| Managed-object lifecycle and GC | [Lifecycle record](./managed-object-lifecycle-status.md) |
| Snapshot capacity and create-and-restore | [Workflow record](./snapshot-capacity-and-restore-status.md) |
| Shell Adapter / Loader | [Loader record](./shell-adapter-loader-status.md); A supersedes its original diagnostic-presentation gap |

These records describe their tested sources and integration; a table entry is
not a fresh test run. Their original development format numbers do not define
which formats the current product accepts.

## M5 is implemented and integrated into develop

Use the [Migration record](./m5-implementation-status.md) for historical delivery
and the [current Migration contract](../spec/contracts/migrations.md) for new work.

### Already defined: preserve these decisions

Preserve target-owned edges, explicit writers, same-Package paths, protection
rules and per-edge publication. Read the [execution owner](../spec/execution/m5-migration-execution.md)
and [command owner](../spec/behavior/m5-migration-command-reference.md) before editing.

### Handoff checks

Recheck the effective contracts, the source-qualified evidence and the current
Instance/resource state. A Plan reserves nothing, a completed edge is not a
whole-path rollback promise, and managed publication does not establish service
coherence. Use the [verification policy](./development-and-verification.md).

## M6 is complete and integrated into develop

The [M6 record](./m6-implementation-status.md) preserves the bounded recovery work.
Current recovery duties are owned by [recovery](../spec/execution/recovery-and-reconciliation.md),
including explicit risk handling and the prohibition on fabricated service repair.

## M7 is integrated; M8 is rejected

[ServiceStorage delivery](./m6-5-implementation-status.md) and
[lifecycle delivery](./m7-implementation-status.md) have separate evidence.
Their current owners are [ServiceStorage execution](../spec/execution/m6-5-service-storage-execution.md)
and [Instance retirement](../spec/execution/m7-instance-retirement.md).
[M8 Recipes were rejected](./history/m8-recipes-rejected.md), not deferred.
A public Candidate API remains deferred until concrete demand.

## Next planned scope and completion sequence

The [original completion plan](./product-completion-milestones.md) and
[remaining capability sequence](./remaining-capability-milestones.md) retain
their approvals and ordering rationale. F has approval for local integration.
No next M-series milestone is selected. A later task needs its own scope.

The [handoff captured before consistency correction](./history/handoff-before-consistency-review-2026-09-28.md)
preserves the overlapping earlier prose for audit. Use this page's current
baseline table instead of that archived support matrix.

## Release readiness is not the next milestone

The owner authorized [Public Preview delivery](./public-preview-delivery.md)
to the public `Doner357/pactrun` repository under MIT on 2026-09-30, including
Git, Release, Pages and package-source publication. Its ledger records completed
candidate/artifact, public-acquisition and hosted Pages checks. This is not formal
1.0.0 promotion or complete Authentik acceptance.
[CI and publication operations](./ci-and-publication.md) owns the ongoing workflow
and opt-in deployment policy; original local-only restrictions are historical.

[Release readiness](./release-readiness.md) remains a separate acceptance and
publication boundary. E engineering delivery and F review do not authorize a
beta, RC, formal release or public website. Follow the
[compatibility policy](../spec/foundations/product-versioning-and-compatibility.md)
for formal promotion and same-Major guarantees.

## Evidence caveat

Use the original delivery receipt when citing a test result. Later documentation
edits do not turn an earlier pass into new runtime qualification. Configuration
readiness, permission, successful process exit, durable Run outcome and service
coherence remain distinct observations. Non-ServiceStorage resource taxonomy,
OS isolation, encrypted export and other conditional future scope are not implied
by completed ServiceStorage or lifecycle work.
