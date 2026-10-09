---
title: Product and Format Compatibility
---

# Product and Format Compatibility

Product versions identify the executable. Format versions identify the contracts
used to read and write data or exchange messages. Updating one does not
automatically change the others.

Current readers and writers are listed in [format domains](./format-domains.md). Formal
same-Major compatibility applies after formal release; prereleases do not
acquire that guarantee merely by sharing a Major number.

[Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html) supplies product-version
terminology and precedence rules. [Conan's client stability policy](https://docs.conan.io/2/introduction.html#stable)
is a reference for keeping ordinary updates within a stable Major contract.
Pactrun's formal same-Major commitment includes continued use of existing Packs,
managed objects and services; its exact scope is defined by the requirements below.

## Version identifiers

Product versions use Major.Minor.Patch[-prerelease]; formats use string
Major.Minor[-prerelease]. Published prereleases use alpha.N, beta.N or rc.N,
positive canonical decimal N. Format Major identifies its product baseline Major;
format Minors evolve independently. Ordering and shared Major do not prove support.

### PR-REQ-0329 - Product Major compatibility boundary

Pactrun product versions MUST follow Semantic Versioning and use the product
Major as the formal external-compatibility boundary.
Alpha products MAY replace prerelease interfaces rather than retain every earlier
alpha contract. Affected format versions and supported readers MUST be explicit;
this does not authorize silent data deletion, identity changes or service actions.
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

**Verification: PR-TEST-0637 (source-qualified baseline continuity; future formal-release evidence remains a promotion gate).**

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

**Verification: PR-TEST-0637 (identity, installed data and live-operation continuity in the native-manager matrix).**

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

The narrow SQLite read-coordination allowance in
[PR-REQ-0078](./format-domains.md#pr-req-0078---persistence-migrations)
permits inspection only, not operation admission or mutation of existing data.

A product in Major 1 may, for example, report that a Pack requires a later
1.x capability. That does not imply Major 2 support. Support diagnostics must
come from an actual contract requirement; this policy does not add a manifest
field, capability registry, negotiation mechanism, or inferred minimum version.

**Verification: PR-TEST-0618, PR-TEST-0619, PR-TEST-0625, PR-TEST-0632, PR-TEST-0633, PR-TEST-0634, PR-TEST-0638.**

<a id="pr-req-0332---one-time-pre-release-baseline-reset" />

### PR-REQ-0332 - Contract-based data acceptance {#pr-req-0332---first-formal-baseline-and-origin-independent-acceptance}

Data acceptance MUST depend on full conformance to supported contracts, not on
creation time, development provenance, or the historical authoring-tool label.
Fully conforming data MUST NOT be rejected solely because it was produced
during development. Pactrun MUST NOT require an additional generation marker
just to distinguish such data. Normal format identifiers and explicit
capability/tool requirements remain applicable.

Conformance MUST include identity, encoding, reference, invariant, and semantic
checks. Matching field names, parseability, or numeric labels alone are
insufficient. Nonconforming data MUST be refused without format guessing,
silent repair or conversion, overwrite, or automatic deletion. Reusing a
version number MUST NOT make incompatible content valid.

The shipping program MUST NOT retain readers, migration chains, dispatch
branches, or special cases solely for unsupported development contracts.
Implementations needed by supported contracts remain valid regardless of when
they were written. Supported published contracts and their regression tests
are unaffected by this exclusion.

Removing an unsupported reader MUST NOT authorize destruction of service-owned
state. Disposal remains a separate explicit operation.

**Verification: PR-TEST-0637, PR-TEST-0638 (origin-independent reuse and refusal of unsupported contracts without changing existing objects).**

### PR-REQ-0333 - Prerelease promotion and release selection {#pr-req-0333---development-and-formal-release-baseline-separation}

The eight current writer/default format contracts MUST use `1.0-alpha.1`.
Prerelease labels identify candidate versions and MUST NOT be presented as
formal releases. Completing implementation or verification MUST NOT
automatically publish or relabel a candidate as formal `1.0.0`. A software-label
change MUST NOT reset formats or rewrite existing object identities.

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

**Verification: PR-TEST-0620, PR-TEST-0626, PR-TEST-0627, PR-TEST-0628, PR-TEST-0629, PR-TEST-0630, PR-TEST-0631, PR-TEST-0635, PR-TEST-0636, PR-TEST-0637, PR-TEST-0640, PR-TEST-0641.**
