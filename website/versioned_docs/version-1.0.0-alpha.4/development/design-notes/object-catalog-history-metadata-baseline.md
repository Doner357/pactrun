---
title: Object Catalog, History and Metadata Baseline
---

# Object catalog, history and metadata baseline

The operator approved B S0-S4 implementation on 2026-09-19. This supersedes
the planning-only status for B in the remaining capability plan. Implementation
and verification are not implied by design approval. No Git publication is authorized.

The human interface uses summary tables, detailed show commands and explicit
mutations, without a pager or TUI. Lists default to 50 records (1-500), with
exclusive typed identity cursors and explicit continuation commands. Existing
`run list <name>` adopts this bounded behavior; IDs are not chronological order.
Single-command observations share a read snapshot; pages do not freeze the store.

Revision discovery exposes exact identity and typed metadata sources. History
discovery uses retained Instance identities rather than live names. Name reuse
never transfers history. Local aliases, notes and trust reuse semantic CAS and
V11 storage; trust is descriptive, not an execution policy. Query operations do
not initialize, upgrade, reconcile, create Runs or acquire service resources.

S1 delivers Revision catalog/inspection; S2 delivers historical discovery;
S3 delivers local metadata commands; S4 closes contract tests and verification.
Revision bundles remain C, machine output remains D and consolidation remains E.
Human formatting is not a machine protocol. Internal results remain typed.

Owning contracts: [command reference](../../spec/behavior/command-and-output-reference.md),
[identity and metadata](../../spec/foundations/identity-and-state.md), and
[metadata transactions](../../spec/persistence/persistence-baseline.md).

At S0 approval, verification was Not run. Current implementation and verification
evidence is recorded in the [delivery record](../object-catalog-history-metadata-status.md);
this approval baseline does not itself claim integration.
