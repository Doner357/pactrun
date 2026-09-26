---
title: E Versioning Redesign Decisions
---

# E versioning redesign decisions

**Status: Operator-approved direction, versioning matrix, update scope,
service-continuity policy, prerelease acceptance, release-selection model and
one-time clean-reset exception;
detailed representations, implementation plan and release authorization remain open.**

Recorded in repository documentation on 2026-09-25 after the operator authorized
the transfer from local discussion notes to a separate design branch.
The same day's follow-up decisions are incorporated below; recording them does
not imply Git integration or implementation.

## 2026-09-26 implementation authorization and installation update

The operator authorized the entire E implementation, including all slices,
verification and local merge to develop. The delivery boundary is local
engineering acceptance, not GitHub publication. Push, public Releases, remote
bucket/tap publication and document/page restructuring remain deferred. Real
pre-existing data/resource cleanup needs named targets, but this does not restore
any compatibility obligation for the retired development baseline.

Scoop and Homebrew replace the earlier custom/re-downloadable-installer proposal.
Its product safety, explicit selection and data-ownership requirements survive;
native command/source integration must implement them without a resident updater.
Windows 11 and qualified Linux x86-64 are the initial targets, without an added
old-platform compatibility project, independent signing service or fixed LTS/SLA.
Necessary specifications and verification documentation remain part of E.

The following record preserves the decisions and their earlier discussion.
Where installer technology or authorization language below conflicts with this
section, this dated operator-approved update takes precedence. No future formal
release or compatibility claim is implied by authorizing the alpha baseline.

## Authority and transition

This record captures the replacement E design discussion. The goals remain:
define version evolution, reset Pactrun-owned product and format versions, and
remove compatibility machinery for the original development-era versions.
Prior E details were reopened rather than inherited as design constraints.

