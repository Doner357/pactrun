---
title: CLI Presentation Implementation Baseline
---

# CLI presentation implementation baseline

**Status: D S0-S4 approved on 2026-09-22; implemented and verified on the feature
branch, with Git integration separate.**

See the [implementation and verification record](../cli-presentation-status.md)
for tested inputs, acceptance scope and retained resources.

The operator approved one typed backend result with human and JSON renderers.
JSON is a versioned public interface for tools and agents; human presentation
may evolve for readability. Neither renderer interprets arbitrary Hook output
or transforms exported payloads. Existing authorization, disclosure, identity,
transaction, recovery and exit semantics remain authoritative.

## Approved decisions

- A leading `--format human|json` selects presentation; omission means human.
  Terminal detection, pipes and redirects do not select a format.
- JSON uses deliberate command-specific projections, never parsed human output,
  Debug serialization or wholesale serialization of internal structures.
- Ordinary JSON responses use one stdout document, including failures. Raw Input
  stdout export has no success envelope and sends Pactrun errors to stderr.
- Execution using Hook output or interactive terminal contracts is refused in
  JSON mode before execution side effects. Plans remain available. Operators can
  execute in human mode and inspect the retained Run in JSON afterwards.
- No result-file destination, event stream, HTTP/MCP service or repurposing of
  `--output` is introduced.
- Improve human grouping, empty collections, paging and actionable errors while
  preserving the facts and disclosure policy of the shared projection.

## Ordered delivery

| Slice | Deliverable | Exit gate |
| --- | --- | --- |
| S0 | Normative JSON contract, schema/examples, complete command/channel matrix and human improvements | Every public command classified; observable decisions recorded |
| S1 | Format selection, typed errors/projections, renderers and startup handling | Foundation and failure contract tests |
| S2 | Management, inspection, catalogs, history, metadata and Plans | Both presentations cover identical facts |
| S3 | Execution, lifecycle and transport results; raw boundaries | Success, partial failure, cancellation and output-failure evidence |
| S4 | Cross-platform acceptance, complete remote CI and documentation closeout | Actual evidence and bidirectional traceability for affected obligations |

Continue through slices without per-slice approval gates. Escalate new unresolved
observable semantics, required permissions or external blockers only. Commit,
merge, push and publication require separate authorization. Preserve unrelated
untracked files. E follows verified D; this approval does not start E.

The owning contract is [CLI JSON V1](../../spec/contracts/cli-json-v1.md).
