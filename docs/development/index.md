---
title: Development
slug: /development
---

# Developing Pactrun

**Status: Informative development entry.** This section helps implementers work
from the specification; it does not replace the product rules or an approved
milestone scope.

## Start a development task

1. Read [the Spec map](../spec/index.md) and, if needed, the
   [developer product overview](./product-overview.md).
2. Choose a [task reading path](./reading-paths.md) rather than reading all files.
3. Check [the current baseline and next milestone](./next-milestone.md).
4. Follow [development and verification policy](./development-and-verification.md).
5. Report changed behavior, rule/test evidence, verification results, and limits.

The [roadmap](./implementation-roadmap.md) controls work order. The Spec controls
product semantics. A design summary may help locate a rule but cannot override
it. The [M4 closeout](./m4-implementation-status.md) records integrated work, not
a release or a fresh claim that every test has just run.

The next design entry is the [M6 bounded baseline](./design-notes/m6-recovery-implementation-baseline.md)
and [ServiceStorage staged alignment](./design-notes/service-storage-staged-design-alignment.md).
The approved work order places ServiceStorage in M6.5 between M6 recovery and
M7 Cleanup/deletion. Bounded M6 S0-S4 is approved; its
[implementation record](./m6-implementation-status.md) tracks delivery gates.
ServiceStorage implementation approval remains separate.

## Change discipline

For behavior-preserving work, choose local implementation details without
inventing new product concepts. For a real conflict, identify the owning rules,
observable consequences, and proposed decision before changing semantics.
A passing test is evidence of an exercised contract, not permission to weaken it.

The [migration review](./spec-migration-review.md) records the document move and
its preservation checks. User, Pack-author, and operation-oriented agent guides
remain reserved for a later documentation phase.

Repository instructions and the task determine execution permissions. Personal
communication and machine preferences are not part of the Pactrun Spec.
