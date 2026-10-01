---
title: Retire Instances and reclaim storage
---

# Retire Instances and reclaim storage

Choose retirement according to what should happen to service data. Review the
Pack's Cleanup behavior and maintain independent backups before deletion.
This is a deliberate lifecycle task, not a required follow-up to installation
or a successful Action. Identify the live Instance or retained deletion attempt
first. If you only want to inspect an Instance, use [inspection and operation guidance](./run-and-diagnose.md).

## Delete with Cleanup

```text
pactrun instance delete demo --plan
pactrun instance delete demo
```

Inspect the plan before execution. Use `--if-version <token>` when acting on a
specific observed state. Deletion follows the authored Cleanup contract and
retained deletion obligations. If Cleanup fails, inspect the attempt rather
than assuming the Instance or service data is gone.

```text
pactrun instance deletion list
pactrun instance deletion show <instance-id>
pactrun run show <run-id>
```

Manual completion confirmation is an explicit assertion of completed Cleanup.
Use it only after verifying the required external work, with the matching
attempt and state token shown by inspection.

## Abandon and retain service data

Before choosing abandonment as recovery from a failed finalization, read the
[container-permission limitation](#container-permissions-in-the-first-preview).
Abandonment does not repair filesystem permissions or guarantee later discard.

```text
pactrun instance abandon demo --plan
pactrun instance abandon demo
pactrun service-storage detached list --no-trunc
```

Abandonment ends Pactrun management and preserves remaining service data.
It is not a service shutdown or a backup. Detached allocations remain separate
from ordinary garbage collection.

`service-storage detached discard <allocation-id> --confirm-discard` is
irreversible disposal of the selected allocation. Inspect it, stop dependent
service use, and verify your backups before invoking it.

## Container permissions in the first Preview

The `1.0.0-alpha.1` Authentik evaluation exposed a retirement limitation:
container-created PostgreSQL and Redis directories had ownership or permissions
that prevented the ordinary Pactrun user from completing storage finalization.
ServiceStorage authority does not grant operating-system access to service files.

Cleanup can report `hook_completion_status: success` while deletion reports
`outcome: failed` with `service_storage/allocation_unavailable` at
`finalize_storage`. This is not successful retirement. Inspect the retained
deletion attempt and Run; `recovery_risk: clear` does not mean all storage was
reclaimed or that an earlier Instance recovery guard was cleared.

Do not replay completed Cleanup, guess old service paths, or recursively delete
an original Compose directory: partial finalization may already have moved some
locations. Abandonment and detached discard do not bypass filesystem permissions;
even detached inspection can fail when access is unavailable. Preserve the
records and evidence and obtain administrator-assisted inspection of the exact
retained allocation before considering irreversible recovery. Do not run a
blanket ownership change, permissive chmod, or privileged retry as routine setup.
Administrator cleanup in the evaluation was a separate recovery outcome, not a
passing ordinary-user deletion path.

Pack authors must evaluate the service's UID, directory permissions and Cleanup
lifecycle together, including initialized database data rather than only an empty
directory. No generally validated permission-handoff recipe is supplied by this
Preview.

### Alpha.2 source correction boundary

The alpha.2 source correction allows identity-qualified read-only handoff without
requiring directory-content read access, and reports safe permission/busy reasons
without revealing paths. It does not grant permission to delete foreign-owned
data or replay Cleanup. Filesystem traversal and identity prerequisites still
apply, so handoff is not guaranteed for every inaccessible or replaced location.

A corrected disposable Authentik evaluation Pack keeps PostgreSQL/Redis data
under the invoking host user's UID/GID from initial deployment. Its initialized
data passed normal retirement; this is not a migration or ownership-repair recipe
for existing data. The public package-manager release remains alpha.1 until a
separate alpha.2 artifact delivery is announced.

## Remove retained objects deliberately

Snapshot, Run, Artifact, and Revision deletion have separate commands and
retention constraints. Export anything you need before deleting it. Artifact
export requires `--authorize-sensitive-export`; choose a protected destination.

```text
pactrun storage gc --plan
pactrun storage gc
```

Garbage collection is store-wide: the plan and execution cover eligible
unreferenced managed content throughout the selected store, not only the Instance
used in the examples above. Review the selected store and the complete plan. It does not
replace Instance retirement or authorize discarding detached service data.

More detail: [User reference](../pactrun-users/reference/index.md).

<details>
<summary>Maintainer sources (optional)</summary>

Evaluation: [Authentik black-box follow-up](../development/public-preview-delivery.md#authentik-black-box-follow-up).
Correction evidence: [alpha.2 retirement qualification](../development/alpha2-retirement-fix.md).
Contracts: [managed-object lifecycle](../spec/behavior/managed-object-lifecycle.md)
and [Instance retirement](../spec/execution/m7-instance-retirement.md).

</details>
