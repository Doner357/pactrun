---
title: Authoring and format contracts
---

# Authoring and format contracts

**Status: Informative navigation.**

Authoring, normalized identity, wire, errors, and transport are distinct contracts. Their versions and stability statuses are independent.

- [Pack Distribution V1](./pack-distribution-v1.md): Source/distribution Pack directories and ZIP-based `.pack` files, exact Revision export, and transactional metadata conflict policies.

- [Actions, Inputs, and Parameters](./actions-inputs-and-parameters.md): Read the author-facing declarations and the distinctions between managed bindings, invocation parameters, and future service-resource exposure.
- [Authoring Model](./authoring-model.md): Understand the authoring contract and its relationship to the normalized Revision boundary. Read the concrete source format separately.
- [Error Taxonomy V1](./error-taxonomy-v1.md): Look up stable Pactrun-owned error identity and catalog rules. An error identity is separate from a Run outcome or a Hook-owned code.
- [Hook Protocol V1](./hook-protocol-v1.md): Implement the exact Frozen wire protocol: framing, authority, operation contexts, acknowledgments, and completion. Protocol contexts do not prove all corresponding runtimes exist.
- [Hooks, Recovery, and Cleanup](./hooks-recovery-and-cleanup.md): Understand semantic Hook authority, risk reporting, and Cleanup obligations. Consult the wire specification for encoding and the roadmap for deferred runtimes.
- [Migrations](./migrations.md): Read target-owned edges, transitions, source roles, writer validation, and per-edge semantics. Service-owned live resources are not another spelling of Managed Inputs.
- [Pack Source YAML V1](./pack-source-yaml-v1.md): Use this Candidate source projection without confusing it with the Frozen Revision Core identity format. Candidate status is not upgraded by this migration.
- [Runtime Content and Retired Recipe Design](./recipes-and-runtime-content.md): Active immutable runtime-content rules and traceable retirement notices for the rejected Recipe proposal.
- [Revision Core Format V1](./revision-core-format-v1.md): Implement the exact Frozen normalized Core encoding and identity calculation. Do not use raw Core JSON as the public Pack authoring interface.
- [Snapshot Bundle V1](./snapshot-bundle-v1.md): Read the transport envelope and validation boundary independently of Snapshot semantic identity. Follow the separate integrity contract for exact identity.
- [Snapshot Integrity Format V1](./snapshot-integrity-format-v1.md): Preserve the Frozen V1 identity and verification rules required by supported existing Snapshots. V2 does not make this compatibility contract disappear.
- [Snapshot Integrity Format V2](./snapshot-integrity-format-v2.md): Read the current M4 capture-writer integrity format. V1 and V2 remain distinct supported identities, not interchangeable encodings.
- [Snapshots and Managed Data](./snapshots-and-managed-data.md): Understand Snapshot declarations, managed-data selection, and restore roles before implementing transport or command presentation.

## M6.5 formats

[Core V2](./revision-core-format-v2.md) and [Hook V2](./hook-protocol-v2.md) are
independently Frozen contracts. [YAML V2](./pack-source-yaml-v2.md) is an accepted,
explicitly versioned Candidate authoring contract, not a Frozen source promise.
See the [format review](../../development/design-notes/m6-5-format-activation-review.md)
and [approved baseline](../../development/design-notes/m6-5-servicestorage-baseline.md).

## Shell Loader formats

[Core V3](./revision-core-format-v3.md) is Frozen and adds explicit built-in
shell-loader launch without modifying Core V1/V2 or Hook Protocol V1/V2.
[YAML V3](./pack-source-yaml-v3.md) remains a versioned Candidate source format.
[Shell Loader](./shell-loader.md) owns script lifecycle, helper commands and data
interfaces. See the [format review](../../development/design-notes/shell-loader-format-activation-review.md)
and [implementation record](../../development/shell-adapter-loader-status.md) for
verification scope, authorized local integration and the unresolved diagnostic
presentation follow-up. Protocol delivery alone is not a diagnostic display/history feature.

Return to the [specification map](../index.md). For current runtime support and
next-milestone boundaries, see [the development entry](../../development/next-milestone.md).
