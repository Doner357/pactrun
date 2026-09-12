# Pactrun

Pactrun is a local-first package management and execution product implemented in
Rust. The repository uses a modular-monolith architecture. Canonical `develop`
contains the completed M1 persistence foundations, the integrated M2 Pack
installation, Instance, and Managed Input binding implementation, and completed
M3 Action execution with Slices 1 through 6 integrated, and completed M4 Snapshot
lifecycle with Slices S0 through S8 integrated. The Rust application,
domain, and repository surfaces remain internal and do not constitute a stable
public Rust API.

M4 provides Capture, Restore, Snapshot inspection/verification and the bundle CLI
on the integrated PersistenceSchemaV5 baseline. New Capture writes integrity V2;
Import, Verify, Export and exact-compatible Restore preserve V1/V2 semantics.
See the [M4 execution and acceptance record](docs/pactrun-developers/engineering/m4-implementation-status.md)
and [Snapshot commands](docs/pactrun-developers/product-behavior/m4-snapshot-command-reference.md).
Snapshot deletion and later lifecycle features remain outside the approved M4
scope. Git integration is not a release or a public documentation deployment.

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
