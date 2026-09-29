---
title: Packages, Revisions, and Instances
---

# Packages, Revisions, and Instances

A **Package** identifies one authored lineage. A **Revision** identifies immutable
content in that lineage. An **Instance** is a long-lived local object bound to an
installed Revision and configured with its own Inputs.

## From source to Instance

1. The author provides Pack source or a supported distribution with an explicit Package identity.
2. Installation validates the definition, acquires its runtime files, and computes
   or verifies the exact Revision identity.
3. Creating an Instance selects an installed Revision.
4. Subsequent operations use that installed Revision, even if the source folder changes.

Editing a Pack directory does not update an existing Instance. Install the new
source, then use an authored [Migration path](../operations/snapshots-migrations-and-recovery.md#migrate-to-a-new-revision)
where appropriate. A new Revision by itself does not prove a safe transition.

## References and names

Use an exact reference for reproducible selection. Labels and aliases are useful
lookup names; they are not substitutes for immutable identity. Inspect the
resolved Revision before a consequential operation.

Human object listings can display usable unique ID prefixes. Authored names and
state-version tokens require their complete values. Use `--no-trunc` on supported
lists when recording exact identities.

## What lives independently

A source directory, installed Revision, live Instance, Run record, Snapshot, and
service resource have different lifetimes. Removing one does not imply removal
of the others. See [retirement](../../guides/retirement.md) before deleting data.

More detail: [User reference](../reference/index.md).

<details>
<summary>Maintainer sources (optional)</summary>

Contracts: [identity](../../spec/foundations/identity-and-state.md),
[Instances](../../spec/behavior/packages-revisions-and-instances.md), and
[ID selectors](../../spec/contracts/cli-id-selectors.md).

</details>
