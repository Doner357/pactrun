---
title: F documentation and website delivery
---

# F documentation and website delivery

**Status: Implemented and verified; operator approved local commit and merge on 2026-09-29.**

## Authorization

On 2026-09-27 the operator authorized F through delivery, verification, and remote
preview on port 3000, initially without Git integration. On 2026-09-29 the operator
authorized local commit and merge into develop. Push, Pages activation, public
Releases, and package-source publication remain unauthorized.
English is the source and website language.
Docusaurus remains a reading-and-search-oriented documentation site.

## Coverage and acceptance

| Area | Deliverable | Acceptance |
| --- | --- | --- |
| Ownership | Current handoff, history classification, source links | No stale baseline claims in reader/agent entry points |
| Writing | External baseline and Pactrun additions | Representative guides reviewed against policy |
| Users | Installation, first Instance, Inputs, Actions, diagnostics, resources, Snapshot/Migration, retirement, transport | Main capability families have procedures and owner links |
| Authors | Minimal Pack, Shell Hook, Actions, parameters, runtime files, Snapshots, Migrations, Cleanup | Tutorial examples exercised in isolated storage |
| Website | Task entry, search/filter, page status, responsive layout | ID and task searches, history scope, keyboard, empty/error states, base path |
| Agents | Same-source text/catalog | Source digest and classification agree |
| Maintenance | Link, traceability, generated help and documentation checks | Tests, typecheck and production build |

## Sequence

F0 inventory and design; F1 ownership and navigation; F2 content and examples;
F3 search and pages; F4 verification and retained preview. Routine decisions
remain within scope. New runtime semantics or external publication require review.

## Verification evidence

The following table records the initial pre-integration verification. Later
correction records and the current authorization above supersede its review/Git
status; they do not turn those earlier runs into fresh verification.

Initial verification on 2026-09-27 (historical delivery evidence):

| Check | Result | Scope |
| --- | --- | --- |
| Documentation tests | Passed | 38 tests: links, requirement/test traceability, owner catalog, text fidelity, search ranking/classification, generated CLI help, and F boundaries |
| TypeScript | Passed | Configured remote, current website source |
| Production build | Passed | Root URL and /pactrun/ base URL; strict links, anchors, indexed routes, source digest, text and CLI-reference freshness |
| Browser acceptance | Passed | 10 checks at each base URL: navigation, generated help, ID search, query/filter persistence, history warnings, empty results, keyboard, mobile overflow, failed-index retry, dark preference |
| Executable tutorials | Passed | Windows x86-64 / PowerShell 7 and Linux x86-64 / sh; minimal Pack and Shell Hook, inspection, missing Action refusal, export/reinstall, retirement and GC plan |
| Visual inspection | Passed | Desktop search, mobile tutorial and dark search screenshots; human review remains pending |
| Full Rust product CI | Not run | Documentation/site-only change; no runtime code or normative product behavior changed |
| Git/publication | Not performed | No commit, merge, push, workflow activation, or public deployment |

The tutorial harness extracts manifest and script blocks from the actual Markdown.
It uses the existing source-qualified E standalone artifacts, identified by the
E delivery receipt, and records executable/tutorial hashes and each command.
This is a new tutorial smoke run against those artifacts, not a rebuild or fresh
whole-product qualification. Early failures exposed missing directory prerequisites,
the exact Windows host executable spelling, and the Hook terminal-output declaration;
the guides were corrected and both platform runs were repeated successfully.

Local detailed receipts, source manifests, screenshots, and logs are retained in
`target/f-delivery/`. The configured remote retains the F source/build workspace,
isolated tutorial data, browser tooling, and the port-3000 preview. A temporary
loopback-only port-3001 service used for base-path acceptance is stopped.

## Reader-review corrections — 2026-09-28

The reader/agent walk-through found contradictions in current Spec prose and
incomplete task navigation despite the initial engineering checks passing. The
operator authorized corrections on 2026-09-28. These corrections align text with
already approved behavior; they introduce no new format, runtime semantics, or
requirement identities.

