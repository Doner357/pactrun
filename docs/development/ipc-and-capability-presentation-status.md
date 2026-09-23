---
title: IPC Initialization and Capability Presentation
---

# IPC initialization and capability presentation

**Status: Implemented, verified and locally integrated on 2026-09-23.**

The subsequent [short-ID integration record](./short-id-selectors-status.md)
records the authorized implementation commit and local `develop` merge, including
this IPC/capability work. The verification details below retain their original scope.

The operator approved automatic safe Linux IPC location handling, accurate Core
and built-in Loader initialization attribution, and presentation metadata in
existing inspection/preview commands. This is separate from pending product
versioning/baseline consolidation and does not authorize publishing or changing
the reconstructed Gitea trial.

## Contracts and implementation

- [PR-REQ-0361/0362](../spec/contracts/hooks-recovery-and-cleanup.md#pr-req-0361---owner-private-ipc-initialization)
  own fallback, isolation, private Loader evidence and error-stage semantics.
- [PR-REQ-0363](../spec/contracts/cli-json-v1.md#pr-req-0363---capability-presentation-at-inspection-boundaries)
  owns current author information at existing query and plan boundaries.
- The error catalog adds two execution identities; CLI JSON V1 adds presentation
  and nullable safe failure detail. No existing error code is repurposed, no
  historical Run is rewritten, and no persistence/Hook/Core format is activated.
- Linux uses a qualified system temporary root when representable and qualified
  `/tmp` when endpoint length or representation requires it. General TMPDIR and
  managed-data locations remain unchanged. Pinned directories constrain cleanup.
- Loader startup evidence is bounded private execution data, not a public Hook
  message or recovery authority. Helper binding precedes session_ready.
- Capability queries batch exact Revision definitions and metadata in read
  transactions. Mutable descriptions remain outside plans, identity and history.

## Verification scope

PR-TEST-0570 through PR-TEST-0577 cover native path budgets, private evidence,
sanitized diagnostics, capability selection, actual lifecycle CLI mappings,
metadata queries/previews, real Loader initialization failure and namespace-safe
cleanup. Existing cancellation, timeout, interactive terminal, recovery,
concurrency, disclosure and output-failure tests remain applicable.

| Status | Evidence |
| --- | --- |
| Passed | Fresh complete configured-remote `cargo xtask ci`: conformance, formatting, workspace/all-target/all-feature Clippy, workspace tests, site typecheck and production build |
| Passed | Linux library: 502 passed; system: 72 passed; xtask: 40 passed; all additional CLI process suites passed |
| Passed | Windows formatting and workspace/all-target/all-feature Clippy |
| Passed | Windows focused CLI library: 52; Shell Loader system: 23; native-process/console system: 5; new capability-metadata system scenario: 1 |
| Passed | V1/V2 long-TMPDIR matrix, native-byte/non-Unicode selection, all five managed-operation initialization mappings, actual Loader pre-ready failure, namespace replacement and concurrent private directories |
| Passed | All 404 source-manifest entries matched before and after the complete remote gate; source-isolated build artifacts were used |
| Passed | CLI JSON schema matches the explicit projections; 25 documentation/link/text-edition checks and production site build |
| Not run | Four pre-existing explicitly ignored capacity/RSS cases; no new ignore or relaxed product timeout/assertion was introduced |

The remote test profile used `CARGO_PROFILE_TEST_DEBUG=0`,
`CARGO_PROFILE_TEST_OPT_LEVEL=1`, `CARGO_PROFILE_TEST_DEBUG_ASSERTIONS=true`,
`CARGO_PROFILE_TEST_OVERFLOW_CHECKS=true` and `RUST_TEST_THREADS=1` on persistent
storage. Explicit concurrency and cross-process tests remain concurrent internally.
The input-manifest SHA-256 is
`f886b0749ca9660a5cb8127aa3eb6f9168e2252ac83eeb24ff411f4e3ec5f994`.

The first full invocation passed Rust checks but stopped because pnpm refused a
node_modules link into an older workspace. Only that new link was removed; the
old workspace was preserved. Isolated frozen-lockfile dependencies were installed,
documentation preflight passed, and a second complete `cargo xtask ci` passed
from start to finish. Earlier fixture/schema-candidate failures are not passing
gate evidence; their corrections preserved the intended assertions and contracts.

This results-only closeout changes this Markdown record, not runtime code, tests,
schemas, dependencies or build inputs. Its documentation checks are rerun; the
unchanged runtime reuses the recorded full-gate evidence, not another fresh CI run.

## Retained resources and authorization

At this initial verification checkpoint the feature was uncommitted; its later
authorized integration is recorded above. Source archives, manifests, build outputs,
isolated website dependencies, generated documentation and logs remain in the
dedicated remote verification workspace; local build/evidence artifacts remain
under target/. Dedicated IPC scratch is cleared after the final documentation
checks. No preview server, product service, public exposure or publishing was
started. Existing unrelated untracked archives and Pages configuration are intact.

No Gitea/Docker deployment is part of this change. The prior temporary trial and
its reconstruction archive remain separate evidence and are not modified.
