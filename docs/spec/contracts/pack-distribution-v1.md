---
title: Pack Distribution V1
---

# Pack Distribution V1

**Status: Approved C implementation contract, 2026-09-22.**

<!-- spec-navigation:start -->
## Reading map (informative)

This contract owns source/distribution Pack containers, portable metadata carriage
and exact Revision transport. Read [authoring](./authoring-model.md) for source
projection, [identity](../foundations/identity-and-state.md) for immutable and
metadata semantics, and the [C status](../../development/pack-transport-status.md)
for verification and integration. Container bytes are not Revision identity;
[Snapshot transport](./snapshot-bundle-v1.md) remains a separate contract.
<!-- spec-navigation:end -->

### PR-REQ-0354 - Pack containers and exact Revision transport

`pack install <path>` MUST accept source or distribution content in a directory
or ZIP file. The conventional extension is `.pack`; extensions MUST NOT select
or validate content. Source has root `pactrun.yaml`; distribution has root
`pactrun-distribution.json`. Both markers together MUST fail, without parse-failure
fallback, nested-root discovery or wrapper stripping. Source retains its authoring
versions and projection. Distribution never reruns authoring or executes Hooks.

Distribution regular members are exactly `pactrun-distribution.json`,
`revision-core.json`, `runtime-content.json`, and `blobs/sha256/<64 lowercase hex>`.
Safe directory entries are permitted. The descriptor is strict JSON with exactly
`kind: "pactrun_distribution"`, integer `format_version: 1`, canonical `package_id`,
`revision_digest` (`sha256:` plus 64 lowercase hex), and optional
`portable_metadata`. Components MUST be exact canonical bytes under their own
independent codec versions. The runtime closure determines the complete distinct
blob set, including empty blobs. Every byte and computed identity MUST be verified
before publication, including repeat installation of an existing identity.

ZIP permits only Stored and Deflate, single-volume ZIP/ZIP64, without encryption
or self-extracting prefixes. Export uses Deflate. Reject duplicate/conflicting
names, unsafe paths, links, special files, inconsistent records, truncated streams
and unsupported methods. Never apply archive attributes to the host. Runtime
descriptors alone determine executability. Ordering, timestamps and compression
do not affect domain identity. Bound metadata before allocation; stream payloads
with checked accounting and existing component limits, not a new runtime-byte quota.

The bounded Pack profile permits at most 65,539 ZIP entries, at most 64 MiB of
container metadata, paths of at most 1,024 UTF-8 bytes (plus a directory slash),
and at most 16 MiB for each source manifest, distribution descriptor or canonical
component. ZIP member names are exact UTF-8 safe source-relative paths; ambiguous
ASCII case aliases and file/directory collisions are rejected. These are structural
limits, not runtime-content byte ceilings. Resource exhaustion remains an I/O
failure. Optional JSON members are omitted rather than null. Portable metadata
arrays default to empty, reject duplicate semantic keys and use the exact closed
target, field, label-source and provenance variants of PackSourceYamlV1.

The common installation boundary uses crate-private typed content, owned staged
bytes and metadata plans. Frontend/container types do not become Domain contracts.
No public Candidate API or stable Rust API is introduced.

**Verification: PR-TEST-0538, PR-TEST-0540, PR-TEST-0542, PR-TEST-0544, PR-TEST-0545, PR-TEST-0548.**

### PR-REQ-0355 - Pack metadata conflict policy

Export omits metadata by default. `--include-portable-metadata` carries all portable
labels, presentation and provenance for the exact Revision, never aliases, notes
or trust. Portable JSON uses closed source-format metadata shapes independently
of authoring; distribution installation MUST NOT generate or parse source YAML.

All installation forms accept `--metadata-conflict overwrite|keep`. Without a
policy, different current presentation values reject the whole authoritative
transaction with actionable non-interactive diagnostics. Missing values are added,
equal values are idempotent. Overwrite applies only carried conflicting fields;
keep preserves those local fields and reports skipped items. Absent fields are
unchanged. Label/provenance semantic tuples are unioned without resolving legal
label ambiguity. No policy bypasses immutable validation. Conflict observation and
mutation MUST share the publication transaction, including concurrent installs.

**Verification: PR-TEST-0539, PR-TEST-0543, PR-TEST-0545, PR-TEST-0546.**

### PR-REQ-0356 - Revision export and publication safety

`revision export <reference> --output <base-path> [--include-portable-metadata]` resolves
one exact installed Revision or refuses ambiguity. Export needs no original source.
The CLI always appends `.pack`, including when the base already ends in `.pack`,
under [PR-REQ-0357](../behavior/command-and-output-reference.md#pr-req-0357---specialized-envelope-export-filenames).
Acquire complete content and selected metadata consistently under deletion/GC
coordination, then compress outside the transaction from owned staging.

Warn that output is unencrypted and may contain sensitive authored bytes; require
no sensitive-export flag. Never include Instance Inputs/Secrets, service storage,
Workspaces or Runs. No hard-coded-secret detection is promised. Privately stage
output and atomically publish without replacing the destination; report errors
after final-name publication accurately. Installation publishes Revision and
metadata atomically. Precommit failures leave no partial authoritative objects;
unreferenced blobs retain existing collection semantics. Cancellation cannot
report success. No Run, persistent Bundle object, implicit upgrade or new schema
is introduced.

**Verification: PR-TEST-0538, PR-TEST-0541, PR-TEST-0543, PR-TEST-0544, PR-TEST-0545, PR-TEST-0546, PR-TEST-0547, PR-TEST-0548, PR-TEST-0549.**