| Review finding | Correction and evidence |
| --- | --- |
| R1: numeric version prose contradicted E | Source/Core/Hook descriptions use the approved string baseline; executable export and numeric-marker refusal checks |
| R2: obsolete diagnostic-discard claim | Current live/default-retention behavior and suppression option documented; default and opt-out verified through Run inspection |
| R3: reader filter hid owning references | Shared multi-audience classification; Authors find source contracts and Users find invocation details |
| R4: help journey omitted invocation details | Direct [Invoke reference](../pactrun-users/operations/invoke-reference.md), complete parser-option coverage, acquisition/default/error examples |
| R5: stale selector, upgrade and guide status | Effective ID prefixes and retired development upgrade command clarified; Spec entry updated |
| Mobile and exact-ID navigation | Collapsible filters, persistent scope summary, explicit widening, and build-verified requirement heading targets |
| Author examples | Explicit starter file tree; complete Windows/Linux Snapshot and same-Package Migration examples with positive and refusal checks |
| Agent context | Same classification as HTML/search prepended to text; original Markdown body retained exactly after front matter removal |

Correction validation covers 43 documentation tests, remote TypeScript, strict
production builds, 14 browser journeys per tested base URL, and executable
workflow checks on Windows and Linux. The workflow harness reads the actual
published manifest/script blocks and records source and executable hashes.
Final receipts and screenshots are retained under
`target/f-corrections-20260928/`; earlier review evidence remains under
`target/f-review-20260928/`. The retained preview serves the corrected build.

The original small tutorial checks remain separate from the expanded workflow
matrix. Full Rust CI is not rerun for these editorial and website changes.
Existing E artifacts are reused explicitly; this is not a new runtime build or
formal release qualification. Operator content/visual acceptance remains pending.

## Full-site consistency correction — 2026-09-28

The second review covered every published route and identified remaining
current/history overlap beyond the first targeted fixes. The operator authorized
this correction. The current handoff and implementation guide now separate
active contracts from dated milestone descriptions. Their pre-correction sources
are retained as [handoff history](./history/handoff-before-consistency-review-2026-09-28.md)
and [guidance history](./history/guidance-before-consistency-review-2026-09-28.md),
with captured-source hashes and rebased links. Legacy section anchors remain usable.

| Finding | Resolution |
| --- | --- |
| F2-R1: conflicting current support matrix | One effective-baseline table links each owning contract; old matrices are confined to the history snapshot |
| F2-R2: graduated ServiceStorage work described as pending | Identity, normalized authoring, Session authority, resource behavior, Migration and lifecycle prose reference the implemented owners; broader resource taxonomy remains conditional |
| F2-R3: outdated developer work order | Product overview and task/implementation guides describe current capabilities and delegate status to the handoff |
| F2-R4: superseded CLI restrictions | Migration fingerprint/target selectors and retirement machine output compose with the current selector and presentation contracts |
| F2-R5: ambiguous staged Snapshot evidence | S4–S6 evidence is explicitly historical; later CLI closure and E qualification remain separate source-qualified records |
| Reading paths | Duplicate links and obsolete version labels removed; historical design review has an explicit separate route |

The same cross-check found related stale gates in the contract catalog, resource
lifetime and Migration owners; those were reconciled as part of this correction.
Requirement headings, owning files and Verification declarations remain unchanged.
No runtime source, format bytes, schema DDL or product version is changed.

The correction gates comprise 51 documentation checks, TypeScript, strict site
builds including all indexed requirement anchors, browser journeys at root and
project base paths, and an all-route desktop/mobile/link/text sweep. The existing
cross-platform workflow harness now also exercises abbreviated Migration planning
and versioned retirement JSON, giving 55 command executions per platform.
Source-qualified final results, rule-preservation comparisons, screenshots and
source manifests are retained in `target/f-consistency-20260928/`. They supersede
older website receipts for the changed files without relabeling old runtime CI.

Full Rust CI remains Not run for this editorial/site correction. Runtime evidence
uses the existing qualified E binaries explicitly. No commit, merge, push,
public deployment or independent-review claim is made; operator acceptance is pending.

## Tutorial usability correction — 2026-09-29

