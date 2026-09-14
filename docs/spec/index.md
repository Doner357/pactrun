---
title: Pactrun Specification
slug: /spec
---

# Pactrun Specification

**Authority:** English normative text under this Spec tree defines Pactrun's
product behavior and required architectural boundaries. Reading aids, examples,
and this map explain the contracts; they do not add requirements.

## Read in layers

1. [Foundations](./foundations/index.md): scope, ownership, identity, and lifetimes.
2. [Observable behavior](./behavior/index.md): what operations expose and guarantee.
3. [Authoring and format contracts](./contracts/index.md): Pack, Hook, wire, errors,
   and exact immutable representations.
4. [Execution and recovery](./execution/index.md): acceptance, concurrency, failure,
   and durable recovery boundaries.
5. [Internal persistence](./persistence/index.md): current schema and earlier
   compatibility contracts, not a public database API.

Use the [shared vocabulary](./glossary.md) to locate a term's owner and the
[contract catalog](./catalog.md) to find an exact page and its original status.
For development work, use the [task reading paths](../development/reading-paths.md)
instead of loading every specification at once.

## One authority, different kinds of evidence

| Material | Role |
| --- | --- |
| Normative Spec text, including unnumbered architecture constraints | Defines product promises and prohibitions |
| PR-REQ identifiers | Stable references to independently verifiable requirements |
| PR-TEST and verification declarations | Evidence relationships, not replacement definitions |
| Development guides and design syntheses | Explain implementation workflow and link to owning rules |
| Roadmap and implementation records | Work order and reported implementation progress |
| Compatibility pages at old paths | Forward old links; contain no independent rules |
| Generated HTML and text copies | Publish the same source, never a second specification |
| User and Pack author guides | Reserved for later usage documentation |

Requirement-bearing documents live here, including new milestone contracts. Non-normative
syntheses and planning baselines live under Development. A milestone-specific
page that actually contains normative rules, such as the M4 execution boundary,
remains in Spec rather than being archived because of its date.

## Status is not one switch

Preserve three separate questions when reading a contract:

- Does this text define a rule, or only explain one?
- Is its format Frozen, Candidate, or internal/non-public?
- Has the relevant runtime been implemented and verified?

A Frozen protocol can describe a context whose runtime is not yet implemented.
An internal schema does not become a public API by appearing on this website.
Pending verification declarations are not removed by this document migration.
See [implementation status and remaining decisions](../development/next-milestone.md).

## Changes and conflicts

Moving or clarifying a rule does not authorize changing its meaning, reusing its
ID, weakening its tests, or changing Frozen bytes. Exact rule headings and
verification markers remain machine-readable. No unnamed normative constraint
may be dropped merely because the ID scanner cannot see it.

Follow [development and verification policy](../development/development-and-verification.md)
when a contract is unclear. Record affected rules and observable consequences;
do not let a guide, a historical summary, or the current code silently decide a
conflict. This migration does not establish new agent runtime or CLI promises.
