# Pactrun Formal Documentation Migration Audit

This audit is an implementation record, not part of the published documentation site and not a normative product specification. It records how the two deleted bootstrap sources were incorporated into the English canonical documentation.

## Completion criteria

- [x] Every source heading was classified as normative product content, contributor policy, informative guidance, a deferred closure item, an external reference, or bootstrap-only material.
- [x] Rules, tables, examples, deferred items, and external references were reviewed in addition to headings.
- [x] Normative clauses have one authoritative destination; other pages use summaries and links.
- [x] The 49-item invariant list was used as a coverage audit and was not copied as a second specification.
- [x] Agent-only task and transfer instructions were omitted. Shared human-and-agent rules were rewritten as contributor policy.
- [x] Product comparison and bootstrap naming were removed from published content.
- [x] All migrated product requirements have unique, continuous `PR-REQ-NNNN` identifiers and an explicit verification status.
- [x] Deferred schema and spelling work remains deferred rather than being invented during migration.
- [x] Product requirements and verification metadata are confined to the Pactrun Developer section.
- [x] Pactrun User and Package Author paths remain as planned public-documentation placeholders without internal traceability.

## Development and verification policy source

| Source section | Disposition | Authoritative destination |
| --- | --- | --- |
| 1. Core principle | Migrated | `docs/pactrun-developers/engineering/development-and-verification.md` |
| 2. Canonical source of truth | Migrated | `docs/pactrun-developers/engineering/development-and-verification.md` |
| 3. Normative and informative documentation | Migrated | `docs/pactrun-developers/engineering/development-and-verification.md` |
| 4. Stable requirement identifiers | Migrated; examples converted to placeholders | `docs/pactrun-developers/engineering/development-and-verification.md` |
| 5. Verification for verifiable requirements | Migrated | `docs/pactrun-developers/engineering/development-and-verification.md` |
| 6. Test identifiers and bidirectional traceability | Migrated as policy; no unimplemented test IDs created | `docs/pactrun-developers/engineering/development-and-verification.md` |
| 7. Verification of the real contract | Migrated | `docs/pactrun-developers/engineering/development-and-verification.md` |
| 8. Required verification levels | Migrated, including all five levels | `docs/pactrun-developers/engineering/development-and-verification.md` |
| 9. Negative requirements | Migrated | `docs/pactrun-developers/engineering/development-and-verification.md` |
| 10. Property-based and fuzz verification | Migrated | `docs/pactrun-developers/engineering/development-and-verification.md` |
| 11. Normative semantics during implementation | Migrated | `docs/pactrun-developers/engineering/development-and-verification.md` |
| 12. Separate design-change workflow | Migrated | `docs/pactrun-developers/engineering/development-and-verification.md` |
| 13. Tests and semantic authority | Migrated | `docs/pactrun-developers/engineering/development-and-verification.md` |
| 14. Tool-enforced traceability | Migrated | `docs/pactrun-developers/engineering/development-and-verification.md` |
| 15. Coverage metrics | Migrated | `docs/pactrun-developers/engineering/development-and-verification.md` |
| 16. Documentation-to-test links | Migrated; sample identifiers converted to placeholders | `docs/pactrun-developers/engineering/development-and-verification.md` |
| 17. Test removal and weakening | Migrated | `docs/pactrun-developers/engineering/development-and-verification.md` |
| 18. Regression verification | Migrated | `docs/pactrun-developers/engineering/development-and-verification.md` |
| 19. Feature requirement definition | Migrated | `docs/pactrun-developers/engineering/development-and-verification.md` |
| 20. Documentation impact check | Migrated | `docs/pactrun-developers/engineering/development-and-verification.md` |
| 21. Task completion checklist | Generalized as the Change Completion Checklist | `docs/pactrun-developers/engineering/development-and-verification.md` |
| 22. Conflict behavior | Generalized as Specification Conflict Reporting | `docs/pactrun-developers/engineering/development-and-verification.md` |
| 23. Anti-patterns | Migrated | `docs/pactrun-developers/engineering/development-and-verification.md` |
| 24. Repository-level rule | Migrated | `docs/pactrun-developers/engineering/development-and-verification.md` and `CONTRIBUTING.md` |
| 25. Desired outcome | Migrated | `docs/pactrun-developers/engineering/development-and-verification.md` |

