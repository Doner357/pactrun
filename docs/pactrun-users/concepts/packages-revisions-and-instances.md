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

Users can give a Package a local name and each of its Revisions a name within
that Package. A reference such as `demo:stable` selects the matching installed
Revision. Names are case-sensitive and can be changed with the
[naming commands](../../guides/pack-management.md); an existing Instance keeps
its exact Revision when a name changes.

Use the full `<package-id>:<revision-digest>` pair for reproducible selection,
without the `sha256:` prefix on the digest. Each component can also be a local
name or an eligible ID prefix, provided the complete reference identifies one
Revision. Inspect the resolved identity before a consequential operation.

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

Contracts: [identity](../../spec/packages/identity.md),
[Instances](../../spec/core/objects.md), and
[ID selectors](../../spec/interfaces/id-selectors.md).

</details>
