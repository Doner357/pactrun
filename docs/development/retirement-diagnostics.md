---
title: Retirement failure diagnostics and Cleanup guidance
---

# Retirement failure diagnostics and Cleanup guidance

**Status: Implemented, verified and integrated into develop on 2026-10-02;
not publicly released.**

On 2026-10-02 the owner approved safe retirement failure classification,
deletion-obligation guidance, author/operator documentation and regression tests.
Delivery ends at verified integration into `develop`, with temporary remote work
branches removed. No merge to main, tag, Release, Pages publication or native
package catalog update is included. Published alpha.2 artifacts stay immutable.

## Evidence and scope

The Caddy alpha.2 black-box handoff reported successful Cleanup followed by
`service_storage:allocation_unavailable` at `finalize_storage`, with no safe
detail. A caller-owned stale Unix socket remained. Removing only that socket
after explicit handoff allowed discard; a separate still-managed control
Instance migrated to a corrected Cleanup and then retired normally. The latter
does not prove repair of the already partially deleted original Instance.
The private handoff and its fixtures are not shipped or executed by this change.

The Linux finalizer already rejects entries other than regular files and
directories. This work distinguishes that rejection from inconsistent identity
or journal evidence; it does not permit deleting additional entry kinds.
The typed internal marker is classified without parsing OS error strings.
No auto-repair, permission changes, privilege escalation, implicit retries,
Cleanup replay or new public lifecycle state is added.

## Information and compatibility review

- Preserve existing error owner/code identities, admission ordering, accepted
  Run boundaries, state transitions and persisted deletion authority.
- Add optional `reason` to Run failure projections for exact safe storage
  classifications and the recognized deletion-obligation admission refusal.
- Add optional `error.retirement_reason` for direct safe storage errors such as
  detached discard. Unknown/old unclassified failures remain unclassified.
- Add optional `error.deletion_obligation` with the known full Instance ID and
  refused accepted Run ID. The latter is not the deletion attempt ID. Reuse
  `instance deletion show` for the current obligation/attempt; historical
  diagnostic text must not claim the condition is still active.
- Do not extend the existing `error.diagnostic.kind` domain. These are new
  optional object members under the existing additional-member tolerance rule;
  the CLI format remains `1.0-alpha.1`. No existing enum/type/meaning changes.
- Human advice and machine facts use the same safe evidence. No native paths,
  entry names, service bytes or arbitrary stored/OS messages are declassified.
- Add operator guidance and a controlled socket lifecycle test recipe for Pack
  authors, not a generally race-safe privileged deletion script.

## Verification

- Runtime candidate: `6107e88c281d4df88dd490ac4294856e87a36de4`.
  PR #16 integrated it into develop at
  `6a5d5bd0dd726d150efb0c4695cdbb01df732a4c` through the protected merge flow.
- Full configured Linux `cargo xtask ci` passed with Rust 1.99.0 on the persistent
  ZFS workspace. All 529 source files matched the candidate commit and were
  checksum-verified before and after the gate. The executable was compiled from
  this workspace, not reused from another checkout.
- Linux: 531 library tests passed (four existing capacity opt-ins ignored),
  17 Migration CLI tests, three retirement CLI tests, 76 system tests, other
  workspace/conformance suites, 89 documentation tests, site typecheck and build
  passed. The existing opt-in native package acceptance test remained ignored;
  no new package or published binary was produced.
- Windows Rust 1.98.1: 70 CLI unit tests, 16 Migration CLI tests, two retirement
  process tests and workspace/all-target/all-feature Clippy passed. Earlier
  focused retirement checks also passed. The enlarged diagnostic context was
  boxed to retain the small CLI error boundary; no lint was suppressed.
- Hosted CI `37014205462` passed complete Windows and Ubuntu verification on the
  candidate. Pages publication was skipped. No protected-branch bypass was used.
- PR-TEST-0652 exercises a real stale Unix socket: Cleanup succeeds, finalization
  fails with the safe reason, JSON/JSONL and history preserve the facts, Action
  and Migration stay blocked without a recovery guard, recovery override does
  not bypass the obligation, and finalization-only retry does not replay Cleanup.
  Abandon/discard still need explicit intent; only the test owner repairs its
  own socket. No automatic socket deletion occurs.
- PR-TEST-0653/0654 cover typed classification, exact-message disclosure limits,
  historical versus current evidence and unrelated errors. PR-TEST-0655 covers
  old/new optional-member compatibility. Removing only the three added members
  and their new definition yields the exact previous schema; existing enums and
  types are unchanged. Existing permission/busy details retain their semantics.
- This is a synthetic native-socket regression, not a rerun or certification of
  the external Caddy Pack. The original handoff, services and untracked user
  artifacts were not modified. Runtime behavior beyond the approved diagnostics
  remains unchanged.

The documentation-only closeout reuses the identical qualified runtime and
requires its own documentation/traceability, typecheck/build and hosted checks
before merge. It does not claim another fresh product-wide run. Local evidence
is retained under `target/retirement-diagnostics-20261002/`; the configured
persistent remote retains the matching source, logs and built documentation.
No new preview server was started; pre-existing preview resources were untouched.
