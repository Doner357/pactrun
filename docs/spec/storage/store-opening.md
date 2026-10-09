---
title: Store Preparation and Upgrades
---

# Store Preparation and Upgrades

Pactrun prepares new Stores and upgrades supported existing Stores before the
requested operation. These maintenance steps preserve managed object identity.

### PR-REQ-0373 - Store opening and supported catalog upgrades

Opening a supported earlier Store MUST upgrade its management schema before
dispatching the requested operation. Help and version inspection do not open a
Store. Schema upgrade is Store maintenance, not permission for a query to mutate
Instances, execute Hooks, reconcile Runs, or inspect service payloads.

An upgrade MUST validate the exact source schema and ownership, serialize with
writers, refuse live or unprobeable old writer admissions, and publish the new
schema and version atomically. Concurrent openers observe a complete supported
schema, never a partially upgraded catalog. Unrecognized or damaged schemas MUST
remain untouched. There is no implicit downgrade.

Local names can be converted only without information loss. A single valid old
alias becomes its Revision's local name. Multiple aliases on one Revision or
names outside the supported grammar MUST block conversion before schema changes,
with an explanation of the required correction. Authors' labels MUST NOT become
user names. Package names remain unset and unavailable installation times remain
unknown. Existing Revision and Instance identities remain unchanged.

The exact alpha.1 and alpha.2 schemas upgrade to Persistence `1.0-alpha.3`.
Existing failure text remains unchanged. An upgrade MUST NOT parse old messages
to manufacture structured historical causes. Such a cause is unavailable for
older Runs unless it was recorded as typed evidence at failure time.

**Verification: PR-TEST-0687.**

**Verification: PR-TEST-0665, PR-TEST-0666, PR-TEST-0667, PR-TEST-0668.**

### PR-REQ-0374 - Automatic preparation of new Stores

An operation opening a Store for writing MUST prepare a missing Store without
requiring an initialization command or manual internal directories. Preparation
MUST qualify the storage filesystem, build a private complete Store, and publish
it without replacing any existing destination. Concurrent preparation MUST NOT
overwrite an already published Store. Existing files, unsupported stores and
incomplete directory trees MUST be refused rather than repaired or adopted.
Help and version inspection MUST NOT prepare a Store. Read-only opening of a
missing Store MUST NOT create its directories. Global collection queries return
their normal empty result when the selected Store does not exist. Queries for a
specific object still fail when it cannot be found. An existing but unreadable or
incomplete Store MUST NOT be presented as an empty Store.

**Verification: PR-TEST-0671, PR-TEST-0672, PR-TEST-0673, PR-TEST-0674, PR-TEST-0677.**