## Architecture and implementation source

| Source section | Disposition | Authoritative destination |
| --- | --- | --- |
| 0. Reading rules and design status | Product-status language migrated; bootstrap-only instructions omitted | `docs/index.md`, `docs/introduction.md` |
| 1. Product position | Migrated without comparison framing | `docs/introduction.md` |
| 2. Product design charter | Migrated | `docs/introduction.md` |
| 3. Package taxonomy and Stack | Migrated | `docs/introduction.md`, `docs/pactrun-developers/package-contracts/authoring-model.md` |
| 4. Source through Instance lifecycle | Migrated | `docs/pactrun-developers/product-behavior/packages-revisions-and-instances.md` |
| 5. Package and Revision identity | Migrated | `docs/pactrun-developers/architecture/identity-and-state.md` |
| 6. Normalized Pack Definition | Migrated | `docs/pactrun-developers/package-contracts/authoring-model.md` |
| 7. Action | Migrated | `docs/pactrun-developers/package-contracts/actions-inputs-and-parameters.md` |
| 8. Snapshot, Migration, and Cleanup separation | Migrated | `docs/pactrun-developers/package-contracts/snapshots-and-managed-data.md`, `migrations.md`, and `hooks-recovery-and-cleanup.md` |
| 9. Inputs and Secrets | Migrated | `docs/pactrun-developers/product-behavior/inputs-secrets-and-readiness.md` and `docs/pactrun-developers/package-contracts/actions-inputs-and-parameters.md` |
| 10. Secret protection | Migrated | `docs/pactrun-developers/product-behavior/inputs-secrets-and-readiness.md` |
| 11. Invocation parameters | Migrated | `docs/pactrun-developers/package-contracts/actions-inputs-and-parameters.md` |
| 12. Recipe authoring contract | Migrated | `docs/pactrun-developers/package-contracts/recipes-and-runtime-content.md` |
| 13. Managed data, artifacts, and snapshots | Migrated | `docs/pactrun-developers/package-contracts/snapshots-and-managed-data.md` |
| 14. Snapshot import and export | Migrated | `docs/pactrun-developers/product-behavior/snapshots-migrations-and-recovery.md` |
| 15. Revision migration | Migrated | `docs/pactrun-developers/package-contracts/migrations.md` |
| 16. Transition checkpoints and recovery risk | Migrated, including the consequence table | `docs/pactrun-developers/package-contracts/hooks-recovery-and-cleanup.md` |
| 17. Workflow compiler | Migrated | `docs/pactrun-developers/architecture/system-model.md` |
| 18. State version, stale plans, and admission | Migrated | `docs/pactrun-developers/architecture/execution-and-concurrency.md` |
| 19. Observe and Mutate concurrency and pins | Migrated | `docs/pactrun-developers/architecture/execution-and-concurrency.md` |
| 20. Hooks and canonical protocol | Migrated | `docs/pactrun-developers/package-contracts/hooks-recovery-and-cleanup.md` |
| 21. Run records and outcomes | Migrated | `docs/pactrun-developers/product-behavior/actions-plans-and-runs.md` |
| 22. Interrupted reconciliation and durable recovery | Migrated | `docs/pactrun-developers/architecture/recovery-and-reconciliation.md` |
| 23. Deletion, Cleanup, and manual recovery | Migrated | `docs/pactrun-developers/product-behavior/snapshots-migrations-and-recovery.md` and `docs/pactrun-developers/package-contracts/hooks-recovery-and-cleanup.md` |
| 24. Management operations and managed executions | Migrated | `docs/pactrun-developers/architecture/system-model.md` |
| 25. Resource lifecycle, retention, and garbage collection | Migrated | `docs/pactrun-developers/architecture/resources-and-versioning.md` |
| 26. Version domains | Migrated; real format and protocol versions retained | `docs/pactrun-developers/architecture/resources-and-versioning.md` |
| 27. Command-line baseline | Migrated; unresolved spelling remains deferred | `docs/pactrun-developers/product-behavior/command-and-output-reference.md` |
| 28. Import and export identity | Migrated | `docs/pactrun-developers/architecture/identity-and-state.md` and `docs/pactrun-developers/product-behavior/snapshots-migrations-and-recovery.md` |
| 29. Modular monolith architecture | Migrated | `docs/pactrun-developers/architecture/system-model.md` |
| 30. Implementer discretion | Migrated as non-normative guidance | `docs/pactrun-developers/engineering/implementation-guidance.md` |
| 31. Core invariants | Audited against authoritative pages; not duplicated | See the invariant coverage table below |
| 32. Suggested implementation order | Migrated | `docs/pactrun-developers/engineering/implementation-guidance.md` |
| 33. Short transfer summary | Omitted as a duplicate bootstrap summary | Not applicable |
| 34. Adopted external design references | Migrated | `docs/pactrun-developers/engineering/design-references.md` |
| 35. Specification queue | Migrated without closing open designs | `docs/pactrun-developers/engineering/implementation-guidance.md` and Developer specification pages |

