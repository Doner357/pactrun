---
title: M8 Recipes — Rejected Design
---

# M8 Recipes and advanced authoring — rejected

**Status: Historical rejected proposal. Removed from the active roadmap by operator decision on 2026-09-16; not deferred implementation.**

## Decision

Remove M8, including install-time Recipes, InstallContext and environment-dependent
Pack generation, from the active implementation plan. Do not replace it with a
renamed Recipe milestone. The old milestone number remains historical and is not
reused. No Recipe runtime was implemented; this decision changes documentation
and planning, not installed data, Frozen formats, YAML behavior or runtime code.

YAML V1/V2 remain the built-in authoring frontends. Their supported declarations
cover the currently implemented Pack capabilities; Hooks implement service logic.
Keep the internal RevisionCandidate / NormalizedPackDefinition boundary. A public,
versioned Candidate entry point for third-party frontends is deferred until a
concrete need exists, not rejected in principle and not a release prerequisite.
Such an additive entry point need not require a product Major change if it preserves
existing authoring meaning, identities and installed-object behavior. No external
API encoding, SDK, execution mechanism or version promise is approved here.

## Why the proposal was rejected

1. **Open-ended Revision variation.** One Recipe can produce different content
   across Linux distributions, architectures, hardware, tool versions and local
   environments, even on nominally identical systems. Both operational declarations
   and owned runtime file digests affect Revision identity. The author may not know
   the set of possible outputs in advance.
2. **No isolated execution environment.** Pactrun manages typed declarations and
   lifecycle operations; it is not a container or an environment-reproduction
   system. Saving generated content does not freeze its host assumptions or bound
   the behavior of all Recipe outputs.
3. **Migration declarations are only part of the problem.** Current target-owned
   inbound edges name exact source digests. Enumerating every generated source is
   not a workable general authoring requirement. Replacing that enumeration with
   compatibility labels alone would not resolve the underlying problem.
4. **Hook behavior can grow combinatorially.** A target's Migration Hook may have
   to handle different source layouts, tools, data representations and environment
   assumptions against different target variants. Different hashes do not always
   need different code, but the proposed generation model gives no bounded support
   space or evidence that one conversion handles them all.
5. **Lifecycle maintenance outweighs installation convenience.** Pack authors own
   service-specific transformations, but assigning responsibility does not make
   open-ended testing, recovery and compatibility affordable. LLM-generated code
   is not evidence of comprehensive coverage. Long-lived Instances and connected
   revisions make this more than an installation-time convenience tradeoff.
6. **No demonstrated need for another frontend API today.** The existing YAML
   frontend can represent current Pack declarations. External authoring tools can
   already prepare YAML plus files before installation; a public Candidate API
   should be justified by actual requirements rather than speculative flexibility.

Static Packs do not eliminate host differences. The decision avoids adding
open-ended install-time generation; it does not claim environment isolation or
automatic service compatibility. It also does not ban networking: a command-driven
CLI may have network-related behavior without being a resident network service.
No new download, package-registry or platform-selection feature is approved here.

## Original proposal (historical, not an implementation instruction)

The removed roadmap entry was **M8 - Recipes and advanced authoring**, with
**State: Proposed.** Its scope was:

> Implement the versioned language-neutral Authoring Contract, InstallContext,
> network- and host-dependent generation, and RevisionCandidate output.

Its completion gates were:

- every frontend converges on the same normalized Candidate boundary;
- authoring metadata and environmental observations do not alter Frozen identity
  semantics unless the relevant format includes them;
- source-dependent generation has deterministic validation and diagnostics.

These goals did not resolve the lifecycle and combinatorial maintenance costs above.

## Retired requirement text

The following text is preserved only as rejected design history. Canonical IDs
and existing anchors remain as retirement notices in the
[runtime-content contract](../../spec/contracts/recipes-and-runtime-content.md);
the IDs are not reused or reported as pending runtime coverage.

**PR-REQ-0137 — former trusted install-time authoring:**

> A Recipe MUST be treated as a trusted install-time authoring program. It MAY
> inspect a provided InstallContext, vary by platform or architecture, fetch from
> the network, resolve mutable upstream references, run authoring tools, and stage
> Revision-owned content.

**PR-REQ-0139 — former Recipe output boundary:**

> The only formal Recipe output MUST be a RevisionCandidate. A Recipe SHOULD
> NOT perform service lifecycle side effects as part of the authoring transaction;
> service behavior belongs in Hooks.

**PR-REQ-0140 — former language-neutral Authoring Contract:**

> The Recipe Authoring Contract MUST be language-neutral at the semantic level,
> versioned independently, and isolated from Pactrun's internal domain types.
> Python, Go, Rust, or other SDKs MUST be adapters rather than separate semantic
> APIs.

The independent Revision digest rule PR-REQ-0138, owned runtime-content rules
PR-REQ-0141/0142 and internal Candidate rules PR-REQ-0121/0122 remain active.
No Migration matching rule or Frozen Core/Hook identity is changed by retirement.

## Planning consequences

M7 remains implemented and integrated. There is no automatically selected next
numbered milestone. [Release readiness](../release-readiness.md) remains required
before formal publication, with scheduling unassigned; removing M8 does not start
that work, reset development data, change the product version or authorize release.

Reconsidering install-time generation would require a new explicit product decision
and evidence addressing these costs, not merely implementing the archived proposal.
