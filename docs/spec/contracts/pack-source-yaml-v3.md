---
title: Pack Source YAML V3
---

# Pack Source YAML V3

**Status: Candidate normative Package authoring contract; versioned, non-Frozen, and implemented.**

<!-- spec-navigation:start -->
Read [YAML V2](./pack-source-yaml-v2.md), [Core V3](./revision-core-format-v3.md)
and [Shell Loader](./shell-loader.md). Actual availability is recorded in the
[implementation status](../../development/shell-adapter-loader-status.md).
<!-- spec-navigation:end -->

### PR-REQ-0350 - Explicit YAML V3 projection

The top-level closed schema, acquisition rules and defaults are unchanged from
YAML V2, except the source MUST explicitly use the plain raw token
`source_format: 3`. The revision projector inserts `format_version: 3` and uses
Core V3 validation. Quoted or floating version tokens, duplicate keys, aliases,
unknown fields and null remain invalid. Installed V1/V2 content is not rewritten.

The Loader launch requires `kind: shell_loader`, `shell`, `command` and `script`.
There are no launcher flags, inline commands, output mapping or implicit shell
selection. Runtime source paths and portable metadata remain outside Core identity
except for their existing normalized identity-bearing projections. This document
does not freeze a public authoring compatibility promise.

**Verification: PR-TEST-0494.**
