---
title: Documentation edition 1 - review
slug: /development/documentation-edition-1
---

# Documentation edition 1

**Historical record.** This records the earlier partial migration. The
[current migration review](../spec-migration-review.md) and [Spec map](../../spec/index.md)
supersede its current-location and guide-completion statements. The concept
sample described here has since been returned to a usage-guide placeholder.

**Status: First review edition; partial documentation migration, not a product release.**

## What to review

1. Read [the concept guide](../../pactrun-users/concepts/packages-revisions-and-instances.md).
   Can you explain the difference between installing a Revision and creating an
   Instance without reading internal contracts?
2. Open [the specification](../../spec/behavior/packages-revisions-and-instances.md).
   Does the explanation match your intent? Existing exact rules and verification
   declarations follow the introduction without semantic changes.
3. Read [the authority map](../../spec/index.md). Is it clear which topics moved,
   which remain in Developer reference, and which capabilities are deferred?
4. Use the site footer's text-document link to inspect the separate tool entry.
   Agent task instructions should not appear in normal HTML navigation.

## Migration ledger

| Material | Edition 1 treatment |
| --- | --- |
| Resource-model specification | Moved from Developer product-behavior to spec/behavior; original body retained apart from relative links |
| Requirements 0086-0089 and 0261-0263 | IDs, rule text, and verification declarations preserved |
| Existing tests 0072, 0078, and 0079 | Existing code and requirement relationships unchanged |
| Pending declarations on 0086-0089 | Preserved, not reclassified as covered or closed |
| Unnumbered resource-model, incomplete-Instance, ServiceStorage, and Package-scope prose | Preserved with original scope and future-runtime qualifications |
| Other normative Developer topics | Still authoritative; not silently archived or merged |
| Old resource-model URL | Informative forwarding page with compatibility requirement anchors |
| User resource-model placeholder | Replaced with an early concept guide, not a command tutorial |

The existing broad pending declarations may coexist with narrower implemented
M2 requirements. This migration preserves that evidence rather than assuming
that a related test automatically closes the broader requirement.

## Governance changes

The specification is no longer required to live only under a Developer label.
Early guides for implemented, verified capabilities are allowed before release,
with explicit development status. General guides do not expose internal test
traceability. These changes affect documentation policy, not product behavior.

## Verification scope

The traceability and error-taxonomy scanners share the same authoritative roots.
A new engineering regression test protects those roots; it does not receive a
product requirement/test ID. The existing scanner's successful result proves
reference consistency, not complete semantic coverage or runtime availability.

The website build runs text-publication tests and produces HTML and Markdown
from the same document inputs. Text publication carries a deterministic source
digest, preserves contract bodies, validates file links, and replaces only its
own output directory. Agent source pages are excluded from HTML routes. Current
text exports retain Markdown admonition syntax rather than dropping warnings.

Final execution results are reported with this task; this page alone is not a
claim that CI or every platform-specific test has passed.

## Not included

- Full migration of all normative topics or completion of all tutorials.
- Product runtime, CLI, protocol, schema, format, or test-vector changes.
- New stable agent APIs or assumptions that every agent discovers the entry.
- A tracked AGENTS.md, personal profile changes, commits, pushes, or deployment.
- Reclassification of Candidate, Frozen, internal, or deferred contracts.

The text export runs inside the normal website build. Publication callers do
not need a separate export command to include the new text artifacts.
