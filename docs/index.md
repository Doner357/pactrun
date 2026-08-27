---
title: Pactrun
slug: /
---

# Pactrun

Pactrun is a local-first tool for installing immutable package revisions,
managing long-lived service instances, and running package-defined operations.
It gives package authors a consistent runtime contract while leaving
service-specific behavior to trusted Hooks.

:::warning Implementation status

The repository currently contains a compile-oriented Rust modular-monolith
scaffold. The Pactrun Developer documentation describes the normative product
contract; it does not imply that the documented behavior is implemented yet.
Pactrun User and Package Author documentation is reserved for completion after
the initial release.

:::

## Start here

- [Introduction](./introduction.md) provides a short product tour and shared
  vocabulary.
- [Pactrun Developers](./pactrun-developers/architecture/system-model.md) covers
  domain invariants, product behavior, Package contracts, execution,
  persistence, recovery, and engineering policy.
- [Pactrun Users](./pactrun-users/concepts/packages-revisions-and-instances.md)
  is a planned guide to managed resources and user-visible operations.
- [Package Authors](./package-authors/fundamentals/authoring-model.md) is a
  planned authoring and integration guide.

## Documentation authority

The English Markdown under `docs/` is the canonical semantic source. The
Docusaurus site is a presentation of those files, not another specification.
Future Traditional Chinese (`zh-Hant`) documentation may translate this source,
but a translation will not independently define Pactrun behavior.

Developer pages identify their content as normative, informative, or
implementation guidance. Public role guides will summarize observable behavior
without exposing internal requirement or test traceability.
