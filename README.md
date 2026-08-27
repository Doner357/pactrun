# Pactrun

Pactrun is a local-first package management and execution product implemented in
Rust. This repository currently contains a compile-oriented modular-monolith
scaffold; it does not yet implement the documented product behavior or publish a
stable Rust library API.

## Repository layout

- `src/` contains the production crate and crate-private architecture modules.
- `xtask/` contains repository-level verification commands.
- `docs/` contains canonical Markdown sources.
- `website/` renders `docs/` with Docusaurus and does not own normative content.

The English Markdown under `docs/` is the canonical semantic source. Start with
the [documentation home](docs/index.md). The Pactrun Developer path contains the
current normative product and engineering specifications. Pactrun User and
Package Author paths reserve the public guide structure for completion after the
initial release. Requirements already have stable IDs; implementation tests,
vectors, and traceability tooling remain future work and are reported honestly
as pending coverage.

## Validation

The shared full-verification entry point is:

```text
cargo xtask ci
```

It checks Rust formatting, Clippy, all workspace tests, Docusaurus types, and the
production documentation build. The documentation-only entry point is:

```text
cargo xtask docs-build
```

Complete verification and documentation builds run on the configured POSIX
remote and in GitHub Actions. Local checks must remain within the workspace and
must not alter host-level configuration.
