# Pactrun agent development entry

This text interface currently serves specification lookup and development of
Pactrun. User-operation, Pack-author, and Hook-integration usage guides remain
placeholders; do not interpret them as completed tutorials.

## Establish authority and state

- Read the [Spec map](../spec/index.md). Only Spec defines product requirements.
- Read [current baseline and milestone handoff](../development/next-milestone.md).
  M5, bounded M6, M6.5 ServiceStorage and M7 Cleanup/deletion are implemented
  and integrated into local `develop`. The current persistence baseline is V8,
  with explicit exact-V7 upgrade only. M8 is rejected and archived; no next
  numbered milestone is selected. Release-readiness work has no assigned start,
  and integration does not authorize publication. Consult the linked closeout
  records for implementation and verification evidence.
- Follow the [product completion work order](../development/product-completion-milestones.md):
  lifecycle/GC, Snapshot capacity/restore convenience, shell loader, machine-readable
  CLI output, then versioning/baseline consolidation. New functions/tests use
  product behavior names and no milestone markers. Record design rationale and
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
