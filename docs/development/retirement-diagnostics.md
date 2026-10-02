---
title: Retirement failure diagnostics and Cleanup guidance
---

# Retirement failure diagnostics and Cleanup guidance

**Status: Implementation in progress; not publicly released.**

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

Pending the focused Windows/Linux checks, schema compatibility and disclosure
checks, full persistent-remote integration gate and hosted CI. Results and final
integration evidence will be recorded before closing this milestone.
