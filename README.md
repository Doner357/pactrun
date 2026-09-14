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
See the [M4 execution and acceptance record](./docs/development/m4-implementation-status.md)
and [Snapshot commands](./docs/spec/behavior/m4-snapshot-command-reference.md).
Snapshot deletion and later lifecycle features remain outside the approved M4
scope. Git integration is not a release or a public documentation deployment.

The integrated [M5 implementation](./docs/development/m5-implementation-status.md)
adds Migration path-ID selection, read-only planning, target-qualified operator
file inputs, declarative and Hook-backed chains, atomic per-edge commits, and
no-replay owner-loss reconciliation. [Migration commands](./docs/spec/behavior/m5-migration-command-reference.md)
use [PersistenceSchemaV6](./docs/spec/persistence/persistence-schema-v6.md).
This checkout initializes V6 and upgrades only exact V5 through explicit
`pactrun storage upgrade`; V4 and earlier first need a compatible M4 build.
M6 and ServiceStorage representation/runtime remain outside the completed M5 scope.

Bounded [M6 recovery](./docs/development/m6-implementation-status.md) is now
complete and integrated into local develop. It adds cross-operation recovery
evidence and fault-injection coverage while preserving V6 and existing runtime
semantics. M6.5 ServiceStorage is next, with separate design approval required.

## Repository layout

- `src/` contains the production crate and crate-private architecture modules.
- `xtask/` contains repository-level verification commands.
- `docs/` contains canonical Markdown sources.
- `website/` renders `docs/` with Docusaurus and does not own normative content.

English Markdown is canonical. The [Spec map](docs/spec/index.md) is the product
specification entry; [Development](docs/development/index.md) provides reading
paths, verification policy, and the [M6.5 handoff](docs/development/next-milestone.md).
All existing requirement-bearing documents have moved into Spec. Usage guides
remain planned placeholders; their completion is a later task.

For coding agents, read CONTRIBUTING.md and [the agent entry](docs/agents/index.md).
Published builds provide llms.txt and agent-docs Markdown without agent-only
HTML pages. Repository instructions and the current task govern permissions;
personal workspace preferences are not product rules. No agent tool is assumed
to discover these files automatically.

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
