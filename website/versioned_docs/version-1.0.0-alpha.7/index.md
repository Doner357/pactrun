---
title: Pactrun
slug: /
---

# Manage Packs and Instances

Pactrun installs immutable Pack revisions and manages long-lived Instances on
your local machine. Packs define Actions, Snapshots, Migrations, and Cleanup.

## Choose a starting point

| I want to… | Start here |
| --- | --- |
| Get Pactrun ready to run | [Install and verify Pactrun](./guides/installation.md) |
| Try an already verified executable | [First Instance](./introduction.md) |
| Install and operate a Pack | [User guides](./guides/index.md) |
| Write a Pack or Hook | [Pack author guide](./package-authors/index.md) |
| Look up commands and output | [Command reference](./pactrun-users/operations/command-and-output-reference.md) |
| Understand an exact product rule | [Specification](./spec/index.md) |

## Find a document

Use **Search** in the navigation bar to search commands, error text, concepts,
and requirement IDs. Search starts with current documents. Select **All states**
to include superseded compatibility pages. Filters and
queries can be shared through the search URL.

Users can stay in the [user reference](./pactrun-users/reference/index.md).
Authors can look up [field values and defaults](./package-authors/reference/pack-fields.md)
or [service and Migration fields](./package-authors/reference/service-fields.md)
without leaving their section. Advanced reader references stay synchronized with
the product rules; their maintainer sources are optional.

## Before using managed data

Run only Packs whose Hooks you trust. Hooks execute host programs and can affect
service data. Use a separate storage root for experiments, inspect plans, and
keep independent backups of important service data.

## Documentation sources

English Markdown supplies both this website and the text interface.
[Spec](./spec/index.md) defines product behavior; guides explain how to use it.
Use the edition selector for the executable version you are running.
