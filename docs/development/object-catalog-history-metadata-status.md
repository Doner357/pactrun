---
title: Object Catalog, History and Metadata Status
---

# Object catalog, history and metadata status

**Status: S0-S4 implemented and verified on feature/object-catalog-history-metadata.
Not committed or integrated into develop; no publication is authorized.**

The [approved baseline](./design-notes/object-catalog-history-metadata-baseline.md)
records the operator's B S0-S4 authorization. The owning
[command contract](../spec/behavior/command-and-output-reference.md#pr-req-0353---human-catalog-retained-history-and-local-metadata-commands)
defines the human catalog increment. Storage remains V11, without a migration,
new format, metadata history or version token.

## Delivered behavior

- Revision list/show and full typed metadata inspection, including complete
  label/provenance sources, local aliases, notes and descriptive trust.
- Retained Instance history and deletion discovery, plus global or exact
  Instance Run enumeration, independent of current live names.
- Bounded identity-keyset lists (default 50, range 1-500), safe text, explicit
  truncation/full-value display and continuation commands. Existing Run lists
  adopt the new bounded behavior. Ordering is not chronology.
- Exact-target, explicit-expectation semantic CAS operations for alias/note/trust.
  Existing transaction, idempotency, conflict and crash boundaries are retained.
- Read-only snapshot queries; no implicit initialization, upgrade, reconciliation,
  Run acceptance, service access, or new GC roots.

Name-based Run continuations use the exact resolved InstanceId. This preserves
the selected object across name reuse and produces safe copyable commands even
for names containing shell punctuation. It is not a switch to another object.
Human layout is not a machine protocol. No pager/TUI or JSON envelope is added.

## Verification

The complete configured-remote `cargo xtask ci` gate passed against source
archive SHA-256:

```text
f2780e5a0e95db86c334499938c4a8941d009d8175bd2df0f24f25a9324daede
```

The archive and per-file source manifest were checked on the remote after the
run. Local source also matched that manifest before this documentation-only
closeout. Tests ran in the persistent supported-filesystem test workspace, not
/tmp or tmpfs. CARGO_PROFILE_TEST_DEBUG=0 reduces fixture executable copying;
no product timeout, filter or acceptance rule was weakened.

| Status | Scope |
| --- | --- |
| Passed; fresh full gate | Conformance, cross-language vectors, bidirectional traceability, formatting, workspace/all-target/all-feature Clippy, workspace tests, site typecheck and production build |
| Passed | Linux primary library: 462 passed; three pre-existing explicit capacity tests ignored, not claimed as executed |
| Passed | Linux system suite: 70; actual CLI: 4; catalog CLI: 2; Migration: 10; retirement CLI: 2; lifecycle CLI: 2; Artifact CLI: 1; xtask: 40 |
| Passed; focused Windows checks | CLI library: 31; catalog/retirement/lifecycle process tests: 6; Migration CLI: 10; native interpreter-path CLI tests: 2 |
| Passed; dependency-reused Windows evidence | PowerShell 7/5.1 Shell acceptance: 23, serial; the subsequent alias-only parser fix and added catalog tests do not change its exercised Shell/Run-list paths |
| Passed | Documentation source/link checks: 25; site typecheck and Docusaurus build |
| Passed | Final-candidate local and remote source-manifest checks |
| Not performed | Commit, merge, push, release, Pages changes or deployment |

Initial preflights are not passing evidence: one new test initially referenced
the preceding requirement ID, and legacy Run-list assertions expected the old
unbounded text format. Their assertions were adapted to the approved table,
full-ID and empty-result behavior without weakening Run count/outcome/ownership
checks. Superseded full-suite attempts were stopped before the final candidate.
Only the completed source-matched gate above is claimed as full CI.

This closeout changes documentation only. Its source/link/traceability and remote
site checks are rerun separately; unchanged runtime/test/dependency inputs reuse
the completed full gate rather than claiming another fresh full-suite run.

## Boundaries and retained resources

Revision Bundle remains C, machine output D and baseline consolidation E.
Full usage-guide authoring is excluded; CLI help, owning Spec, this record and
traceability accompany the implementation. No commit, merge, push, release,
Pages workflow modification or deployment has been performed. Existing unrelated
untracked Pages configuration and the user archive remain untouched.

The source archives, logs, Cargo cache/test artifacts, installed site dependencies
and generated site remain in the dedicated remote catalog test workspace. Local
evidence remains under target/. No preview server was started or stopped.

The obligation inventory distinguishes B's delivered catalog/CAS/history clauses
from C's still-pending transport and E's existing non-catalog management/Input/
Instance coverage audit. Those broader rules are not falsely marked complete.