For future E design, these decisions supersede conflicting assumptions in the
[earlier policy](../../spec/foundations/product-versioning-and-compatibility.md),
[release readiness](../release-readiness.md), and
[E outline](../remaining-capability-milestones.md#e-versioning-and-baseline-consolidation).
In particular, mandatory user-driven storage upgrades do not satisfy the new
same-Major promise, and keeping all internal evaluation at product 0.1.0 is no
longer the selected release workflow. Independently evolving format Majors and
separate CLI result/event versions are replaced by the matrix below.

This does not authorize reinterpretation of current Frozen bytes, identity
changes, reader removal, storage upgrades or relabeling the executable. Owning
Spec requirements must be explicitly reconciled before implementation, preserving
IDs and traceability, especially PR-REQ-0077, PR-REQ-0078 and PR-REQ-0329 through
PR-REQ-0333. Existing runtime contracts remain binding until that approved
transition. This record is not implementation evidence or a second executable
specification. The [pre-E readiness review](../pre-e-readiness.md) records the separately integrated prerequisite verification; it does not prove this redesign implemented.

## Confirmed compatibility policy

The policy below governs evolution after the new baseline. The explicitly
approved [one-time reset exception](#one-time-reset) overrides compatibility,
stability and service-continuity constraints only for the transition from the
original development-era baseline; it is not a general prerelease exemption.

> Formal releases within the same product Major must remain backward compatible.
> All other cases should preserve compatibility where practical, without a
> guarantee. Breaking changes must explain the reason and provide a handling
> procedure.

Backward compatibility means newer software supports earlier formal contracts
and existing objects of that Major, not that old software understands later
capabilities.

- Products use SemVer. Minor releases add features of any size; Patch releases
  fix bugs and provide maintenance. Neither may break the formal same-Major promise.
- Updating Pactrun within a formal Major must not require rewriting Packs or
  Hooks, converting/recreating/rebinding objects, or running a storage-upgrade
  command merely to continue existing use. It must not require service restart,
  redeployment or interruption, or change existing defaults and semantics.
- New features may have opt-in requirements, not new prerequisites for old use.
- Pactrun maintenance bears compatibility costs. Historical representations may
  accumulate within a Major. Major transitions allow incompatible cleanup and
  consolidation; behavior-preserving internal refactoring remains possible at
  other times.
- Prereleases and cross-Major cases should still preserve compatibility where
  practical, not deliberately reject content. Best effort does not promise
  permanent support for every experimental contract.
- Handling procedures need not always be automatic converters, but must provide
  actionable steps. No compatibility guarantee does not mean permission for
  silent misinterpretation, destructive repair, deletion or service interference.

Updates must satisfy the compatibility contract, not weaken it to accommodate
an implementation. Within the formal same-Major promise, inability to preserve
existing use within the approved update scope makes the update design
unacceptable: revise or defer the change rather than transfer the cost to users
or services. Safe refusal alone is not backward compatibility.

For cross-Major or unguaranteed experimental data, preserve compatibility where
practical. If safe support is unavailable, refuse the affected operation before
side effects, preserve existing data and service access, and explain the reason
and separate handling procedure. A Major transition does not itself authorize
service interruption or data conversion.

### Approved update scope and service continuity

This scope covers replacing Pactrun and any automatic compatibility processing
needed for continued existing use. It does not bundle separately requested
Migration, Restore, deletion or service operations; those retain their own
authorization and contracts.

| Object | Permitted update scope |
| --- | --- |
| Pactrun executable and bundled components | May change within the selected installation, Major and release selection; do not overwrite coexisting installs or take over their default command |
| Private metadata, indexes and storage structures | Representation changes only, preserving object meaning, identity, relationships, settings, bindings, lifecycle state and existing operational semantics; do not disrupt active operations |
| Rebuildable private caches and temporary data | May change only when proven disposable or rebuildable and not in use; active Workspaces and Sessions are not disposable caches |
| Published immutable content, including Revisions and Snapshots | No in-place rewriting for an update, including identity or integrity data; provide compatible reading instead |
| Managed Inputs, Secrets and existing Bindings | Do not change effective contents or bindings, or relocate resources in use merely because Pactrun manages them |
| ServiceStorage and other service-authoritative live state | Do not move, convert, rebuild, change permissions or acquire access control that blocks the service for an update |
| User Pack sources, Hooks, external files and service configuration | No update-driven rewriting |
| Resources used by active Runs, Hooks or services | Do not revoke paths, handles, access or contracts, reclaim resources, or take over an operation because a newer executable starts |

Classification depends on ownership and active consumers, not directory
location alone. Even an otherwise permitted private representation change must
not make data, locations, permissions or access methods used by an existing
service under its supported contract invalid or temporarily unavailable.
Keeping the service process alive while denying its data is not continuity.
An update must not require the service to pause, reopen resources, restart or
redeploy. These safety boundaries also apply to prereleases and Major changes.

Changing internal representation is not permission to choose new product state:
for example, changing a binding, Run outcome or protection state still requires
its owning operation contract. Preserve active ownership; do not seize ownership,
force-unlock resources or interrupt an operation to make an update possible.
When safety cannot be established, do not perform the change; retain existing
data and service access. Prefer simple, conservative and verifiable mechanisms.

Cross-version lock/owner coordination and safe representation-change mechanisms
still require engineering design and acceptance evidence against the existing
execution and recovery contracts. No lock-transfer protocol, automatic migration
mechanism, or unconditional downgrade/rollback guarantee is approved here.

### Downgrade and rollback boundary

**Operator-approved policy: users may explicitly select an older executable,
but arbitrary data downgrade or management rollback is not guaranteed.**
Installing an earlier version is distinct from that executable being able to
operate on data already used by a newer version. Ordinary updates still do not
automatically downgrade.

An older executable may operate where its implemented support can safely
interpret the data. Otherwise, refuse the affected operation clearly without
guessing, automatic conversion or service interference. Do not promise a
general-purpose data-downgrade facility or maintain reverse converters solely
to make every historical executable usable again.

This does not authorize newer software to rewrite existing data arbitrarily:
the approved update scope, immutable-content rules and same-Major forward-update
commitments still apply. After an update problem, replacing the executable with
an older one may not restore management capability; a corrected release or an
explicit handling procedure may be necessary. That limitation is not permission
to interrupt the existing service. Detailed diagnostics and implementation
remain to be designed under this policy.

## Confirmed version model

Product syntax is Major.Minor.Patch[-prerelease]. Format/protocol syntax is
Major.Minor[-prerelease], with no Patch. The latter is Pactrun's convention,
not full SemVer. Version values use explicit strings, not numeric values such
as 1.0. Exact field names, placement, wire encoding and suffix grammar remain
open. Parsing and comparison rules are shared; each domain owns its support
decisions.

Format Major identifies the corresponding product baseline Major. Format Minors
evolve independently of product Minor and other formats. A format Minor does not
establish a minimum product Minor. Product bug fixes need not change formats;
prerelease executables may use unchanged formal formats.

Existing data retains its original format identity, rather than being rewritten
to match a new executable's Major. Prior-Major support is best effort. Published
format identifiers must not silently acquire different meanings.

### Supported formats and bounded refusal

**Operator-approved behavior: execute only with explicitly supported contracts;
otherwise refuse early and clearly within the necessary scope.** This governs
the new baseline and subsequent evolution, without restoring obligations waived
by the one-time original-development reset exception.

Version ordering or a shared Major alone is not evidence that an executable can
interpret a format. Support decisions must reflect implemented and verified
capabilities; do not ignore unknown required content and continue. Same-Major
backward compatibility obliges newer software to support earlier formal
contracts, not older software to predict later formats. This refusal policy
does not permit a new same-Major formal release to drop promised support.

For an unsupported format, refuse the affected operation before side effects.
Identify the domain and encountered version, explain the current executable's
support limitation, and provide actionable next steps, such as explicitly
obtaining a supporting Pactrun release. Do not implicitly update the executable,
convert or repair data, or stop the existing service.

Limit refusal to the scope that can be established safe. An unsupported Snapshot
bundle should prevent that import, not unrelated compatible Instance operations.
An unsupported management-root Persistence format prevents operations requiring
its interpretation; do not guess how to use the root merely to keep running.
Existing service access remains protected. Exact support matrices, Hook contract
selection, validation boundaries and diagnostic representations remain detailed
design work; this records behavior, not completed support machinery.

### Independent version domains

| Domain | Version method | Scope | Consumer |
| --- | --- | --- | --- |
| Pactrun product | Major.Minor.Patch[-prerelease] | Product behavior and compatibility | All users |
| Pack source | Major.Minor[-prerelease] | Authoring syntax, fields, defaults and declarations | Pack authors |
| Revision canonical format | Major.Minor[-prerelease] | Canonical content, runtime closure, encoding and identity calculation | Pactrun-managed |
| Hook Protocol | Major.Minor[-prerelease] | Transport, Sessions, data access, authority, submission, cancellation and completion | Hook authors |
| Snapshot content and integrity | Major.Minor[-prerelease] | Manifest, descriptors, encoding and integrity calculation | Pactrun-managed |
| Snapshot bundle | Major.Minor[-prerelease] | Transfer container and identification of contained Snapshot formats | Pactrun-managed |
| Pack distribution | Major.Minor[-prerelease] | Transfer container and identification of contained Revision formats | Pactrun-managed |
| CLI machine interface | Major.Minor[-prerelease] | One version for ordinary JSON results and event streams, including errors | Automation authors |
| Persistence | Major.Minor[-prerelease] | Storage representation, admissibility and read/write contract | Pactrun-managed |

There is one product version and eight format/protocol domains. CLI results and
events retain different message kinds, but share one compatibility version.
Users do not manually select every format.

### Initial format versions and formal-release prerequisite

All eight format/protocol domains start at **1.0-alpha.1**, alongside product
1.0.0-alpha.1. The shared starting point does not synchronize later releases:
change a domain's version only when its contract requires it. A product bug fix
or alpha-to-beta transition alone does not require changing format labels.
A structured version field uses a string, for example
`"format_version": "1.0-alpha.1"`; this example does not settle every domain's
field spelling or wire representation.

Before a formal Pactrun release, every format/protocol needed for normal use
must have a formal contract. For the first formal release, this covers all eight
domains, including Persistence. Normal creation and output use formal formats,
and Pack authors, Hook authors and automation consumers have formal interfaces.
If a required domain is still experimental, the product remains a prerelease.

The initial formal target is product 1.0.0 with each domain at 1.0 unless actual
contract evolution requires a different format number. Domains may formalize
independently before the product; later beta/rc builds may use formal formats.
Formalization requires contract, compatibility, identity-impact and acceptance
review, not merely deleting the prerelease suffix.

A formal product may retain compatible reading of prerelease-created data.
Its formal support contract must define the accepted representations and their
interpretation with verification evidence. This is not dependence on an undefined
experimental contract and does not require relabeling all existing objects.
Identify version markers participating in canonical bytes or integrity/digest
calculations before specifying promotion: a label change may change identity.
Preserve the approved acceptance and service-continuity policy rather than
silently rewriting data to match formal labels. Exact handling remains part of
the detailed design; the one-time original-development reset exception remains
separate.

### Responsibilities inherited from an enclosing contract

| Item | Version responsibility; no additional independent version |
| --- | --- |
| CLI commands, arguments and operational behavior | Product version |
| Error owner/code identity and meaning | Product compatibility; containing interface governs representation |
| Migration path selector | Product governs encoding and interpretation; necessary encoding discriminators are Pactrun-managed, not a separate user choice |
| Error taxonomy catalog file | Corresponding product source revision and verification tools; no independent release/compatibility version |
| Hook operation Sessions, bindings, authorities, paths and descriptors | Hook Protocol |
| Hook framing and handshake identification | Hook Protocol, not a separate evolution domain |
| Revision runtime closure, canonical encoding and hash rules | Revision canonical format |
| Snapshot canonical encoding and integrity rules | Snapshot content and integrity format |
| Shared path, identifier and encoding rules | Each owning format/protocol fixes its applicable rules |

This is a closed list of Pactrun-owned version responsibilities. Unlisted items
receive no additional version domain. Substructures inherit their owning
contract; implementation details are not independently versioned. State tokens,
object IDs, digests and author-owned display labels are not converted into format
versions. Third-party/tool-mandated version fields are outside this reset.

Before removing a redundant field, prove the enclosing contract uniquely
determines interpretation and is available wherever the value is consumed.
Self-contained values can still require encoding discriminators. Do not remove
hash domain separation, wire detection or file identification merely because
it contains an old-looking version label. Consolidate responsibility, not safety.

## Hook stability

Hook Protocol is the unified complete interaction contract, not just framing.
Helpers, wrappers and SDKs follow it rather than introducing another semantic API.

A Hook following a supported formal contract must not need changes for existing
operations because of a same-Major Minor/Patch update. This covers Snapshot and
Migration data access, including supplied paths/handles, source/target and
active/retained semantics, authority, lifetime, readiness, materialized-content
mapping, output submission, acknowledgments, completion and recovery behavior.

Pactrun maintains the Hook's selected supported view when internal formats change;
it does not force old Hooks to consume new Session shapes or unknown fields.
Hooks use supplied locators, not guessed private database/scratch layouts.
Service-owned payload formats remain the Pack/service's responsibility. Contract
authority does not imply an OS sandbox.

## Releases, installation and coexistence

- Formal and prerelease builds have comparable installation experience, artifact
  reliability and data-safety expectations; prereleases relax compatibility only.
- Use target-version prerelease labels such as 1.0.0-alpha.N or 2.0.0-beta.N,
  rather than requiring practical evaluation to remain at 0.1.0. The first new
  baseline product prerelease is **1.0.0-alpha.1**; all eight format domains start
  at **1.0-alpha.1** as defined above.
- GitHub Releases is the selected distribution mechanism, with alpha/beta/rc
  marked as prereleases. Shell/PowerShell scripts or
  other installer technology have not been selected.
- Selection uses the minimal model below. Maintaining Major 1 while developing
  Major 2 must be possible.
- Existing installs do not silently cross Major or switch formal to prerelease.
  Ordinary Pactrun operations do not update the executable implicitly.
- Formal/test installs coexist without overwriting each other or taking over
  the default command. Test use defaults to a separate management root; separate
  service resources are also needed for actual isolation.

### Installer responsibilities and data boundary

**Operator-approved direction: one installer manages installation, updates,
version switching and uninstallation. Pactrun itself does not self-update.**
The installer uses a CLI. Its implementation language, script/binary packaging
and exact command syntax remain open; a GUI is not required for this scope.

| Installer operation | Agreed behavior |
| --- | --- |
| Install | Install the executable and necessary components using the selected Major, prerelease eligibility or exact version; do not implicitly create Instances, take over services or convert data |
| Update | Update the explicitly selected installation under its saved release-selection policy without changing other coexisting installations |
| Switch version | Select the executable for subsequent launches, acquiring it first if needed; do not take over or force-terminate active Pactrun/Hook operations or restart services |
| Uninstall | Remove the selected program installation, not management roots, Inputs, Secrets, Snapshots or service data; do not implicitly stop services |

The installer manages program versions, not data compatibility. Pactrun checks
whether it can safely operate on the selected management root before performing
the requested management operation. Successfully selecting an older executable
does not establish that existing data can be downgraded; the confirmed support
and refusal policy still applies. The installer does not guess, move, convert or
delete management data to make a version selection work.

Keep formal and test installations distinguishable. Updating one does not
implicitly switch another, and installing a test version does not automatically
take over the default command. The installer handles default-executable selection
explicitly. Changing saved release eligibility alone remains distinct from an
explicit installation or version-switch operation.

Management-data cleanup is a separate, explicitly scoped operation, not a hidden
part of uninstallation. These are normal new-baseline installation rules; they
do not restore old-environment preservation obligations waived for the one-time
development-era reset. No installer implementation or host change is executed
by this decision.

### Re-downloadable CLI installer

**Operator-approved initial approach: obtain a newer installer by downloading
and running it again, rather than implementing installer self-update.**
Provide a stable official download entry point and convenient acquisition using
tools such as curl, alongside identifiable versioned installer artifacts.
Exact URLs, platform-specific commands and artifact verification mechanisms
remain to be designed; no download endpoint or installer is published here.

The installer is not resident and is not required for ordinary Pactrun execution.
A newer installer can recognize existing program installations and their saved
version selections without first uninstalling Pactrun. Keep installation records
small and limited to program-management information, not service data. If an
installer cannot interpret required release information or installation records,
stop clearly and direct the user to obtain a supporting installer; do not guess
or damage the existing installation.

Downloading or replacing the installer alone does not change installed Pactrun
versions, the default command or management data. Changes require an explicit
installer operation. Do not silently fetch and execute another installer to
update itself, and do not add a persistent updater for this initial scope.

Convenient downloading does not waive artifact verification. Design the usage
path around downloading, verifying and then executing the installer; convenience
must not depend on directly piping unchecked network content into a shell.

### Initial supported platforms

**Operator-approved initial release and verification scope: Windows x86-64 and
Linux x86-64.** macOS and ARM64, including Windows/Linux ARM64, are outside the
initial support commitment. This bounds release artifacts and acceptance
coverage; it does not deliberately prevent later ports. Add other combinations
when justified by demand and appropriate verification environments.

This is not a promise to support every Windows version or Linux environment.
Minimum OS versions, Linux runtime requirements and other platform prerequisites
must be established from the implementation, dependencies and verification
evidence in the detailed design. Platform approval is neither proof that release
artifacts have passed those checks nor authorization to publish them.

### Minimal release selection

| Selection | Explicit installation/update behavior |
| --- | --- |
| Formal only, selected Major | Select the newest formal release within that Major |
| Allow prereleases, selected Major | Select the newest version within that Major, including both formal releases and alpha/beta/rc |
| Exact version | Remain fixed until the user explicitly changes the selection |

Use version precedence, not upload time. Updates do not automatically downgrade
or cross Major. Changing selection alone does not replace the executable. With
no eligible formal release, report that none is available; never silently choose
a prerelease. Updates require an explicit user request, not background work or
ordinary Pactrun operations.

Allowing prereleases is a continuing choice within the selected Major: it can
follow 1.0.0-alpha.1 through beta/rc to 1.0.0 and later 1.1.0-alpha.1, each only
on explicit update. This replaces the earlier, unapproved target-specific
proposal to stop after one target becomes formal. Switching to formal-only
selection neither changes the executable immediately nor permits an automatic
downgrade.

Do not add nightly/daily releases, separate alpha/beta/rc channels, an additional
channel service, or per-target graduation/transfer state. This simplification
does not relax artifact verification, source consistency or service continuity.

### Release stages and prerelease data

| Stage | Agreed role |
| --- | --- |
| alpha | The new baseline is installable and usable; contracts can still be refined, without relaxing data safety or service continuity |
| beta | Intended formal functionality and contracts are substantially settled; focus on actual use, platform and compatibility verification |
| rc | A formal-release candidate with no planned contract changes; verify actual release artifacts and installation/update behavior |
| formal 1.0.0 | Complete the formal promise scope and acceptance checks, then obtain explicit publication approval |

Stages advance on readiness, not a preset date. Detailed measurable gates remain
to be designed. Stage changes do not themselves require data conversion or a
format-version change, and this decision does not relabel the current executable.

Accept prerelease-created data that fully conforms to the formal contracts;
do not require rebuilding solely because its producer was a prerelease. Where a
different representation can be supported safely, preserve it and use compatible
reading. This does not promise support for every experimental format. Otherwise,
refuse the affected new operation before side effects with actionable guidance,
preserving existing data and service access rather than stopping the old service.

Installing a formal release does not automatically move, merge, convert, rebind,
restart or redeploy a test environment. Explicitly choosing to operate on an
existing test environment still requires compatibility validation. Any necessary
migration is separately arranged with its impact explained, not bundled into
installation or promotion. Service continuity takes priority.

Installable prereleases address immediate availability, not formal stability or
guaranteed unchanged promotion of all test objects. Exact source/build evidence
is needed for prereleases as well as formal release candidates.

## One-time reset

**Operator-approved exception: prioritize a clean reset and remove historical
burden for this transition only.** This supersedes the earlier proposal to
preserve the old environment and arrange a compatible transfer, and overrides
the update-scope, compatibility, stability and service-continuity restrictions
above insofar as they would require preserving the original development-era
baseline during this reset.

- Old data, Packs, Hooks, management roots and deployments need not remain usable
  with the new baseline. Compatibility with original development-era versions
  is not an acceptance requirement for this transition.
- Maintaining old-environment stability and uninterrupted service is not a
  constraint on the necessary breaking reset. Do not retain historical machinery
  solely to satisfy those guarantees for the old baseline.
- Remove obsolete readers, codecs, schema-upgrade paths, compatibility branches,
  and tests or documentation promises that exist only to support retired
  development versions. Inventory dependencies before removal; retain mechanisms
  only when the new design itself needs them, not because they existed before.
- No converter, old/new coexistence mechanism or seamless management takeover
  is required solely for this transition. Old-environment preservation is no
  longer the selected reset direction.
- Establish a coherent new contract and verify it. The exception permits a
  breaking reset; it does not excuse defects in the new baseline or justify
  removing still-needed safety mechanisms under the label of historical cleanup.

The exception applies once, from the original development-era baseline to the
new baseline whose first product prerelease is 1.0.0-alpha.1. It is not an
exemption for the whole alpha phase or later updates. Subsequent evolution
follows the confirmed formal/prerelease compatibility policy, approved update
scope and service-continuity constraints above.

The design direction does not itself execute deletion of on-disk data or service
resources. Actual cleanup must identify its exact targets and scope before
execution and must not affect unrelated content. This record authorizes neither
an immediate destructive cleanup nor runtime implementation.

Next, inventory the old readers, codecs, schema paths, markers, fixtures, tests
and documentation obligations to remove or retain for the new contract. Define
the new representation and identity rules explicitly; do not mechanically reset
arbitrary IDs, state tokens or third-party versions. Exact reset mechanics and
old-data handling remain to be designed, without reinstating old compatibility
or service-continuity obligations as blockers. No reset is executed here.

## Architecture for future compatibility

**Operator-approved direction: remove historical support, not the ability to
honor future compatibility promises. Keep the design simple.** The one-time
reset retires original development-era implementations; it does not waive the
need for an architecture that can support subsequent contract evolution.

- Implement only the known new baseline now. Add another representation when
  a real contract requires it, not in anticipation of hypothetical versions.
- Concentrate version recognition, support decisions and decoding in a small
  number of explicit boundaries rather than scattering version conditions
  through product operations. Keep external representations separate from
  internal behavior, without erasing contract-specific defaults, meaning or
  identity merely to force everything into one model.
- Separate supported reading from writing choices. Reading an earlier
  representation does not authorize rewriting it, and a new capability does
  not require updating every existing object. Preserve immutable content and
  its applicable identity rules after the reset.
- Design ownership, coordination and recovery boundaries with future supported
  versions in mind from the first baseline. This does not select a new lock
  mechanism or promise arbitrary mixed-version concurrency or downgrade.
- Establish new-baseline vectors and behavioral tests as future compatibility
  evidence. Architectural intent alone is not proof of compatibility or safety.
- Do not build hypothetical codecs, a general conversion engine, dynamic plugin
  registration or a broad compatibility framework. Prefer the smallest typed
  boundaries that satisfy the confirmed contracts.

Shared functionality currently located in old-version modules may still be
needed by the new baseline. Consolidate that functionality before removing the
retired dispatch paths; old filenames are not a deletion criterion. Detailed
module organization, representations and acceptance evidence remain design and
implementation work, not completed mechanisms authorized by this record.

## Remaining discussion and design closure

| Topic | Required decision/output |
| --- | --- |
| Concrete representations | String version values are confirmed; specify field names/placement, suffix grammar, parsing/comparison, wire preamble, storage metadata and diagnostics |
| Support selection | Explicit-support and bounded pre-side-effect refusal behavior is confirmed; specify product/format support matrices, Hook selection, diagnostic representation, validation boundaries and whether declarations alone suffice |
| Compatibility architecture | Map the approved simple boundaries to current shared functionality and new-baseline tests; no speculative future codecs or framework |
| Same-Major persistence | Engineer and verify representation changes and cross-version lock/owner coordination within the approved scope; implement the confirmed explicit-version selection and safe-refusal policy without an arbitrary data-downgrade guarantee |
| Initial numbering and promotion | Product 1.0.0-alpha.1 and all eight formats 1.0-alpha.1 are confirmed; define measurable stage gates and canonical-byte/identity handling for promotion under the approved acceptance policy |
| Reset and existing deployments | Apply the approved one-time exception; inventory historical support for removal, define fresh-baseline/reset mechanics and exact cleanup targets without requiring old compatibility or continuity |
| Identifier consolidation | Error-owner naming, selector encoding/discriminators and hash domains; prove removal of redundant markers safe without adding independent domains |
| Installer/update UX | Re-downloadable CLI installer, convenient curl-style acquisition, Windows/Linux x86-64 and install/update/switch/uninstall boundaries are confirmed; specify OS/runtime prerequisites, script/binary packaging, download entry point and verification, command spelling, installation records/roots, management-root association, default selection and pinning mechanics |
| Release maintenance | Windows/Linux x86-64 is the initial platform scope; define minimum environments, supported Major maintenance policy/duration, artifact verification/signing, source/tag/build consistency and publication procedure |
| Formal release gate | All eight required domains must have formal contracts; define remaining formal promise scope, automated and actual-use evidence, real release-candidate checks and explicit publication approval |
| Spec/implementation handoff | Reconcile owning requirements and planning text; ordered E slices, acceptance matrix and source-qualified checks using the integrated prerequisite baseline |

Maintenance duration is distinct from backward compatibility. Version syntax
alone does not establish safe interpretation. No installer command, new manifest
requirement, automatic migration or release date is approved here.

## Authorization boundary

This is a documentation-only decision record, not authorization for runtime work,
new requirement/test IDs, version/schema changes, release workflows, deployments,
Git integration or publication. Preserve existing requirements/evidence until
the reviewed Spec transition. Document checks do not prove implemented
compatibility; full product and remote-build work are separate.
