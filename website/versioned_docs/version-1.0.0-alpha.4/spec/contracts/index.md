---
title: Authoring and format contracts
---

# Authoring and format contracts

**Status: Informative navigation.**

Authoring, normalized identity, wire, errors, and transport are distinct contracts. Their versions and stability statuses are independent.
Use the task reading path to choose an owner. This list is a lookup map; there is
no requirement to implement or read every format in list order.

- [CLI object ID selectors](./cli-id-selectors.md): Resolve unique prefixes and display usable short object IDs while preserving full machine identities.
- [CLI Machine Interface](./cli-machine-interface.md): Typed JSON/JSONL results, events, delivery and compatibility rules.

- [Pack Distribution Baseline](./pack-distribution.md): Source/distribution Pack directories and ZIP-based `.pack` files, exact Revision export, and transactional metadata conflict policies.

- [Actions, Inputs, and Parameters](./actions-inputs-and-parameters.md): Read the author-facing declarations and the distinctions between managed bindings, invocation parameters, and service-resource exposure.
- [Authoring Model](./authoring-model.md): Understand the authoring contract and its relationship to the normalized Revision boundary. Read the concrete source format separately.
- [Error Taxonomy V1](./error-taxonomy-v1.md): Look up stable Pactrun-owned error identity and catalog rules. An error identity is separate from a Run outcome or a Hook-owned code.
- [Hook Protocol Baseline](./hook-protocol.md): Implement the current wire framing, granted authority, operation contexts, acknowledgments, and completion. Consult the operation owners for runtime obligations.
- [Hooks, Recovery, and Cleanup](./hooks-recovery-and-cleanup.md): Understand semantic Hook authority, risk reporting, and Cleanup obligations. Consult the wire specification for encoding and the linked execution contracts for runtime boundaries.
- [Migrations](./migrations.md): Read target-owned edges, transitions, source roles, writer validation, and per-edge semantics. Service-owned live resources are not another spelling of Managed Inputs.
- [Pack Source Baseline](./pack-source.md): Author and validate the supported YAML source, including service declarations and Shell Loader alternatives. Installation projects it into normalized Revision content.
- [Runtime Content and Retired Recipe Design](./recipes-and-runtime-content.md): Active immutable runtime-content rules and traceable retirement notices for the rejected Recipe proposal.
- [Revision Canonical Baseline](./revision-canonical.md): Implement exact normalized Core encoding and identity calculation. Use Pack source for public authoring.
- [Snapshot Bundle Baseline](./snapshot-bundle.md): Read the transport envelope and validation boundary independently of Snapshot semantic identity.
- [Snapshot Integrity Baseline](./snapshot-integrity.md): Read the supported string-marked identity, canonical bytes, payload closure, and verification rules. Numeric development readers are retired.
- [Snapshots and Managed Data](./snapshots-and-managed-data.md): Understand Snapshot declarations, managed-data selection, and restore roles before implementing transport or command presentation.

## M6.5 formats

The current Revision, Hook, and Pack source baselines above include the approved
ServiceStorage declarations and authorities. Their version domains are independent.
The [format review](../../development/design-notes/m6-5-format-activation-review.md)
and [design baseline](../../development/design-notes/m6-5-servicestorage-baseline.md)
retain the historical V2 activation decisions; those labels do not authorize
development-era readers.

## Shell Loader formats

The current Revision and Pack source baselines include explicit Shell Loader
launch alternatives. [Shell Loader](./shell-loader.md) owns script lifecycle,
helper commands, and data interfaces. The
[format review](../../development/design-notes/shell-loader-format-activation-review.md)
and [implementation record](../../development/shell-adapter-loader-status.md)
retain historical activation and delivery evidence. Current live and retained
diagnostic behavior belongs to [execution diagnostics](../behavior/execution-diagnostics.md);
the original Loader delivery's presentation gap is closed by that later work.

Return to the [specification map](../index.md). For current runtime support and
next-milestone boundaries, see [the development entry](../../development/next-milestone.md).