The tutorial-only Windows walkthrough completed the worked examples but exposed
an acceptance gap: importing into the source store reported `already_present`.
That was duplicate-import evidence, not fresh-store recovery. The operator
requested this correction together with the reading-friction fixes.

The Snapshot guide now exports both the exact Revision and the Snapshot, checks
a new destination, installs the exported Pack, imports the absent Snapshot,
verifies it and restores into a new Instance. Destination cleanup and the return
to the source store are explicit. The source Snapshot must remain available
after destination cleanup. The original same-store Restore remains a separate
step and is not used as proof of this recovery path.

The tutorials also identify the full `exact:` install-output line and the
`path_id:` value to copy, supply complete Windows manifests, provide concrete
incompatible-Revision/unrelated-Package fixtures, and include executable byte
comparisons for PowerShell and POSIX shells. The operator guide links to the same
recovery sequence rather than maintaining a second variant.

The workflow verification executes the published Snapshot command blocks and
operator snippets. It requires absence before import, matching producer identity,
a non-duplicate import, a successful destination Restore, and preservation of the
source Snapshot. It also checks refusal to reuse the destination, comparison
mismatch/read-error handling, and both documented compatibility refusals. Windows
manifests are read directly from the Windows sections; the harness no longer
rewrites POSIX manifests to make them work on Windows.

Source-qualified receipts and screenshots for this correction are retained under
`target/tutorial-polish-20260929/`. The earlier manual walkthrough remains under
`target/tutorial-only-20260929/`. Earlier duplicate-import results are not
retrospectively counted as reconstruction tests. Product runtime, normative
contracts and product version are unchanged; full Rust CI is Not run. Public
publication and Git integration remain unauthorized, and operator acceptance
is still pending.

## Batch-review corrections (2026-09-29)

The next batch review found misleading required-Input deletion guidance, retired
schema pages classified as current, informative maps labeled as product contracts,
stale ServiceStorage availability summaries, and a setup block that could select
an existing tutorial store after directory-creation errors.

The Input guide now separates initially missing required values from deletion of
active optional or retained bindings. Shared classification marks retired schemas
as superseded and informative Spec pages as reading aids in HTML, search, and
agent text. Summary prose points to current owners. M6.5/M7 evidence remains
scoped to its recorded tests; historical schema-upgrade plans do not imply support
for retired numeric stores. Both introductory shell variants refuse an existing
destination before selecting its store.

Regression checks cover the retired schema family, informative maps, guide
distinctions, and direct execution of the published setup block with a fresh
directory, an existing directory, and an existing file. Browser journeys check
retired-result discovery and informative-role filtering. Source-qualified results
for this correction are retained under `target/docs-batch-fix-20260929/`.
These are documentation and presentation corrections; product runtime behavior
and requirement obligations are unchanged. Operator review remains pending.

## Iterative consistency review (2026-09-29)

The operator requested repeated review and correction through a clean verification
pass. The review found remaining milestone-era availability claims inside current
Core, Migration, execution, and persistence contracts; old Snapshot-reader claims
in the format index; and outdated bootstrap, selector, and error-owner descriptions.
These passages now follow the approved E baseline and current operation owners.
Historical exclusions retain explicit scope and stable anchors. Compatibility
links to different persistence slices now resolve to their corresponding sections.

Regression checks compare bootstrap prose with the implementation's private
marker, service diagnostic rows with the registered catalog, and format navigation
with current owning-page titles. They also cover selector/upgrade policy,
historical anchor routing, and known obsolete availability claims. The existing
DDL equality, requirement/test traceability, text-publication fidelity, and
rendered-anchor checks remain in force.

Review iterations, source-qualified verification results, remaining limits, and
the final stop condition are retained under `target/docs-convergence-20260929/`.
This correction changes documentation and its checks, not runtime code, format
bytes, schema DDL, or approved product behavior. Engineering self-review does not
replace operator acceptance. No Git integration or public publication is authorized.

## Reader-role completion (2026-09-29)

The operator requested that users and Pack authors find their working information
inside their own categories, with Spec and Development reserved for optional
traceability. The author entry now includes isolated setup, field choices and
defaults, service/Migration declarations, helper APIs, and direct-protocol details.
The user section includes vocabulary, recovery steps, complete command/lifecycle
references, machine output with local schema copies, and Snapshot limits.

