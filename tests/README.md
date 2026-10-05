# Running tests

Run commands from the repository root. Choose the entry point that covers the
change; the broader commands already include the lower-level checks.

| Command | Scope |
| --- | --- |
| `cargo fmt --all -- --check` | Rust formatting |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Rust lint checks |
| `cargo test -p pactrun --test invoke_cli --all-features` | Focused executable invocation tests |
| `cargo xtask system-test` | System scenarios and the invocation CLI target |
| `cargo xtask rust-ci` | Formatting, Clippy and all workspace Rust tests |
| `cargo xtask ci` | Format conformance, Rust checks, website typecheck and documentation build |

For a narrower change, select the owning Cargo target and behavior filter, then
check that the intended tests actually ran. Requirement coverage uses stable
Test-ID and Verifies comments, not file names. Full CI needs the Node/pnpm versions
in `website/package.json`; platform and tooling checks also use Python 3.11 or newer.

Process and filesystem tests need a supported persistent filesystem. Keep test
outputs in the workspace. On Linux, run the full test commands serially:

```sh
RUST_TEST_THREADS=1 cargo xtask ci
```

Unrelated fork/exec activity can inherit another test's open-file-description
lock. Explicit concurrency tests still create their own competing processes.
The CI workflow defines its assertion-preserving test profile.

## Snapshot capacity

These opt-in tests write real bytes, not sparse placeholders. On the Linux test
host, build a release-profile library test executable:

```text
cargo test --release --lib --no-run
python3 tests/snapshot_capacity_evidence.py PATH_TO_RELEASE_TEST_EXECUTABLE
```

Replace PATH_TO_RELEASE_TEST_EXECUTABLE with the executable path printed by Cargo,
not the ordinary CLI binary. The runner executes the cases sequentially, records
the binary hash and measures each process's peak RSS. Its default sequence covers
small, medium and beyond-former-ceilings round trips through Capture, verification,
Restore, export and import. The largest case requires 160 GiB free; the other
cases require 12 GiB. Use `--cases small` for a bounded smoke check, not evidence
for the largest case.

Peak RSS is paired with zero-inline-content and bounded database/WAL assertions;
it is not a host cache budget or throughput guarantee. An ordinary Cargo test
pass does not include these ignored capacity cases.

## Native package acceptance

The ignored `native_package_acceptance` target invokes
`tools/native_package_acceptance.py`. Inspect its arguments without installing
anything:

```text
python tools/native_package_acceptance.py --help
```

Set `PACTRUN_NATIVE_ACCEPTANCE_ARGS` to a JSON array of argument strings:

| Argument | Input |
| --- | --- |
| `--root` | A new, test-owned directory; it must not already exist |
| `--manager` | The source-qualified Scoop/Homebrew manager template |
| `--xtask` | The source-qualified xtask executable |
| `--parts` | Four platform release-part.json files: Windows and Linux for both versions |
| `--versions` | Explicit baseline and candidate versions when not using the default fixture pair |

On Windows, temporary Scoop PATH changes also require explicit approval and
`PACTRUN_NATIVE_ACCEPTANCE_ALLOW_USER_PATH=1`. Then run:

```text
cargo test --locked --test native_package_acceptance --all-features -- --ignored --nocapture
```

The default mode tests isolated source-selection fixtures. Public acquisition
uses the runner's explicit `--public-source` and `--public-version` options;
it is not implied by a default fixture pass. See [delivery tools](../tools/README.md)
for the candidate build and publication boundary.
