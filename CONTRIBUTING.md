# Contributing to Pactrun

Pactrun follows a modular-monolith architecture and a Git Flow branch model:
`main` is releasable, `develop` is the integration branch, and work is prepared
on focused `feature/*` branches.

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

Usage guides for users, Pack authors, and operation-oriented agents remain
placeholders until a later documentation phase. Development guides and
[agent development navigation](docs/agents/develop-pactrun.md) are available now.
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

Run `cargo xtask ci` for the same ordered checks used by the remote environment
and GitHub Actions. Every change must report its verification status and
documentation impact.

Commits, pushes, host-level toolchain installation, and machine configuration
require separate explicit authorization.
