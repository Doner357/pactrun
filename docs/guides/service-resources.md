---
title: Inspect service resources
---

# Inspect service resources

ServiceStorage holds service-authoritative live data. An Input is a detached
managed value; Workspace is scratch. Choose the correct area before accessing data.
You need a [live Instance](./use-pack.md) whose Pack declares service resources.
If its resource list is empty, this procedure does not apply. The empty first
Instance example does not supply a `database` resource.

## Discover declarations

```text
pactrun service-storage list demo
pactrun resource list demo
pactrun resource show demo database
pactrun resource observe demo database
```

Replace `database` with a declared resource ID. Observation reports existence
and kind; it does not certify health or grant access. A wrong-kind object can be
reported as present without qualifying for an operation.

## Request an allowed location

```text
pactrun resource locate demo database --intent read
```

Use the path only for the declared intent. It is a host location at that moment,
not a permanent identity or a reservation. A resource can contain sensitive
service data even when it has no Input-style Secret label.

For write access, use `--intent write` only when direct mutation is allowed.
If the declaration requires an Action, use that Action. Do not bypass a refusal
by searching internal storage directories.

## Inspect retained resources

Add `--retained` to select retained declarations. This selects retained resources
instead of active resources. Retained write location is refused. Read access
follows the retained declaration and current qualification.

After abandonment, inspect detached allocations with:

```text
pactrun service-storage detached list --no-trunc
pactrun service-storage detached show <allocation-id>
```

Location disclosure requires `--reveal-location`. Discarding detached storage is
a separate destructive action; see [retirement](./retirement.md).

More detail: [User reference](../pactrun-users/reference/index.md).

<details>
<summary>Maintainer sources (optional)</summary>

Contract: [ServiceStorage commands](../spec/behavior/m6-5-service-storage-command-reference.md).

</details>
