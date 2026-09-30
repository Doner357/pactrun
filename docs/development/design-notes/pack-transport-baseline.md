---
title: Pack Transport Implementation Baseline
---

# Pack transport implementation baseline

**Status: S0-S3 approved on 2026-09-22. Implementation acceptance is tracked
separately in the [status record](../pack-transport-status.md).**

The operator approved source/distribution Packs in directories or ZIP-based
`.pack` files, with one `pack install` command. `revision export` emits a Deflate
distribution Pack from exact installed canonical content, not reconstructed YAML.
Folder layout is identical to archive layout. ZIP is the sole container; format
versions remain independent from authoring, Core and persistence versions.

The [owning Spec](../../spec/contracts/pack-distribution.md) records metadata
defaults, opt-in portable carriage, overwrite/keep/refuse conflict semantics,
structural limits and publication boundaries. Source and distribution adapters
converge on a private typed installation core. Public Candidate/SDK APIs remain
deferred. The existing immutable storage and V11 schema are reused.

Rationale: two actual input paths justify separating container acquisition from
authoring and publication; a speculative plugin framework does not. Canonical
transport avoids source reconstruction and projection-version dependency. Metadata
is excluded by default because a Revision's immutable identity does not own local
presentation decisions. Explicit conflict policies provide file-manager-like
control without silently overwriting local values. No export authorization flag
is required for authored Pack bytes; an unencrypted-content warning remains.

Execute contracts, validation/export, installation/publication and acceptance in
order, continuing across slice boundaries. New observable semantic conflicts or
external authorization blockers require an operator decision; routine implementation
choices and repairable failures do not. Git integration and publication are separate.

The subsequent approved [export naming rule](../../spec/behavior/command-and-output-reference.md#pr-req-0357---specialized-envelope-export-filenames)
changes `--output` to a base path for Revision/Snapshot envelopes, with an
unconditionally appended `.pack`/`.snapshot` suffix. It does not change this
baseline's container, identity, payload or authorization contracts. Raw exports
retain exact caller-selected filenames.
