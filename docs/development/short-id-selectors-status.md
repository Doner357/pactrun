---
title: Short Object ID Selectors
---

# Short object ID selectors

**Status: Implemented and verified; ready for authorized local integration, 2026-09-23.**

The operator approved unique-prefix CLI selection and local commit/merge to
`develop` after successful verification. The owning rules are in the
[selector contract](../spec/contracts/cli-id-selectors.md); reference sources are
recorded in [design references](./design-references.md).

## Implementation

- Typed CLI selectors resolve once through read-only catalog queries before
  staging, payload acquisition or execution. Full IDs retain existing semantics.
- Run, Snapshot, historical Instance, allocation and Revision operands include
  mutation, export and cursor positions. Authored names and state tokens remain exact.
- Migration path fingerprints and operator Input target digests resolve within
  a captured Instance/Package/target context. The Instance identity and selected
  full path are fixed before later admission checks. Candidate enumeration remains
  paginated and cancellable; complete path selectors avoid prefix enumeration.
- Human lists use globally unique abbreviations, with `--no-trunc` available.
  JSON/JSONL retain full IDs and expose bounded typed selection-error candidates.
- The cancellable stdin adapter starts its reader only on demand, so resolving
  an invalid/ambiguous ID never consumes input bytes in a background reader.
- No persistence schema, canonical identity, Hook protocol, authorization or
  recovery semantics change.

## Verification and integration

The tests cover lexical boundaries, hidden-page
collisions, namespace isolation, typed errors, frozen resolved identities, exact
CAS tokens, complete-ID idempotence, and actual Snapshot/Migration/retirement flows.

| Verification | Result |
| --- | --- |
| Linux library suite | Passed: 512; existing ignored: 4 |
| Linux executable system suite | Passed: 74 |
| Other Linux integration tests | Passed: 23 |
| xtask suite | Passed: 40 |
| Rust formatting, Clippy and cross-language conformance | Passed |
| Windows CLI group | Passed: 61 |
| Windows-specific executable regressions | Passed: 5 |
| Windows Migration integration | Passed: 11 |
| Windows short-selector and affected Snapshot scenarios | Passed |
| Documentation tests, typecheck and production build after catalog correction | Passed |

The configured persistent workspace is
`/home/test/pactrun-test-workspace/cli-interface-HImYbPZP`. The final runtime/test
input archive SHA-256 is
`519c5afcc6346fe298841cf15cbc5aa4ad393e60b72a3eb87f3c0f2444547d86`.
`cargo xtask ci` completed all Rust/conformance stages; its documentation stage
identified missing catalog/navigation registration for the new contract. Only
Markdown navigation and verification records changed afterward. Traceability,
typecheck and the complete docs-build were rerun; the unchanged runtime reuses
the recorded full-suite evidence. This is not a fresh all-stages CI invocation
after the documentation-only correction.

The first runtime pass also exposed an old Snapshot-list assertion that expected
a full ID without requesting `--no-trunc`. It now requests the full view, and a
separate regression verifies that the default short ID resolves correctly. The
final Rust/system suites above include both assertions.

Linux used test optimization level 1, debug information disabled, debug assertions
and overflow checks enabled, and one test thread. Four pre-existing explicit
capacity/RSS acceptance tests remain ignored. No service deployment or publication
was performed. The CI process exited and zero delivery-spool directories remained;
the dedicated source/build workspace, caches and verification logs are retained.

Remote evidence: `target/short-id-full-ci.log`, `target/short-id-final-preflight.log`,
`target/short-id-source.sha256`, and `target/short-id-docs-closeout.log`.
Local focused evidence is retained under `target/short-id-*.log`.

The existing IPC and human/JSON/JSONL work on the same feature branch is included
in the integration boundary. Unrelated workflow and archive files remain excluded.
Push and publication are outside the operator's authorization.
