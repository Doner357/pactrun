# Pactrun

Pactrun manages installed Pack revisions, long-lived Instances, and Pack-defined
operations on the local machine. It runs as a single executable on Linux and Windows.

## What you can do

- Install Packs from source, distribution directories, or `.pack` archives.
- Create Instances and supply their Inputs and Secrets.
- Discover Actions, preview operations with `--plan`, and inspect Run outcomes.
- Capture and restore Snapshots.
- Migrate Instances through declarative or Hook-backed revision paths.
- Manage persistent ServiceStorage and inspect declared resources.
- Run Cleanup, delete managed objects, or abandon an Instance while retaining its
  service data.

Packs define how services are started, configured, migrated, and stopped. Pactrun
tracks managed state and execution outcomes; service processes use their own
runtime, such as Docker, systemd, or Kubernetes.

## CLI output

Human output provides summaries, descriptions, and actionable errors. `run show`
provides execution diagnostics.

Object IDs accept unique prefixes of at least eight lowercase hexadecimal digits,
for example `pactrun run show 6fb391d6`. Human lists display usable short IDs;
`--no-trunc` shows full IDs. See [ID selectors](./docs/spec/contracts/cli-id-selectors.md)
for Revision and Migration forms.

- `--format json` returns one complete response.
- `--format jsonl` streams events and ends with a result.
- Both use `format: "pactrun.cli"` and `format_version: "1.0-alpha.1"`.
- Noninteractive Hook output is delivered as Base64 byte chunks in machine modes.
  Human mode keeps the Hook's stdout and stderr streams.
- `--cancel-on-output-close` requests cancellation when a JSONL execution's
  receiver disconnects. By default, Pactrun continues managing the operation.

See the [command reference](./docs/spec/behavior/command-and-output-reference.md)
and [machine-output contract](./docs/spec/contracts/cli-machine-interface.md).

## Documentation

- [Usage guides](./docs/guides/index.md) - planned guides and topic coverage.
- [Specification](./docs/spec/index.md) - formats, lifecycle, and exact behavior.
- [Development](./docs/development/index.md) - implementation history, current
  baseline, and verification policy.
- [Design references](./docs/development/design-references.md) - sources and adopted
  design principles.

English Markdown in `docs/` is canonical. `website/` renders it with Docusaurus.

## Development

Pactrun is written in Rust as a modular monolith. Read
[CONTRIBUTING.md](./CONTRIBUTING.md) and the [agent entry](./docs/agents/index.md)
before changing the implementation. Source lives in `src/`; repository checks
live in `xtask/`.

Choose checks using the
[verification policy](./docs/development/development-and-verification.md#risk-based-validation-scope).
The full verification gate is:

```text
cargo xtask ci
```

It checks formatting, Clippy, workspace tests, documentation contracts, Docusaurus
types, and the documentation build. Full verification and documentation builds
run on the configured POSIX development environment or CI.