## Core invariant coverage

The ranges below are audit pointers only. The destination pages contain the authoritative clauses and requirement identifiers.

| Source invariant numbers | Authoritative destination |
| --- | --- |
| 1–7 | `docs/pactrun-developers/architecture/identity-and-state.md` |
| 8–9 | `docs/pactrun-developers/product-behavior/packages-revisions-and-instances.md` and `docs/pactrun-developers/architecture/identity-and-state.md` |
| 10–16 | Developer Package Contracts, `docs/pactrun-developers/product-behavior/inputs-secrets-and-readiness.md`, and the remaining Product Behavior pages |
| 17–20 | `docs/pactrun-developers/package-contracts/snapshots-and-managed-data.md` and `docs/pactrun-developers/product-behavior/snapshots-migrations-and-recovery.md` |
| 21–25 | `docs/pactrun-developers/package-contracts/migrations.md` |
| 26–31 | `docs/pactrun-developers/architecture/system-model.md` and `execution-and-concurrency.md` |
| 32–36 | `docs/pactrun-developers/package-contracts/hooks-recovery-and-cleanup.md` |
| 37–42 | `docs/pactrun-developers/product-behavior/actions-plans-and-runs.md`, `docs/pactrun-developers/architecture/recovery-and-reconciliation.md`, and `docs/pactrun-developers/product-behavior/snapshots-migrations-and-recovery.md` |
| 43–46 | `docs/pactrun-developers/package-contracts/hooks-recovery-and-cleanup.md` and `docs/pactrun-developers/product-behavior/snapshots-migrations-and-recovery.md` |
| 47–49 | `docs/pactrun-developers/architecture/resources-and-versioning.md` and `recovery-and-reconciliation.md` |

## Deferred closure audit

The following remain explicitly open: wire schemas, canonical manifests, persistence and concurrency encoding, exact command spelling, structured output schemas, complete test vectors, backend isolation enforcement, and future translation tooling. No placeholder test identifiers or speculative contracts were added for these items.

## Validation record

- [x] Requirement identifiers are unique and continuous.
- [x] Every current requirement uses `Verification: Pending automated coverage.`
- [x] Requirement and test traceability appears only in Pactrun Developer documentation.
- [x] All 11 public role pages use the common planned-placeholder structure.
- [x] Published English documentation contains no non-English natural-language paragraphs.
- [x] Markdown is valid UTF-8 and the rendered site preserves Unicode and emoji.
- [x] Repository-owned source and configuration files contain no unapproved non-ASCII characters.
- [x] Docusaurus navigation is explicitly role-oriented and all 32 pages render.
- [x] Local format, lint, test, and type checks pass.
- [x] The configured POSIX remote passes `cargo xtask ci`, including the optimized documentation build.
