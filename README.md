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
That integrated baseline initializes V6 and upgrades only exact V5 through
explicit `pactrun storage upgrade`.
M6 and ServiceStorage representation/runtime remain outside the completed M5 scope.

Bounded [M6 recovery](./docs/development/m6-implementation-status.md) is now
complete and integrated into local develop. It adds cross-operation recovery
evidence and fault-injection coverage while preserving V6 and existing runtime
semantics.

The integrated develop baseline adds [M6.5 ServiceStorage](./docs/development/m6-5-implementation-status.md):
explicit YAML/Core V2 installation, persistent Instance-isolated service storage,
resource inspection/observation/path disclosure, independently selected Hook V2,
and target-owned resource reuse, retention, reattachment and transformation.
It initializes [PersistenceSchemaV7](./docs/spec/persistence/persistence-schema-v7.md)
and upgrades only exact V6 through explicit `pactrun storage upgrade`; earlier
stores first require a compatible historical build. Live service bytes are not
Managed Inputs or automatic Snapshot contents. See the
[ServiceStorage commands](./docs/spec/behavior/m6-5-service-storage-command-reference.md)
and [format review](./docs/development/design-notes/m6-5-format-activation-review.md).
M6.5 is integrated into local develop; its delivery record distinguishes
final-source validation from Git integration. M7 Cleanup, deletion and
abandonment require separate design approval and remain outside this scope.

The separately approved [M7 implementation](./docs/development/m7-implementation-status.md)
is implemented, verified and integrated into local develop.
It adds Cleanup/deletion, explicit completion assertion, Abandon, detached
handoff/discard and [V8](./docs/spec/persistence/persistence-schema-v8.md) with
exact-V7 upgrade. Native retirement is qualified against concurrent namespace
changes and retains durable no-replay/retry evidence. No product network
feature, daemon, release, remote push or documentation deployment is implied.

The integrated [managed-object lifecycle implementation](./docs/development/managed-object-lifecycle-status.md)
adds Artifact export/deletion, Snapshot/Revision/Run deletion and explicit
`pactrun storage gc [--plan]`. That lifecycle integration introduced
[PersistenceSchemaV9](./docs/spec/persistence/persistence-schema-v9.md), with exact
V8 upgrade. GC never collects service-owned data, and Artifact
export requires `--authorize-sensitive-export` with a new file destination.
See the [lifecycle contract](./docs/spec/behavior/managed-object-lifecycle.md) for
guards, retention, retry and read-only preview semantics.

The integrated [Snapshot Capacity and Restore workflow](./docs/development/snapshot-capacity-and-restore-status.md)
removes fixed service-data byte ceilings, retains structural/resource safeguards,
and adds `instance create <name> --revision <reference> --restore-from <snapshot-id>`.
That integration introduced [PersistenceSchemaV10](./docs/spec/persistence/persistence-schema-v10.md),
with explicit upgrade from exact V8 or V9 and preserved legacy inline readers.
Immutable data references keep new service bytes out of SQLite WAL while
preserving atomic Snapshot publication and guarded Restore. Full CI and real
beyond-ceiling round trips passed; local integration does not authorize publication.
Shell Adapter / Loader is integrated. The approved
[execution diagnostics milestone](./docs/development/execution-diagnostics-observability-status.md)
is implemented, verified and integrated into local `develop`, with default
bounded Hook evidence. The current baseline is
[PersistenceSchemaV11](./docs/spec/persistence/persistence-schema-v11.md), with
explicit exact-V8/V9/V10 upgrade. No release or publication is implied.

## Repository layout

Portable Pack transport is implemented, verified and integrated into local develop; see the
[C implementation record](./docs/development/pack-transport-status.md). The approved
[Pack format](./docs/spec/contracts/pack-distribution-v1.md) supports source and
distribution directories or ZIP-based `.pack` files through `pack install`.
`revision export` preserves exact installed identity without reconstructing source.
Revision and Snapshot export take `--output <base-path>` and always append
`.pack` and `.snapshot` respectively, even if the base already has that suffix.
Input and Artifact export retain exact caller-selected filenames. Import uses
the exact supplied path; existing Snapshot `.zip` files remain supported.
Local integration does not authorize release or publication.

- `src/` contains the production crate and crate-private architecture modules.
- `xtask/` contains repository-level verification commands.
- `docs/` contains canonical Markdown sources.
- `website/` renders `docs/` with Docusaurus and does not own normative content.

English Markdown is canonical. The [Spec map](docs/spec/index.md) is the product
specification entry; [Development](docs/development/index.md) provides reading
paths, verification policy, and the [baseline / M7 handoff](docs/development/next-milestone.md).
All existing requirement-bearing documents have moved into Spec. Usage guides
remain planned placeholders; their completion is a later task.

For coding agents, read CONTRIBUTING.md and [the agent entry](docs/agents/index.md).
Published builds provide llms.txt and agent-docs Markdown without agent-only
HTML pages. Repository instructions and the current task govern permissions;
personal workspace preferences are not product rules. No agent tool is assumed
to discover these files automatically.

## Validation

Choose the smallest sufficient checks under the
[risk-based validation policy](docs/development/development-and-verification.md#risk-based-validation-scope).
The shared full-verification gate, not the default for every edit, is:

```text
cargo xtask ci
```

It checks Rust formatting, Clippy, all workspace tests, Docusaurus types, and the
production documentation build. Documentation-only verification uses:

```text
cargo test -p xtask candidate_or_frozen_metadata_and_traceability_are_valid -- --test-threads=1
pnpm --dir website run typecheck
cargo xtask docs-build
```

Complete verification and documentation builds run on the configured POSIX
remote and in GitHub Actions. Local checks must remain within the workspace and
must not alter host-level configuration.
An unchanged implementation need not rerun full CI merely because a closeout
paragraph or commit SHA changed; the policy defines dependency-aware evidence
reuse and the remaining full-suite gates.
