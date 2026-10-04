---
title: Shell Loader Format and Activation Review
---

# Shell Loader format and activation review

**Status: Format gate closed; Core V3 Frozen by status-only closeout.
Not a commit, integration, release or publication record.**

The operator approved the [baseline](./shell-adapter-loader-baseline.md) and
continuous S0-S6 execution. This review closes the format gate inside that scope;
it does not authorize Git integration, release, publication or another milestone.

## Format and authority review

- Core V3 explicitly adds shell_loader to the inherited V2 closed schema. YAML V3
  explicitly selects it. V1/V2 reject the new launch and retain exact old bytes.
- Version 3 changes the Core document and revision frame together. Rust and an
  independent Node oracle compare canonical Core/content components, full frames
  and four checked-in shell identity digests from normalized inputs.
- Hook Protocol V1/V2 are independently selected from admitted facts; Loader
  cannot negotiate or infer another version from a peer preamble. Framing and
  the canonical 16 MiB limit are unchanged. Private helper-envelope overhead is
  not a second semantic protocol or increased Core frame allowance.
- All launch selections retain exact-path admission and revalidation, including
  Migration, Snapshot and Cleanup. Loader adds no grant or durable state. The
  existing V10 storage schema remains exact; no persistence migration is needed.
- One built-in loader and native helper process avoid shell-specific imported
  libraries, bundled interpreters, PATH aliases, string-code launch construction
  and daemon lifecycle. File interfaces preserve opaque bytes and explicit grants.

## Acceptance and compatibility boundary

The [implementation record](../shell-adapter-loader-status.md) maps capabilities
to actual test IDs and separates focused results from final full CI. Acceptance
includes POSIX sh/Bash and Windows PowerShell 7/5.1, plain scripts, every existing
operation, transforms, terminal channels, loss, cancellation/timeouts, explicit
outputs, Secret handling and maximum-frame/beyond-frame file paths.

The conformance, platform and scoped integration gates passed. Core V3 Freeze
is a status-only change: no regeneration of expected schema, component
bytes, framing or digests. YAML remains Candidate. Loader helper availability
does not advance Cargo's product version or grant a formal-release compatibility
promise. Machine-readable CLI output and final baseline consolidation remain
separate tasks. Existing historical integration evidence is not rewritten.

## Tested inputs and evidence reuse

The configured-remote complete `cargo xtask ci` passed for source-manifest SHA-256
`b5807049d1dec3887c7b935f577dee58db32a576a9d02f88cfa5f110752419e5`.
Its log SHA-256 is
`5558ad1e694f22afa06a27bf307af058a946fe14544baaea62c47b613069a142`.
The source manifest was checked again after that gate. This is the full-suite
evidence, not a claim that every later edit received another full-suite run.

The bounded post-review delta contains only Loader-client control-integer
validation before binary64 conversion, refusal of file-shaped Windows pipe
locators, owner-controlled waiting for busy helper connections, their tests and
verification references. No Domain, persistence, authority/publication policy,
Core/Hook schema, identity bytes, dependency or toolchain was changed by that delta.
The existing proven exact-integer parser is reused rather than reimplemented.

Delivered runtime source-manifest SHA-256
`3adbc883be5beff8939245a0afa0e4ba2b81cb6e7ed511941c7060c0d8d1741c`
passed fresh Windows and POSIX Loader checks: 21 system tests on each host, six
adapter unit tests, Core/source conformance filters, formatting and Clippy.
Nine existing Windows native process/terminal probes also passed. The independent
Node oracle matches all four Core V3 component/frame/digest cases. Test IDs and
commands are recorded in the implementation record.

Under the repository's risk-based validation policy, unchanged product-suite
evidence is reused and the complete affected adapter checks are fresh. This
bounded delta is not mislabeled as a second complete CI pass. Status-only Freeze
and navigation closeout receive fresh metadata/traceability and site checks.

The pre-Freeze V3 corpus SHA-256 is
`2794505faa1fed09a2597ab9c5cded5ff8a0b6cc1d5cac7ae19b7bd3180446a4`.
Only its status string changes at Freeze. V1/V2 Core, Hook and Snapshot expected
vectors are untouched. Self-review is not independent runtime verification;
the Node oracle's independence is limited to normalized identity parity.
