---
title: Manage installed Packs
---

# Manage installed Packs

Use this after [installing a Pack](./use-pack.md). Packages and Revisions have
stable IDs; local names are optional conveniences you control in the selected
Store. Authors supply descriptions, not your local names.

## Find an installed Revision

```text
pactrun revision list
pactrun revision show app:initial
```

The list is ordered newest-installed first. Unknown installation times appear
last. Names are shown when they form an unambiguous reference; otherwise Pactrun
uses IDs. `--no-trunc` shows complete identity references. Copy the continuation
command when another page is available; its cursor remains usable if the last
row on the previous page has since been deleted.

## Set, change, or remove names

Choose a Package ID and Revision ID from the list, replacing the placeholders:

```text
pactrun package rename <package-id> app
pactrun revision rename app:<revision-id> initial
```

Each Package and Revision has at most one local name. Names contain 1 to 31
ASCII letters, digits, hyphens, underscores or periods, starting with a letter
or digit. They are case-sensitive. Package names are unique in the Store;
Revision names are unique within their Package. A conflict is refused, not
replaced automatically.

```text
pactrun package rename app web
pactrun revision rename web:initial stable
pactrun revision show web:stable
```

Names do not change identity or update existing Instances. `stable` and `latest`
are ordinary names, not automatic update channels. To remove a name without
removing data:

```text
pactrun revision unname web:stable
pactrun package unname web
```

Use IDs again after removing names. Installation may set names with
`--package-name` and `--revision-name`, but does not silently rename an already
named object. Repeated installation preserves the recorded installation time;
deleting and later reinstalling a Revision records a new time.

## Record a local note or assessment

Notes and trust assessments do not travel with an exported Pack. Choose a current
reference from `revision list` for `<revision>`:

```text
pactrun revision note set <revision> --value "Local evaluation" --expect-absent
pactrun revision trust set <revision> trusted --expect-absent
pactrun revision metadata show <revision>
```

Use `--expect` with the exact previous value when replacing an existing note or
assessment. A conflict requires reinspection, not a forced overwrite. Trust is
a local annotation: `trusted` is not publisher authentication, and `distrusted`
does not independently prevent execution.

## Export or remove a Revision

```text
pactrun revision export <revision> --output ./saved-pack --include-portable-metadata
pactrun revision delete <revision>
```

Export creates `saved-pack.pack`; `.pack` is always appended to the output base
path. The archive is unencrypted and can contain sensitive authored content.
Portable metadata contains descriptions and provenance, not local names, notes,
trust assessments or installation times. Existing destinations are not replaced.

Deletion is subject to references and lifecycle constraints; it does not delete
Instances or run service Cleanup. See [retirement](./retirement.md) for those
operations and [data locations](./data-location.md) for Store selection.
