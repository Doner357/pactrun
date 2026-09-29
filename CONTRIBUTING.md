# Contributing to Pactrun

Pactrun follows a modular-monolith architecture and a Git Flow branch model:
`main` is releasable, `develop` is the integration branch, and work is prepared
on focused topic branches. Choose the target version line before the prefix:
`feature/*` for capabilities, `fix/*` for development-line fixes, and `docs/*`
for documentation all start from and merge into `develop`. `release/*` is for
release stabilization; `hotfix/*` is for an existing released production line,
not every urgent or small change. See the
[Git Flow rules](./docs/development/development-and-verification.md#git-flow-and-topic-naming).

## Change discipline

- State assumptions and define verifiable completion criteria before changing
  behavior.
- Prefer the smallest sufficient, local change. Do not add speculative
  abstractions.
- Preserve module ownership: the domain must not depend on CLI, persistence, or
  third-party adapter types.
- Do not reinterpret normative semantics during ordinary implementation. Report
  an invariant conflict and use a separate design-change workflow.
- Add requirement, test, and vector identifiers only when they refer to real
  contracts. Do not create placeholder traceability.
- New functions and tests use product concepts, behavior and invariants, not
  milestone/slice markers in names or executable content. Existing occurrences
  are reviewed when working in the affected area; do not mass-rename unrelated code.
  Real format/protocol versions and stable requirement/test IDs are not milestones.
- Record design rationale with the owning rule or a stable decision reference:
  problem, reason, material alternatives, assumptions/evidence and conditions for
  reconsideration. Keep rationale informative and never invent an original reason
  that was not recorded. See the
  [detailed rules](./docs/development/development-and-verification.md#product-oriented-functions-and-tests).

## Documentation

Canonical Markdown is maintained in `docs/`. The `website/` project is a
presentation layer and must not rewrite those sources. Read the relevant
[specification authority map](./docs/spec/index.md) and normative pages before changing behavior. English is the
canonical documentation language; a future `zh-Hant` tree may provide
translations but will not independently define semantics.

Keep documentation scoped to its audience. Only docs/spec defines product
requirements; Development contains workflow, design syntheses, implementation
records, and work-order guidance. Old paths are compatibility entries, not
additional authority. Read the [Spec map](docs/spec/index.md) and the
[development entry](docs/development/index.md).

Users, Pack authors, and operation-oriented agents have complete reading entries:
[user onboarding](docs/guides/index.md), [authoring](docs/package-authors/index.md),
and [agent task selection](docs/agents/index.md). Product implementers use
[agent development navigation](docs/agents/develop-pactrun.md).
Agent sources are excluded from human HTML pages and navigation. Generated text
and HTML are publication copies, never additional authority. Personal developer
preferences do not belong in the product specification.

Documentation must be valid UTF-8. English documents may use Unicode punctuation,
symbols, diagrams, emoji, proper names, and encoding examples, but their natural
language must remain English. Do not enforce this rule with a blanket non-ASCII
ban.

Repository-owned source code, identifiers, comments, test names, and embedded
fixtures must use ASCII by default, while Pactrun behavior must correctly support
UTF-8 external text and opaque data. Prefer escapes, byte construction, or a
dedicated fixture for Unicode test data. A literal non-ASCII source-code
exception must be indispensable, narrowly scoped, and explained in ASCII.

The complete contributor contract is the
[Development and Verification Policy](./docs/development/development-and-verification.md).

## Verification

Choose the minimum sufficient checks using the
[risk-based validation policy](./docs/development/development-and-verification.md#risk-based-validation-scope).
Documentation-only edits use document/link/traceability checks and affected site
typecheck/build, not the complete Rust product suite. Full `cargo xtask ci`
remains the high-risk runtime, milestone-integration and formal-release gate.
Every change must report its verification scope, status and documentation impact;
identify reused evidence separately from checks run now.

Commits, pushes, host-level toolchain installation, and machine configuration
require separate explicit authorization.
