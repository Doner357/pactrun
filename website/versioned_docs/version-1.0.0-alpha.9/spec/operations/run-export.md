---
title: Run Support Export
---

# Run Support Export

### PR-REQ-0404 - Explicit content selection and disclosure authority

`pactrun run export <run-id> --output <path>` copies an available support report
to a new ZIP file at the exact supplied path. It is not a Snapshot, backup,
replay plan or import format. Default content is safe Run metadata and available
Pactrun-owned diagnostics, including the Core phase trace. Raw arguments,
parameter values, native Store paths and terminal streams are not included.

`--include-hook-text`, `--include-artifacts`, `--include-inputs` and
`--include-secrets` independently select retained Hook evidence, currently retained
Artifacts, ordinary historical Inputs and historical Secrets. Selecting Secrets
does not implicitly select ordinary Inputs. Arbitrary Hook text and Artifacts
require `--authorize-sensitive-export`; Secrets require the separate
`--authorize-secret-export`. Authorization alone does not select content. No
configuration file can retain this disclosure consent.

Selected bytes are not sanitized or guaranteed private. Logical identifiers and
timing metadata can themselves be sensitive. Reports MUST identify selection and
unavailable evidence, warn readers to review before sharing, and MUST NOT upload
anything or contact a Pack author automatically.

The command supports machine interface `1.0-alpha.4`. Its result identifies the
Run, `outcome: exported`, copied Input and Artifact counts, a count of selected
Inputs without copied bytes (`unavailable_inputs`, including known absence and
transient values), and `review_before_sharing: true`. The report's per-item states
provide the distinction that the aggregate count cannot. Legacy machine requests
are refused before report publication.

**Verification: PR-TEST-0779, PR-TEST-0780, PR-TEST-0781, PR-TEST-0783, PR-TEST-0784.**

### PR-REQ-0405 - Observational, provenance-qualified copying

Run metadata, retained Hook evidence, Artifact inventory and historical Input
resolution use one consistent Store database read snapshot. A separately stored
Core phase trace is an independent diagnostic observation, not an atomic Run
state receipt. A running Run can change after inspection. Export neither waits
for completion nor implies that the retained inventory is all original evidence.

Input resolution follows [historical references](./input-history.md). Missing
history is not backfilled. Absent, transient, pending-output and reclaimed values
remain distinct; current values MUST NOT substitute. A copied empty payload is
distinct from unavailable bytes. Available Source/Target views retain exact edge
identity. Only selected Input and Artifact payloads are read. Corruption and I/O errors fail
the export, rather than masquerading as normal unavailable evidence.

Source-copy and encoding diagnostics distinguish known access, storage-capacity,
contention and validation failures without echoing backend error text or payload paths. An
ordinary I/O failure is not itself a corruption verdict.

The report contains `report.json` and generated relative `inputs/*.bin` and
`artifacts/*.bin` members for copied content. Member paths are not derived from
Hook-supplied filenames or Input identities. The JSON identifies
`format: pactrun.run-report`, `format_version: 1.0-alpha.1`, the full Run ID,
selection, observed inspection, independently observed Core trace, and per-item
provenance, state and member path. This is an inspection document, not an
authoritative persisted Run representation.

**Verification: PR-TEST-0780, PR-TEST-0781, PR-TEST-0782, PR-TEST-0783, PR-TEST-0784, PR-TEST-0785, PR-TEST-0786, PR-TEST-0794.**

### PR-REQ-0406 - No-overwrite staged publication

All output is first written to operation-local staging. Reads stream bounded
chunks; the report does not require all selected payload bytes in memory. Any
failure before publication discards staging and leaves no final destination.
Publication MUST NOT overwrite an existing object, consume source content, mutate
Run outcomes or extend historical payload lifetimes. Existing publication rules
apply if a destination was installed but its durability cannot be confirmed:
report that partial result accurately and do not remove the published file.
Machine output reports command results, not ZIP bytes. An output-response failure
after publication does not undo publication and MUST NOT claim it never occurred.

**Verification: PR-TEST-0780, PR-TEST-0781, PR-TEST-0782, PR-TEST-0794.**
