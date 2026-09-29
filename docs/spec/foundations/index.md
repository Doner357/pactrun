---
title: Foundations
---

# Foundations

**Status: Informative navigation.**

Read these contracts to understand the scope and ownership model before changing any operation.

Start with the system model, then identity/state and resource lifetimes. Consult
versioning when the change affects support or compatibility. This is a reference
map, not a sequence of implementation authorizations.

- [System Model](./system-model.md): Start here for the product boundary, data ownership, and architectural constraints that apply to every operation.
- [Identity and State](./identity-and-state.md): Look up exact identities, non-identity metadata, Instance state, Managed Inputs, and the separately closed ServiceStorage semantics. This is not a CLI tutorial.
- [Resources and Versioning](./resources-and-versioning.md): Use this contract for resource lifetimes, retention and pins, local trust, and independent format/runtime version boundaries.
- [Product Versioning and Compatibility](./product-versioning-and-compatibility.md): Current alpha-baseline admission and origin-independent data acceptance, plus the separate formal-release Major compatibility commitment and promotion gates.

Return to the [specification map](../index.md). For current runtime support and
next-milestone boundaries, see [the development entry](../../development/next-milestone.md).
