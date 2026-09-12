# Pactrun

Pactrun is a local-first package management and execution product implemented in
Rust. The repository uses a modular-monolith architecture. Canonical `develop`
contains the completed M1 persistence foundations, the integrated M2 Pack
installation, Instance, and Managed Input binding implementation, and completed
M3 Action execution with Slices 1 through 6 integrated. The Rust application,
domain, and repository surfaces remain internal and do not constitute a stable
public Rust API.

The `feature/m4-snapshot-lifecycle` working tree adds the complete approved M4
Capture, Restore, Snapshot inspection/verification and bundle CLI implementation.
See the [M4 execution and acceptance record](docs/pactrun-developers/engineering/m4-implementation-status.md)
and [Snapshot commands](docs/pactrun-developers/product-behavior/m4-snapshot-command-reference.md).
This does not claim that those changes have been committed or integrated into
`develop`, or that Snapshot deletion or later lifecycle features are implemented.

## Repository layout

- `src/` contains the production crate and crate-private architecture modules.
- `xtask/` contains repository-level verification commands.
- `docs/` contains canonical Markdown sources.
- `website/` renders `docs/` with Docusaurus and does not own normative content.

The English Markdown under `docs/` is the canonical semantic source. Start with
the [documentation home](docs/index.md). The Pactrun Developer path contains the
current normative product and engineering specifications. Pactrun User and
Package Author paths reserve the public guide structure for completion after the
initial release. Requirements, implementation tests, vectors, and traceability
are recorded with stable IDs and report actual coverage without implying support
for deferred milestones.

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
