---
title: Operation Artifact Persistence
---

# Operation Artifact Persistence

### PR-REQ-0392 - Lifecycle and edge-scoped artifact storage

The current Persistence `1.0-alpha.6` retains `run_scoped_artifacts` and
`run_scoped_artifact_chunks`. These rows belong to retained Runs, not live
Instances. The existing Action terminal artifact representation remains unchanged.
Capture and Restore ordinary artifacts use the new scoped representation. Neither
immutable Revision/Snapshot bytes nor existing artifact payloads are rewritten
by the additive schema upgrade.

A scoped artifact has one Run, an authored output identity, and either a singleton
Capture, Restore or Cleanup scope or an exact Migration source/target edge on
that Run's immutable path. The private `edge_index` is -1 for singleton operations
and the zero-based path edge index for Migration. It is not a user-selectable
display ordinal. Public edge selectors
resolve the exact source and target Revision digests within the Run.

The private BLOB key is the signed edge index as eight big-endian bytes followed
by the output identity's UTF-8 bytes. Readers MUST verify this encoding and the
operation/edge relationship; the key never replaces public output identity.
Chunk ordering, completeness and the existing per-artifact size bound apply.
Same-name artifacts in different edges remain distinct. An operation MUST NOT
publish an artifact under another edge's identity or through both physical
representations for the same logical artifact.

Successful Cleanup publishes its eligible ordinary artifacts atomically with
the authorization to finalize deletion. Reopening or resuming finalization MUST
retain those bytes without re-executing the Hook or resubmitting artifacts.
Migration publishes each successful edge's ordinary artifacts with that edge's
managed-state boundary. Failure diagnostics publish with the failure boundary.
Artifact retention must not be lost when the Instance is retired, a later edge
fails, or the owner is lost after an acknowledged durable boundary.

The tables introduced by alpha.5 remain unchanged. Source schemas alpha.1 through
alpha.5 upgrade through the existing exact-schema, writer-exclusion and
atomic-publication rules. Tables absent from the source begin empty; existing
artifact rows and bytes remain intact. No historical reports or successful
submissions may be fabricated. A source schema
with unknown additions, damage or live/unprobeable writers remains untouched.

**Verification: PR-TEST-0718, PR-TEST-0725, PR-TEST-0726, PR-TEST-0732, PR-TEST-0733, PR-TEST-0734, PR-TEST-0735, PR-TEST-0736.**
