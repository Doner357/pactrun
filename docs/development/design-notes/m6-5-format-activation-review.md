---
title: M6.5 Format and Activation Review
---

# M6.5 format and activation review

The operator approved the S0 design and continued S1-S7 implementation on
2026-09-15, including mapped-source consumption. This engineering review closes
the format/conformance gate within that approved scope. It introduces no new
identity, wire, authoring, ownership or compatibility decision.

## Format gate

- Core V2 retains the specified strict JSON/number/Unicode rules and independent
  version-2 framing. The six positive component/frame/digest vectors and fourteen
  raw rejection vectors remain byte-identical to the pre-activation snapshot.
  Rust verification and the independent Node 24 JCS/SHA-256 oracle pass.
- Hook V2 independently selects version 2 and its dedicated preamble. The
  protocol tests reject fallback, foreign/duplicate authorities, invalid target
  handles and ordinary success or risk resolution for transforming Sessions.
- Real ordinary and transforming Hooks, durable Open-before-ack, target receipt,
  zero-exit/tree termination, owner-held publication and crash boundaries have
  automated evidence. Format approval does not claim all milestone gates passed.
- Frozen Core/Hook V1 bytes and meanings are unchanged. YAML remains a versioned
  Candidate authoring contract; explicit source_format 2 selects Core V2 without
  upgrading or reinterpreting installed V1 content.
- V7 custody and the future explicit M7 receipt/upgrade boundary remain as
  approved. This review does not authorize Cleanup, deletion or abandonment.

The pre-activation source manifest SHA-256 is
`08ec33c273a2433e9b131a488349f65947ccae93e788764e988ad4e3d17f68ef`.
Its complete configured-remote `cargo xtask ci` passed, including conformance,
formatting, Clippy, workspace/integration tests, documentation typechecking and
the production documentation build. Every manifest entry was checked again
after the run. The source was tested in the persistent supported-filesystem
workspace, not a temporary-memory filesystem.

The existing positive vector file SHA-256 before the status-only metadata
update is `4df4b41c06e37a06109a229de42258f48ded72189cebdeab6be191778e457058`;
the rejection file is `63d1bca594f71ec406b72c1b831a6fcda1bfd716aeb320280c075dfd83c58a70`.
Freeze must not regenerate expected bytes, schemas or digests. The implementation
review is self-review, not an independent verification claim; the independent
oracle is limited to canonical byte/digest parity.

## Activation and remaining completion gate

Core and Hook V2 are Frozen. The installer selects the declared YAML version
through the existing typed version-neutral acquisition/publication path.
PR-TEST-0383 is the public CLI activation acceptance test, including V1/V2
transitions, stable identity despite live service changes, retention and explicit
reattachment. PR-TEST-0382 adds real split/merge multi-Hook evidence.

Post-activation focused tests are recorded in the implementation status; the
delivery report must supply the final exact-source full-CI result. This review
is not an M6.5 integration, commit or push record. See
the [implementation status](../m6-5-implementation-status.md) for the current
remaining work and validation results.
