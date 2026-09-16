---
title: Product Versioning and Compatibility
---

# Product versioning and compatibility

**Status: Approved formal-release compatibility design; implementation and baseline reset pending.**

This page owns the approved versioning direction for the first formal release
and its successors. It does not claim that release-version requirements or a
SemVer compatibility mechanism are implemented today. Existing format
dispatchers, codecs and storage upgrades remain governed by their current
contracts until the separately implemented pre-release reset.

<!-- spec-navigation:start -->
## Reading map (informative)

Read [independent version domains](./resources-and-versioning.md#pr-req-0077---separate-version-domains)
and [identity and state](./identity-and-state.md) alongside this policy.
[Release readiness](../../development/release-readiness.md) owns work planning,
acceptance gates and scheduling, not additional product semantics.
<!-- spec-navigation:end -->

## Terms and applicability

Product Major means the Major component of the Pactrun software version, not
the integer of an individual format or persistence schema. These domains remain
independent. A format number alone is not a minimum Pactrun software version.

Pack authors directly use authoring contracts; Hook authors also use the wire
protocol. Canonical Revision and Snapshot representations are normally generated
by software, but their observable identities and interchange meaning are still
contracts. SQLite layout and crate-private implementation types are not public
programming interfaces. Internal representation freedom does not waive the
obligation to preserve the meaning of managed objects.

The same-Major formal promise starts with product Major 1; Major 0 builds do not
receive it merely by sharing the label 0.1.0. The first formal product version
is 1.0.0. Development-era means the retired pre-reset generation, not every
artifact ever produced by a 0.1.0 executable. PR-REQ-0332 retires that old
generation; PR-REQ-0333 governs promotion of the reorganized new baseline that
was tested while the software still reported 0.1.0.

This approval does not authorize ordinary feature work to reinterpret existing
Frozen bytes or silently change current data; the reset is a distinct future
work item.

### PR-REQ-0329 - Product Major compatibility boundary

Pactrun product versions MUST follow Semantic Versioning and use the product
Major as the formal external-compatibility boundary.
Within one Major, newer software MUST continue to install and use Packs valid
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
minor release. A new Major does not promise compatibility with a previous
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

Compatibility MUST cover both installing an older Pack and continuing to use
already persisted Instances and Revisions. A supported explicit persistence
upgrade may change representation, but MUST preserve domain identity, references
and unresolved execution/recovery obligations under PR-REQ-0078. It MUST NOT
reidentify or reinterpret an existing object merely because the tool changed.
Canonical identity/integrity encodings are not freely mutable internal layout.

This does not promise that an old binary can write a newer storage schema, or
that every schema can be opened without an explicit upgrade. Exact supported
upgrade paths remain an owning persistence-contract decision.

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

**Verification: Pending automated coverage.**

### PR-REQ-0332 - One-time pre-release baseline reset

The approved pre-release reset MUST establish a distinguishable new baseline
without backward compatibility for development-era formats or data. The
shipping program MUST NOT retain development-only readers, migration chains or
compatibility paths merely to continue those experimental stores or artifacts.
Retired designs and experimental lessons may remain in history, separate from
the active product specification and shipping implementation.

The new baseline MUST distinguish old development data reliably even when
numeric format labels are reused. Unsupported old data MUST be refused rather
than guessed, silently adopted, overwritten or automatically deleted. A minimal
generation/format rejection check is not a legacy compatibility implementation.
The exact marker, namespace and new format encodings are not selected here.

Removing compatibility does not grant authority to destroy service-owned live
state. Actual disposal of experimental stores is a separate explicit operation,
not an incidental effect of opening them with the new program.

**Verification: Pending automated coverage.**

### PR-REQ-0333 - Development and formal-release baseline separation

Pactrun MUST remain at product version 0.1.0 during the planned pre-release
versioning work, reset and internal testing. Completing the reset MUST NOT
automatically publish or label the product 1.0.0. Internal test artifacts and
results MUST identify the exact build/source and applicable baseline; the
shared label 0.1.0 is not a unique build or data-generation identity.

The explicitly approved formal release changes the software version to 1.0.0
and validates the actual release artifact. Changing that software version alone
MUST NOT perform another format reset, change canonical identity meaning or
reidentify existing objects in the validated new baseline. This is not a promise
to accept pre-reset development data that also carried version 0.1.0.

Version checks MUST use the actual product version; an internal build MUST NOT
pretend that 0.1.0 satisfies a declared 1.x requirement. Compatibility logic may
be tested with controlled version scenarios, but the actual 1.0.0 candidate also
requires verification before publication. No runtime bypass or override API is
defined by this testing obligation.

**Verification: Pending automated coverage.**

## Verification status

The requirements above are approved design, not implemented release governance.
Documentation checks only establish placement, scope, links and honest status.
Existing V1/V2 dispatch and V7 upgrade tests do not certify these future release
commitments. The versioning and reset work must add direct automated evidence
before claiming their implementation complete.