The plain-language guides are the primary route. Advanced reference views are
generated from existing owners and checked for exact synchronization; product
rules are not redefined in the reader categories. Ordinary search favors reader
guidance, while exact requirement searches retain the normative owner. Sidebars
and previous/next navigation stay in the current reader role, and agent entries follow
the same material. Developer routes remain available.

Verification for this change is retained under `target/role-docs-20260929/`.
Role-reading checks cover complete primary paths, generated fidelity, field
choices, shared safe setup, schemas, search ranking, and sidebar reachability.
No runtime, public release, Git integration, or operator-acceptance claim is added.

## Reader-role follow-up review (2026-09-29)

Repeated review added independent protection against rewriting literal Markdown
inside code examples, retained exact source-section links, and supported ordinary
bare relative links in generated views. Invocation references now include exact
integer/float text grammars, rejected forms, and the distinction from YAML defaults.
Reconciliation and GC procedures explicitly state their selected-store-wide scope.

Audience-scoped task search separates reader relevance from title scoring, so
matching role guidance precedes Spec for ordinary queries without overriding
global title relevance. Exact requirement searches retain
their owner. Optional maintainer citations no longer inject unrelated search terms,
and reader pages offer audience-scoped search. Verification includes mobile role
switching and a private project-base-path preview in addition to the root preview.
Source-qualified iteration evidence is retained under `target/role-review-20260929/`.
No runtime or data-format change, public publication, Git integration, or operator
acceptance is implied by this documentation review.

## Review entry points

The 2026-09-29 reading-order audit includes all published document sources,
repository entry documents, the historical migration audit, and the two ignored
local bootstrap mirrors. Its per-file ledger distinguishes current tasks,
reference lookup, compatibility entries, preserved historical evidence, and
noncanonical local files. It does not requalify every historical runtime claim.
Evidence is retained under `target/reading-audit-20260929/`.

Installation now precedes First Instance and contains supplied-archive setup,
checksum verification, executable selection, and terminal-only PATH handling.
Store selection is a separate task. A new supplied-Pack procedure bridges the
empty tutorial to a real live Instance with inspected capabilities. Guided user,
author, and developer routes agree with the sidebar and page-footer navigation;
independent tasks and lookup pages return to their index instead of chaining
unrelated operations. Historical pages direct readers to the current baseline.

Acceptance includes Windows/Linux installation and refusal cases, rendered route
traversal, root/project-base builds, whole-site link/text checks, and rerun tutorial,
field, and workflow examples. This is documentation and site verification, not a
fresh full Rust CI run or publication of a download source.

- [First Instance](../introduction.md) and [user task guide](../guides/index.md)
- [Author tutorial](../package-authors/fundamentals/authoring-model.md)
- [Writing and maintenance policy](./documentation-style.md)
- Website navigation: Search and Commands

## Bounded decisions and remaining limits

Search uses a same-build browser-side full-text catalog and three filters:
state, audience, and document role. Queries do not use an external search provider.
No additional runtime dependency or search account is required. This delivery
has no fuzzy matching, hosted AI answers, version filter, or separate requirement
explorer. Exact requirement queries prioritize the owning current section; every indexed
requirement anchor is checked against the rendered HTML.

Historical records remain available, with explicit warnings. Requirement IDs and
contract obligations are preserved. Obsolete wording is reconciled to the
approved effective baseline; dated evidence stays in its historical record.
This work does not certify that every possible prose inconsistency is eliminated.

The current candidate requires an empty storage directory skeleton for a fresh
standalone tutorial. F documents and tests this prerequisite without changing
runtime initialization. Native package publication and minimum-environment
qualification remain release-readiness work.

Self-review and automated checks are implementation evidence. Local integration
approval does not establish public-release readiness or independent certification.
The operator deferred the Authentik black-box exercise until the complete release
and download flow is available. Its observed binary/document mismatch is not a
confirmed defect in the current runtime. Application startup, real Hook integration,
and portable Pack acceptance remain unverified for that prototype; it is outside
this F integration. No public release or successful external-author acceptance is claimed.
