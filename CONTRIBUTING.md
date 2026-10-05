# Contributing to Pactrun

## Build and run

CI uses Rust stable with rustfmt and Clippy; Cargo.toml declares Rust 2024.
Build on Windows or Linux with a native C compiler and linker for bundled
SQLite. Windows CI uses the MSVC toolchain. Release reproduction uses the pinned
toolchain in the release workflow rather than whichever compiler is newest.

From the repository root:

```text
cargo build --locked
cargo run -- --help
```

Run exercises against an isolated, explicitly provisioned storage root;
see [data locations](docs/guides/data-location.md).

## Make a change

Read the relevant [Spec](docs/spec/index.md) and its tests. Define changed behavior
before implementing it. Keep Domain policy independent of CLI, storage adapters
and third-party mechanism types.

Tests declare actual coverage with Test-ID: PR-TEST-NNNN and Verifies: PR-REQ-NNNN
comments. Owning requirements cite those tests in the other direction. Preserve
IDs when moving code; retired IDs are not reused and numeric gaps are valid.

Use [Git Flow](https://nvie.com/posts/a-successful-git-branching-model/): development
topics start from and return to `develop`; `main` contains release-ready history. Use `feature/*`, `fix/*` or `docs/*` for development topics,
`release/*` for stabilization and `hotfix/*` for a released production line.
Use Conventional Commits.

For CLI presentation, use [CLIG](https://clig.dev/#human-first-design) for human-first
output and discoverable help, and [GOV.UK error-message guidance](https://design-system.service.gov.uk/components/error-message/)
for explaining what went wrong and how to correct it. Keep those wording choices
separate from the [machine interface](docs/spec/interfaces/machine-output.md).

## Verify the change

| Change | Checks |
| --- | --- |
| Documentation or navigation | Documentation tests, traceability and affected site typecheck/build |
| Spec, embedded SQL/schema, codecs or fixtures | Consuming contract tests plus documentation checks |
| Bounded Rust change | Formatting, Clippy and affected tests, including refusals |
| High-risk or cross-cutting runtime change; formal release | Full `cargo xtask ci` and affected platform/artifact checks |

Inspect consumers before narrowing scope: Markdown may be executable test input.
Identity, persistence, concurrency, Hook authority and recovery changes need
particular care. Use the full gate when impact cannot be bounded. A filter must
execute the intended tests, not return success with zero matches.

Report checks actually run and their tested source; distinguish reused evidence
and unrun checks. A documentation-only commit does not by itself invalidate
unchanged runtime evidence. Do not remove behavioral assertions to obtain a pass.
See [test entry points](tests/README.md) for focused and opt-in checks.

## Documentation and maintenance

Canonical English Markdown lives in `docs/`. UTF-8 punctuation and examples are
valid; external product data is not restricted to ASCII. Repository code,
identifiers and comments use ASCII by default; Unicode fixtures use escapes or
dedicated data files where practical.

- [Website maintenance](website/README.md): generated references, versions and builds.
- [Delivery tools](tools/README.md): candidate builds, package catalog and Pages.

Use the [Google developer documentation style guide](https://developers.google.com/style)
for prose and [Diátaxis](https://diataxis.fr/) to distinguish tasks, references and
explanations. [Good Docs templates](https://www.thegooddocsproject.dev/templates)
are optional starting points, not required page structures.
