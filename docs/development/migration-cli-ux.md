---
title: Migration CLI diagnostics and help
---

# Migration CLI diagnostics and help

**Status: Implemented, verified and integrated into develop on 2026-10-02.**
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

## Verification and integration

- Initial implementation: `fb5dc13`; complete configured Linux `cargo xtask ci`
  passed with Rust 1.98.1 on the persistent ZFS workspace.
- Final runtime candidate: `04710078cab3f6b21f4c420b5762019770c817fc`;
  all 528 committed source files matched the remote qualification tree byte-for-byte.
  A fresh complete Linux gate passed with Rust 1.99.0, test opt-level 1, debug
  assertions and overflow checks enabled: 528 library passes (4 existing capacity
  opt-ins ignored), 17 Migration CLI integration passes, 76 system passes, plus
  the remaining workspace/conformance suites and 89 documentation checks.
- Native Windows focused qualification passed 16 Migration CLI cases and the
  help, guard and service-plan regressions. The hosted Windows and Ubuntu full
  gates passed in [run 36993966133](https://github.com/Doner357/pactrun/actions/runs/36993966133).
- PR-TEST-0646 through PR-TEST-0651 cover safe typed acquisition facts, no-Run
  behavior, JSON/JSONL parity, actual permission/read errors, guard refusal with
  an accepted Run and no new Hook, complete help without storage, reason
  classification, and unobserved service plans without Debug wrappers.
- Schema review checked optional additions and old response validity. Unknown
  diagnostic reasons are rejected by the current schema rather than invented;
  explanatory wording is not a machine reason classifier.
- The generated site's command-help text matched compiled CLI --help exactly,
  including all 60 documented command forms. Site typecheck/build passed. This
  was an isolated build, not publication to GitHub Pages.
- [PR #14](https://github.com/Doner357/pactrun/pull/14) integrated the candidate into
  develop as `910a02dcbaf026d1d32ccd216f13cab2bfce6f12`. Documentation-only closeout
  reuses unchanged runtime evidence and reruns the affected documentation checks.

### Rolling-stable verification maintenance

The first hosted run, 36992042861, used Rust 1.99 and rejected pre-existing
single-element-loop/deprecated-atomic code. The follow-up replaced test-only
fetch_update calls with the already available try_update alias, retaining the
same orderings and closures, and removed one single-query loop without changing
its SQL literal or condition. No lint suppressions, test weakening or workflow
policy changes were introduced. Rust 1.98.1 compilation remains supported; the
final candidate was fully checked on 1.99 rather than relying on the older pass.

## Delivery boundary

Main, published alpha.2 assets, tags, native package definitions and the public
website were not updated. The improvements are in develop only and are not
features newly shipped in the existing alpha.2 download. Local/remote test and
build evidence remains; no real service deployment was required. Larger author
tutorials and all previously deferred work remain separate tasks.
