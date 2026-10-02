---
title: Migration CLI diagnostics and help
---

# Migration CLI diagnostics and help

**Status: Authorized implementation, 2026-10-02; verification in progress.**
The owner approved five items: Migration Input diagnostics, recovery-guard
guidance, readable human plans, explicit unobserved/unacquired preview facts,
and descriptions in existing help. Delivery stops at verified integration into
`develop`. No main merge, Pages publication, product release/tag or package update
is authorized by this milestone. Work branches are removed remotely after merge.

## Approved information policy

Expose the minimum sufficient, explicitly needed public facts, with machine
information at least equivalent to human diagnostic facts. Safe target/Input IDs
(including Secret Input names), phase and bounded reason classification are
approved. Secret values, native paths, raw OS strings, value-derived digests,
automatic retry, override, guard acknowledgment and privilege escalation are not.
Future expansion requires compatibility/disclosure review; fewer fields must not
mean ambiguous facts. Existing guard Run IDs and committed-boundary fields are
reused instead of adding contradictory copies.

## Concrete boundary and compatibility review

- Add optional error.diagnostic for pre-acceptance Migration Input acquisition:
  kind, instance_id, target_revision, input_id, phase, reason, run_acceptance.
  Classification is typed before rendering; human/JSON/JSONL use the same facts.
- Existing error identities, result alternatives, exit codes and Run creation
  boundaries do not change. No failure is reclassified as automatically retryable.
- Add operator_input_acquisition to machine plans, optional for older producers;
  current producers emit not_performed. Existing service observation facts remain.
- Keep machine help in result.usage with the complete same documentation text;
  no new help query grammar or parser migration is included.
- CLI format remains 1.0-alpha.1: additions are optional for readers and existing
  fields/types/closed enums keep their meanings. Schemas and compatibility tests
  cover the extensions. Strict out-of-contract readers rejecting all unknown
  fields are not a general compatibility guarantee. Future diagnostic enum/kind
  changes must be reviewed, not assumed additive-safe.
- Source product version remains the existing development version until a
  separately authorized delivery. No rebuilt artifact replaces published alpha.2.

CLIG human-first/help and GOV.UK error-writing principles follow the existing
[design references](./design-references.md); Pactrun contracts own semantics.
Site help is generated from the executable's source declaration and must remain
consistent. Larger service-transform/repair tutorials, retirement performance
and progress investigations, visual redesign and documentation snapshots remain
outside this implementation.

## Verification

Planned: focused acquisition/privacy/no-Run and JSONL checks; guard refusal with
an accepted Run and no new Hook; plan no-read and human/machine comparisons;
full help/source-site consistency; generated schema review; complete remote CI
and hosted Windows/Linux gate before develop integration. Results are recorded
at closeout, not inferred from compilation alone.
