# Pactrun agent development entry

This text interface currently serves specification lookup and development of
Pactrun. User-operation, Pack-author, and Hook-integration usage guides remain
placeholders; do not interpret them as completed tutorials.

Current integrated work: A-D are implemented, verified and integrated into local
develop: [diagnostics](../development/execution-diagnostics-observability-status.md),
[catalog/history/metadata](../development/object-catalog-history-metadata-status.md),
[Revision transport](../development/pack-transport-status.md), and
[CLI presentation](../development/cli-presentation-status.md). The
[2026-09-23 short-ID integration](../development/short-id-selectors-status.md)
also includes IPC hardening and subsequent CLI delivery work. The current
persistence baseline is V11, with explicit exact-V8/V9/V10 upgrade.
E (Versioning and Baseline Consolidation) is the next design milestone and
requires separate S0 approval; integration does not authorize publication.

## Establish authority and state

- Read the [Spec map](../spec/index.md). Only Spec defines product requirements.
- Read [current baseline and milestone handoff](../development/next-milestone.md).
  M5, bounded M6, M6.5 ServiceStorage and M7 Cleanup/deletion are implemented
  and integrated into local `develop`. Managed Object Lifecycle and GC is also
  implemented, verified and integrated. Snapshot Capacity and Restore Workflow is
  integrated as well; the integrated develop persistence baseline is V11, with explicit
  exact-V8/V9/V10 upgrade. M8 is rejected and archived; no next
  numbered milestone is selected. Release-readiness work has no assigned start,
  and integration does not authorize publication. Consult the linked closeout
  records for implementation and verification evidence.
- Read the [original product completion plan](../development/product-completion-milestones.md)
  and follow the [remaining A-E sequence approved on 2026-09-19](../development/remaining-capability-milestones.md).
  A is execution diagnostics/Instance observability; B is object catalog/history/metadata;
  C is Revision bundles; D is machine-readable output; E is final consolidation.
  The original grouping approved planning; A-D subsequently received separate
  approvals and are integrated. E retains its separate S0 gate. Full usage guides
  are excluded, while necessary Spec/help/acceptance work remains. Lifecycle/GC is already
  integrated, as is Snapshot Capacity and Restore Workflow. Shell Adapter / Loader
  is implemented, verified and integrated into local develop;
  see its [implementation record](../development/shell-adapter-loader-status.md).
  Hook diagnostic presentation was unresolved at Loader closeout; protocol
  delivery alone did not establish usable display or history.
  The follow-up is implemented, verified and integrated by A into local develop.
  E is the next design milestone and requires separate S0 approval.
  The [verification closure record](../development/verification-gap-closure.md)
  tracks subsequent tests, repaired evidence links and the bounded remaining gaps.
  The [pre-E readiness review](../development/pre-e-readiness.md) owns the current
  follow-up dispositions and final validation/integration gate; the earlier
  closure count is historical. E S0 still needs separate approval.
  Consult the [current obligation audit](../development/obligation-audit-2026-09-23.md)
  for evidence gaps, concrete follow-ups and exclusions. The
  [original inventory](../development/shared-obligation-inventory.md) is a dated
  snapshot, not the current implementation status. New
  functions/tests use product behavior names and no milestone markers. Record design rationale and
  choose Git Flow topic prefixes by intent; see the
  [contributor policy](../development/development-and-verification.md).
- In a checkout, read applicable repository instructions and CONTRIBUTING.md.
  Use the checkout revision and working-tree state to identify the source.
- In the published text edition, [publication metadata](../publication.json)
  identifies the document snapshot. It is not the installed binary's version.

## Develop Pactrun

Start with [the development route](./develop-pactrun.md), select a
[task reading path](../development/reading-paths.md), and open only the owning
contracts and their necessary dependencies. Use the [catalog](../spec/catalog.md)
for exact contract status. Descriptive summaries cannot override a rule.

The old Developer paths only forward links. Historical planning notes, pending
coverage declarations, and proposed capabilities are not implementation evidence.
Report genuine conflicts with their owning rules rather than inventing behavior.

## Deferred usage guides

- [Using Pactrun](./use-pactrun.md)
- [Authoring Packs](./author-packs.md)
- [Integrating Hooks](./integrate-hooks.md)

These retain locations only. Agent discovery is not assumed automatic; this file
can be supplied explicitly to a development tool.
