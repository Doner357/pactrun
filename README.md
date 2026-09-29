# Pactrun

Pactrun manages installed Pack revisions, long-lived Instances, and Pack-defined
operations on the local machine. It runs as a single executable on Linux and Windows.

## Get started

1. [Install and verify the supplied Pactrun executable](./docs/guides/installation.md).
2. [Create your first Instance](./docs/introduction.md) in an isolated environment.
3. [Install a supplied Pack and create a usable Instance](./docs/guides/use-pack.md),
   then choose an operating task from the [user guide](./docs/guides/index.md).

To write Packs instead, verify the executable first, then follow the
[author route](./docs/package-authors/index.md). Public download/package-source
publication is not announced here; the installation procedure uses an approved
standalone delivery and checksum.

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
`--no-trunc` shows full IDs. See [references and pagination](./docs/pactrun-users/operations/command-and-output-reference.md#references-and-pagination)
for Revision and Migration forms.

- `--format json` returns one complete response.
- `--format jsonl` streams events and ends with a result.
- Both use `format: "pactrun.cli"` and `format_version: "1.0-alpha.1"`.
- Noninteractive Hook output is delivered as Base64 byte chunks in machine modes.
  Human mode keeps the Hook's stdout and stderr streams.
- `--cancel-on-output-close` requests cancellation when a JSONL execution's
  receiver disconnects. By default, Pactrun continues managing the operation.

See the [user command reference](./docs/pactrun-users/reference/index.md)
and [machine-output reference](./docs/pactrun-users/reference/machine-output.md).

## Documentation

- [User guides](./docs/guides/index.md) - onboarding and independent operating tasks.
- [Pack author guide](./docs/package-authors/index.md) - authoring route, optional capabilities, and field references.
- [Specification](./docs/spec/index.md) - formats, lifecycle, and exact behavior.
- [Development](./docs/development/index.md) - implementation history, current
  baseline, and verification policy.
- [Design references](./docs/development/design-references.md) - sources and adopted
  design principles.

English Markdown in `docs/` is canonical. `website/` renders it with Docusaurus.

## Development

Pactrun is written in Rust as a modular monolith. Read
[CONTRIBUTING.md](./CONTRIBUTING.md) and the [developer agent entry](./docs/agents/develop-pactrun.md)
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
