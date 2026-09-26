---
title: Product Versioning and Compatibility
---

# Product versioning and compatibility

> **Pending E policy revision:** The operator-approved
> [E redesign decisions](../../development/design-notes/e-versioning-redesign-decisions.md)
> replace conflicting future-design assumptions here, including mandatory
> user-driven same-Major storage upgrades and the 0.1.0-only evaluation workflow.
> Numbered requirements remain below for explicit reconciliation before
> implementation. Current runtime formats are unchanged; new compatibility
> mechanisms are not claimed to be implemented.

**Status: Approved formal-release compatibility design; implementation and baseline consolidation pending.**

This page owns the approved versioning direction for the first formal release
and its successors. It does not claim that release-version requirements or a
SemVer compatibility mechanism are implemented today. Existing format
dispatchers, codecs and storage upgrades remain governed by their current
contracts until the separately implemented first-formal-baseline consolidation.

<!-- spec-navigation:start -->
## Reading map (informative)

Read [independent version domains](./resources-and-versioning.md#pr-req-0077---separate-version-domains)
and [identity and state](./identity-and-state.md) alongside this policy.
[Release readiness](../../development/release-readiness.md) owns work planning,
acceptance gates and scheduling, not additional product semantics.
<!-- spec-navigation:end -->

## Terms and applicability

E's 2026-09-26 approved transition establishes a clean product 1.0.0-alpha.1
baseline and eight format/protocol domains at 1.0-alpha.1.
Development iterations are not earlier formal product releases. The one-time exception
removes original development-era compatibility obligations; it is not an exemption
for all future alpha updates. Their historical owning contracts are retired only
through the explicit E Spec transition, not by silently changing Frozen bytes.

Product versions use Major.Minor.Patch[-prerelease]; formats use string
Major.Minor[-prerelease]. Published prereleases use alpha.N, beta.N or rc.N,
positive canonical decimal N. Format Major identifies its product baseline Major;
format Minors evolve independently. Ordering and shared Major do not prove support.

### PR-REQ-0329 - Product Major compatibility boundary

Pactrun product versions MUST follow Semantic Versioning and use the product
Major as the formal external-compatibility boundary.
Within one formal Major, newer software MUST continue to install and use Packs valid
under earlier published external contracts of that Major, with their existing
meaning and applicable platform/execution prerequisites preserved. Minor and
patch releases MUST NOT require a previously valid Pack to be rewritten merely
because the tool was upgraded.

External-format evolution within a Major MUST be a non-breaking extension.
New capabilities MUST NOT invalidate an older Pack because it does not declare
them. Existing valid fields, omission behavior, defaults and Hook semantics
MUST NOT silently change. Syntactic acceptance without semantic compatibility
does not satisfy this requirement.

An older tool is not required to implement capabilities introduced by a later
minor release. Prerelease and cross-Major interpretation is best effort, never
permission for destructive repair or service interference. A new Major does not promise compatibility with a previous
Major's external formats. Breaking Major transitions are exceptional deliberate
decisions, not a routine shortcut for internal refactoring. Absence of a
cross-Major promise does not require deliberately rejecting content that an
explicitly supported contract can still interpret correctly.

**Verification: Pending automated coverage.**

### PR-REQ-0330 - Internal evolution preserves supported external meaning

Within a formal product Major, the internal model MUST remain capable of representing
and executing the meaning of valid older external contracts of that Major. Conversion
MUST NOT lose old capabilities or require an author to supply new information
that was not required by the old contract. Internal schema or implementation
changes MUST NOT, by themselves, force an external-format change.

Compatibility MUST cover installation and existing objects. Updating within a
formal Major MUST NOT require user-triggered storage upgrades, object conversion,
recreation/rebinding, service pause, resource reopening, restart or redeployment.
Internal representation changes MUST preserve identity, content, relationships,
bindings, lifetimes, active ownership, unresolved Runs and recovery obligations.
Reading support does not grant permission to rewrite immutable content.

Do not seize ownership, force unlock or interrupt an operation to enable an update.
If the design cannot meet the same-Major promise, revise or defer that design;
safe refusal alone does not discharge the compatibility obligation. Explicitly
selecting an older executable does not guarantee data downgrade or rollback.

**Verification: Pending automated coverage.**

### PR-REQ-0331 - Explicit support selection and actionable refusal

Pactrun MUST distinguish software-version or capability requirements from format
identifiers and from the version of the tool that happened to author a Pack.
Using a newer authoring tool alone MUST NOT manufacture a requirement for that
tool version when the resulting contract and capabilities do not require it.

An unsupported external contract or declared capability/tool requirement MUST
be refused before the affected operation's side effects, with an actionable
diagnostic about the known requirement. The tool MUST NOT guess another
format, ignore required new content and continue, or invent a minimum product
version that the available declaration does not establish.

For example, a tool in product Major 1 may report that a Pack requires a later
1.x capability. That is not an unbounded promise that Major 2 will accept it.
This example defines no literal manifest field, version-range syntax, capability
registry, implicit negotiation or introduced-version lookup table. Their exact
representation and validation are pending versioning-mechanism design.

**Verification: PR-TEST-0618, PR-TEST-0619, PR-TEST-0625, PR-TEST-0632, PR-TEST-0633, PR-TEST-0634; operation-boundary coverage pending E integration.**

<a id="pr-req-0332---one-time-pre-release-baseline-reset" />

### PR-REQ-0332 - First formal baseline and origin-independent acceptance

Consolidation MUST establish the first formal product baseline, not a successor
required to support development iterations as earlier formal releases. The
shipping program MUST NOT retain readers, migration chains, version dispatch or
special cases solely to support superseded development contracts. Implementations
still required by the final contracts are not legacy merely because they were
originally written during development.

Data acceptance MUST depend on full conformance to the current supported formal
contracts, not on whether the data was produced during development. Fully
conforming data MUST NOT be rejected solely because of its development provenance,
creation time or historical authoring-tool label. Such acceptance is ordinary
current-contract validation, not a backward-compatibility path. Pactrun MUST NOT
require a development-generation marker or discriminator merely to identify and
exclude development data. This does not remove normal format identifiers or
explicit capability/tool requirements under PR-REQ-0331.

Conformance MUST include applicable identity, encoding, reference, invariant and
semantic checks; matching field names, parseability or numeric version labels
alone are insufficient. Nonconforming data MUST be refused without legacy
guessing, silent repair/conversion, overwrite or automatic deletion. Reusing a
number does not authorize reinterpreting incompatible content as valid.

Development-only Freeze versions, iteration records and superseded decisions
MUST remain historical rather than required shipping compatibility machinery.
Their disciplined verification/evolution practices MAY be retained in development
history as implementation references. This does not prohibit Frozen formal
contracts, retained behavioral regression tests or accurate historical timestamps.
Removing compatibility MUST NOT authorize destruction of service-owned state.
Actual disposal remains a separate explicit operation.

Informative rationale: the first formal product has no earlier formal release
to emulate. Current-contract validation permits naturally conforming data while
avoiding both experimental compatibility debt and an artificial origin barrier.
This replaces the previous generation-rejection design; no runtime behavior is
changed merely by recording the revised rule.

**Verification: Pending automated coverage.**

### PR-REQ-0333 - Development and formal-release baseline separation

The initial new product baseline MUST be 1.0.0-alpha.1, with all eight domains at
1.0-alpha.1. Prerelease labels identify real candidate versions; they MUST NOT
pretend to be formal releases. Local milestone completion MUST NOT automatically
publish, push or relabel a candidate as formal 1.0.0. A software-label change
MUST NOT perform another format reset or rewrite existing object identities.

Before a formal product release, all eight required writer/default contracts MUST
be formal and independently verified. Beta/rc software MAY use formal formats.
Promotion MUST preserve existing conforming objects without relabeling canonical
bytes, identities or bindings merely to remove a suffix. Accepted prerelease
representations require explicit compatible readers and evidence, not provenance
rejection, guessed compatibility or forced rebuilding.

Release selection MUST use product-version precedence, not upload time. Major
selection distinguishes formal-only from formal-plus-prerelease eligibility;
exact selection stays fixed until explicitly changed. Ordinary updates MUST NOT
downgrade or cross Major. No formal candidate MUST NOT fall back to prerelease.
Changing saved eligibility alone MUST NOT replace the executable. Switching an
executable and validating its data compatibility remain separate responsibilities.

**Verification: PR-TEST-0620, PR-TEST-0626, PR-TEST-0627, PR-TEST-0628, PR-TEST-0629, PR-TEST-0630, PR-TEST-0631, PR-TEST-0635, PR-TEST-0636; remaining artifact/promotion coverage pending E integration.**

## Verification status

The requirements above are approved design, not implemented release governance.
Documentation checks only establish placement, scope, links and honest status.
Existing V1/V2 dispatch and V7 upgrade tests do not certify these future release
commitments. The versioning and consolidation work must add direct automated
evidence before claiming their implementation complete.
