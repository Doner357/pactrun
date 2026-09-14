---
title: M5 Implementation Status
---

# M5 implementation status

**Status: Complete. The approved M5 scope is implemented, verified and integrated
into local develop. This is not a release or Pages deployment.**

The [approved baseline](./design-notes/m5-migration-implementation-baseline.md)
fixes M5 scope. The operator subsequently authorized commit, merge and push.
Implementation commit `e40f4569111b8b0e2ae70d4b3663d0733cef2f82` was integrated
on September 14, 2026 by no-fast-forward merge
`3d01aedd193e8d38fb0ce99da66e8be2ecb63840`. The merge tree is identical to the
verified feature tree. The retained feature branch records implementation;
develop is the integrated baseline. This follow-up updates integration records
and corrects the obsolete README capability description.

At this local integration step no Git remote was configured, so no push was
performed. The SSH CI workspace is not a Git publication destination. Publication
requires the operator's destination URL and non-force reconciliation with its
actual branch state. No release tag, branch deletion or Pages deployment is added.

Completion evidence must include full CI after the final integration-document
edit. The delivery report records that exact source manifest and CI result.

## Implemented foundation

- Pure exact-path selection over intrinsic-valid installed Revision observations:
  unique simple paths, explicit exact selections, and relational role checks.
- Typed symbolic binding plans: declared writers, absence, protection,
  retained continuity, simultaneous renames, mandatory outputs, and chained
  readiness. No generic DAG engine or service-owned resource model.
- Read-only compilation observations over one SQLite read transaction, including
  the complete active/retained registry and Package graph. These create no Run,
  writer admission, staging, or lifetime pin.
- Compiler integration with exact observation consistency checks and whole-path
  Hook protocol, runtime-content, and launcher qualification.
- Stateless versioned path IDs bind exact routes rather than row ordinals and
  resolve without enumerating the graph. Bounded, cursor-based path discovery
  preserves IDs when newly installed Revisions change the candidate list.
- Real read-only CLI: instance migration-paths and instance migrate --plan
  with --path selection. Direct and chained paths use the same mechanism.
- Exact V6 schema and explicit V5-to-V6 upgrade: admission-aware quiescence,
  old-writer revalidation, transactional rollback, data preservation, and
  no implicit orphan reconciliation. Pristine stores now initialize as V6.
- Typed Migration Run acceptance, inspection, whole-path Admission, actual path
  and payload pins, checkpoints and atomic declarative edge publication are
  implemented. Final edge and Run success share one transaction. Input management
  cannot interleave with an admitted Migration; Observe consequences remain visible.
- Declarative CLI execution reuses owner continuations and the accepting lease.
  Uncertain acceptance or lost publication acknowledgments never create a new
  Run or repeat an edge. Interrupted chains retain their last committed boundary.
- Checkpoints protect committed bindings until the terminal recovery directive
  is applied; the Instance retains the committed data afterward. Historical
  progress remains readable without retaining hidden Input history.
- Domain/compiler/persistence/CLI tests PR-TEST-0277 through PR-TEST-0292, plus
  unnumbered regression cases. Traceability is scoped to those foundations, not
  Migration execution or the broader M6/ServiceStorage promises.
- V6 schema/upgrade/reference tests PR-TEST-0293 through PR-TEST-0300. The
  historical V4-to-V5 bootstrap remains test-only evidence, not an M5 CLI option.

## Runtime integration and acceptance coverage

- S3: target-qualified operator files use native-path parsing, whole-chain writer
  preflight, bounded detached staging and atomic edge publication. Read-only
  plans do not open their files; empty acquired files remain present values.
- S4: Frozen Migration Sessions expose full pinned source and staged active
  target views. The shared supervisor handles mandatory outputs, risk handshakes,
  cancellation, deadlines, retained process control and durable-operation retry.
- S5 tests cover mixed chains, invalid completions/protection fields, output
  capacity failure, cancellation/timeouts, lost commit acknowledgments and
  fresh-process reconciliation at each commit and recovery-risk boundary.
  Final-Hook atomic success, incomplete results, multiple independent Hook
  Sessions, full active/retained source views, native paths, and unchanged Frozen
  Migration transcripts have dedicated regression evidence.

Current production storage is V6. V5 requires explicit storage upgrade; V4 and
earlier require a compatible build to reach V5 first. Hook-backed chains and
operator file inputs are integrated into the existing Migration Run path.
PR-TEST-0301 through PR-TEST-0311 cover the declarative substrate; PR-TEST-0312
through PR-TEST-0324 cover operator/Hook integration. Windows targeted tests and
the configured persistent POSIX workspace's full cargo xtask ci are the delivery
gates. Revalidation after the closeout changes is mandatory.

## Scope deliberately not closed

No ServiceStorage representation/runtime, generalized M6 recovery, Cleanup or
deletion, Recipes, stable JSON output, or public Rust API is introduced. Migration
reconciliation preserves only the last committed Pactrun-owned boundary; it does
not certify service bytes or replay service transformations. Broader pending
ServiceStorage/Package-transformation requirements remain pending, including the
future-facing scope of PR-REQ-0159, PR-REQ-0166, PR-REQ-0237, PR-REQ-0238 and
PR-REQ-0245. Managed Input evidence must not be used to close those future rules.

The existing Pages workflow and deployment configuration are unchanged. The
unrelated untracked Pages workflow and pactrun.zip are excluded from all commits.

## Resolved command decision

The operator approved candidate listing followed by path-ID selection. Both
A-to-C and A-to-B-to-C can now be selected with their listed IDs; --direct and
--via are not needed. The exact versioned selector and pagination contract is
PR-REQ-0309. No mutable path registry, abbreviated selector or new wire identity
was introduced.

## Verification discipline

Run local workspace-contained foundation tests and formatting/Clippy, then full
cargo xtask ci in the configured persistent remote workspace. A later source or
documentation edit invalidates earlier full-tree verification. Test IDs are
evidence links, not a claim that every future milestone rule is covered.

Use a checkout-local Cargo target. Sharing a target directory across extracted
checkouts can reuse xtask's compiled-in workspace path and test the wrong tree.
Keep document dependencies local to the checkout rather than symlinking another
project's node_modules. Verify the tested source manifest before and after CI.
