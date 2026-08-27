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
Pactrun Developer normative pages before changing behavior. English is the
canonical documentation language; a future `zh-Hant` tree may provide
translations but will not independently define semantics.

Keep documentation scoped to its audience. Requirement identifiers, test
identifiers, verification status, test design, and implementation details belong
only in the Pactrun Developer section. Pactrun User and Package Author pages
must contain only audience-appropriate introductions, tutorials, operational
guidance, and observable behavior. Before the initial release, those public
sections remain planned placeholders. A public explanation may summarize a
Developer specification, but it must not expose internal traceability or create
a second normative clause.

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
[Development and Verification Policy](docs/pactrun-developers/engineering/development-and-verification.md).

## Verification

Run `cargo xtask ci` for the same ordered checks used by the remote environment
and GitHub Actions. Every change must report its verification status and
documentation impact.

Commits, pushes, host-level toolchain installation, and machine configuration
require separate explicit authorization.
