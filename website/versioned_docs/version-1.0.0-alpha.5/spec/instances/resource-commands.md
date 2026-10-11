---
title: Service Resource Commands
---

# Service Resource Commands

### PR-REQ-0328 - Resource inspection and access commands {#pr-req-0328---proposed-human-resource-inspection-and-access}

```text
pactrun service-storage list <instance> [--retained]
pactrun resource list <instance> [--retained]
pactrun resource show <instance> <resource-id> [--retained]
pactrun resource observe <instance> <resource-id> [--retained]
pactrun resource locate <instance> <resource-id> --intent <read|write> [--retained]
```

Existing global storage-root selection, Instance reference resolution, human
escaping and exit conventions apply. Unknown/duplicate flags and missing intent
are syntax errors. The resource operations do not add a raw-path piping guarantee, automatic tool
launch, stdin/file-content import, read/write command, or resource-discard command.
Current JSON/JSONL presentation is owned by the
[CLI machine contract](../interfaces/machine-output.md). Unsupported persistence
stores are refused without conversion or data deletion.

Default selection is active only. `--retained` selects retained only, not an
inclusive union and not fallback resolution. IDs use their exact typed spelling.
Lists sort by ID; preserved uncommitted allocations sort by allocation ID after
published retained entries. Empty lists succeed. A missing selected identity
fails rather than searching another Instance, role or native path.

| Command | Contract |
| --- | --- |
| service-storage list | Show storage identity, role and declaration Revision; no native path or recursive filesystem scan |
| service-storage list --retained | Include published retained storage associations and separately labelled protected/unassociated target-attempt allocations with origin Revision/Run and allocation ID; these are not active associations or reattachment candidates |
| resource list | Show identity, storage identity, kind, role and exposure/route; do not probe existence or print native paths |
| resource show | Show the selected exact published declaration, portable locator, declaration Revision, stored exposure/route and effective route; no service contents |
| resource observe | Make one no-follow administrative existence observation; Present also reports observed kind and whether it matches; Absent is not an empty file; Unknown carries a fixed safe cause |
| resource locate --intent read | Require readable exposure and a safely resolved Present object of matching type; print its native path only |
| resource locate --intent write | Require active resource with direct mutation route and safe matching object, or an Absent leaf with an existing safe parent; print its native path only, never perform the write |

`hidden` suppresses native path/content exposure, not contract metadata or the
operator's administrative existence observation. Hidden exposure is not OS
access control. Retained
read access follows the last published declaration's readable setting. Retained
write location is always refused until explicit Migration reattachment; a former
Action or direct route is not silently revived under another active Revision.
An operation-mediated route prints a safe Action-name hint on refusal but never
invokes it or fabricates parameter values.

Directory exposure grants its subtree, not a filtered view: a hidden child
declaration does not revoke a separately granted readable/direct parent scope.
Authors must not expose the parent if its contents must not be exposed. Live
resources have no inferred Managed Input Secret floor; they may contain sensitive
service data. Hidden/unavailable defaults avoid accidental path grants, but are
not encryption or OS access control. These commands neither declassify Managed
Inputs nor waive existing Snapshot export/declassification authorization.

Observation of a wrong-kind leaf can succeed as Present with kind_matches=false;
it does not satisfy authority qualification. A linked/unsafe prefix or an I/O
failure yields Unknown, never inferred Absent. Resource-only locate does not
create missing service parents. Multiply linked regular files and symlink/reparse
traversal are refused for mediated access, even though a native OS user may have
other access outside Pactrun's contract.

Absent is observed only inside a healthy owned allocation. A missing protected
allocation root produces Unknown with allocation_unavailable, not an Absent
resource result and not permission to recreate the Storage.

These are Observe queries: no Run, staging, lease, schema upgrade, allocation,
reattachment, risk resolution or InstanceStateVersion mutation. They can operate
while a manual-recovery guard exists; printing a direct write path is permission
under the declaration, not performing a managed execution or clearing the guard.
It provides no quiescence, validation, hot-edit safety, reload/restart, Snapshot,
service rollback, conflict detection or linearization with external writers.

Exit 0 means a successful query (including Absent observation); exit 2 means
syntax failure; exit 1 means operational failure, Unknown observation or refused
path disclosure. Broken stdout is failure and does not imply rollback of already
printed text. Output is human-escaped, not safe shell source. A successful locate
is the command in this group that emits native service paths; errors and ordinary
Run inspection do not leak them, service contents or derived content hashes.

### Diagnostics and existing operations {#proposed-diagnostics-and-existing-operations}

The following entries use error owner `service_storage` and their fixed
categories from the [error taxonomy](../interfaces/errors.md).

| Code | Category | Meaning |
| --- | --- | --- |
| unknown_resource | resolution | no association in the requested active/retained role |
| exposure_denied | admission | declaration does not permit requested user disclosure/route |
| observation_unknown | execution | safe definitive observation could not be made |
| resource_absent | admission | requested read or presence requirement needs an existing object |
| resource_kind_mismatch | admission | observed object cannot satisfy the declared access kind |
| unsafe_path | admission | owned-root/prefix/link/path representation qualification failed |
| mapping_conflict | relational_semantic_validation | wrong source role/association, implicit adoption or invalid continuity; intrinsic writer validation belongs to the Revision baseline |
| allocation_unavailable | execution | protected allocation missing/unusable; no automatic recreation |
| target_publication_rejected | execution | target proposal cannot publish under current owner/boundary/risk checks |
| corrupt_storage_state | persistence | durable allocation/association invariants are violated |

The error catalog uses the owners `revision_core` and `hook_protocol`
under the [error taxonomy](../interfaces/errors.md). Only registered
owner/code pairs are stable error references. Intrinsic validation also uses
internal codes such as `invalid_service_declaration` and `invalid_service_mapping`;
their appearance in a validator or vector does not register a stable error.
New stable references require owning requirements, catalog registration, and
negative fixtures. Do not map raw library strings to speculative stable codes.

Instance create uses the existing command with the eager allocation semantics
of the current [Revision baseline](../packages/revision-format.md).
Migration keeps the path-ID/--plan/override/input
spelling. Its Plan shows declared service mappings and unresolved runtime checks,
not host paths or a service-state snapshot. Run show adds committed service-edge
evidence and preserved target-allocation identities when present, explicitly
distinguishing them from published resources. No failed-attempt adoption CLI is
provided. Snapshot CLI and Snapshot integrity/bundle encodings are unchanged.

**Verification: PR-TEST-0350, PR-TEST-0353, PR-TEST-0354, PR-TEST-0355, PR-TEST-0356, PR-TEST-0357, PR-TEST-0358, PR-TEST-0359, PR-TEST-0383, PR-TEST-0387.**
