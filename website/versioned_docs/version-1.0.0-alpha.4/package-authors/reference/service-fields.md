---
title: Service and Migration fields
---

# Service and Migration fields

Use ServiceStorage for service-owned live data. Use managed Inputs for detached
configuration bytes. Neither a resource declaration nor a successful command
means the service is healthy, stopped, backed up, or coherent.

This page complements [Pack fields](./pack-fields.md). The
[Migration tutorial](../managed-capabilities/migrations.md) provides a runnable
Input transition; [declaration types and identity](./revision-format.md) provides
the complete mapping and validation details within this author section.

## Storage and resources

`revision.service_storages` and `revision.service_resources` are optional
sequences, empty when omitted.

| Field | Values / omission | Meaning |
| --- | --- | --- |
| `service_storages[].id` | Required semantic ID | Storage lifetime and allocation identity; Instance creation allocates its root. |
| `service_resources[].id` | Required semantic ID | Stable service-resource meaning; separate namespace from Storage IDs. |
| `.storage_id` | Required declared Storage ID | Allocation containing the resource. |
| `.locator` | Required relative service locator | Exact location beneath the allocation, not an arbitrary host path. |
| `.kind` | Required `file` or `directory` | Expected filesystem kind; creation does not materialize the resource. |
| `.read_exposure` | `hidden` or `readable`; default `hidden` | Whether an operator may obtain an allowed read location. It does not control Hook grants. |
| `.user_mutation` | Default `{kind: unavailable}` | Operator write route below. |

| `user_mutation.kind` | Other fields | Meaning |
| --- | --- | --- |
| `unavailable` | None | No direct operator write-location route. |
| `direct` | None | Explicitly permit the direct-write route; not a safety, locking, or hot-reload guarantee. |
| `operation` | Required `action_id` | Named existing Mutate Action owns writes and needs matching authority. Locating the resource does not invoke it. |

Service locators are 1–1024 ASCII bytes, split by `/` into 1–128-byte segments
matching `[A-Za-z0-9._-]+`. Leading periods and uppercase letters are allowed.
Empty, `.` and `..` segments, trailing periods, device names, spaces, non-ASCII,
backslashes, colons, absolute/drive paths, and portable case aliases are rejected.
A file cannot be another resource's ancestor; directories may contain resources.

```yaml
# Properties directly inside revision:
service_storages:
  - id: state
service_resources:
  - id: database
    storage_id: state
    locator: db
    kind: directory
    read_exposure: hidden
    user_mutation: {kind: unavailable}
```

The service or an authorized Hook creates `db`; Pactrun only provides the
Storage root. Keep backups independent of the live allocation.

## Hook service grants and prerequisites

`hook.service_access` and `hook.service_requires` default to empty sequences.

| Shape / field | Values | Meaning |
| --- | --- | --- |
| `service_access[]` | `{reference: <reference>, mode: read or write}` | Explicit granted scope. `write` includes read. |
| `service_requires[]` | `{reference: <reference>, presence: present or absent}` | Prerequisite only. The reference kind must be `resource`; this does not grant access. |
| `reference.view` | `current`, `source`, `target` | Operation view described below. |
| `reference.role` | `active`, `retained` | Current declaration or explicitly retained contract. |
| `reference.kind` | `storage`, `resource` | Whole allocation or a declared resource. |
| `reference.id` | Matching semantic ID | ID in the selected view and type. |

All fields in a present grant/reference/prerequisite are required. Do not repeat
a reference with different modes or add a redundant grant already covered by a
whole-storage grant. A presence condition never substitutes for authority.

- Actions, Capture, and Restore use `current/active`.
- Migration uses `source/active`, `source/retained`, or `target/active`; no `current` view.
- Cleanup can use `current/active` and `current/retained`.
- A source identity must have one consistent role across mappings, grants, and prerequisites.
- A wrong-kind object may be observed as present but still fail qualification.
  Unknown observation satisfies neither present nor absent.

Hooks receive mediated handles/paths for their grants. These grants do not
provide OS isolation or freeze service bytes. Readiness, safe paths, available
allocations, and risk handling are separate checks. Restore's service authority
uses the target Instance's live storage, while its Input view is staged Snapshot data.

## Inbound Migration: `revision.migrations[]`

The target Revision declares an edge from one exact source in the same Package.

| Field | Type / omission | Meaning |
| --- | --- | --- |
| `source_revision_digest` | Required full `sha256:<64 lowercase hex>` | Exact source Revision; the Package is this Pack's lineage. |
| `transitions` | Required sequence; `[]` is valid | Managed Input continuity/disposition below. |
| `requires_source` | Required sequence of `{role, input_id}` | Active/retained source binding prerequisites. |
| `requires_target` | Required sequence of Input IDs | Staged target bindings required before the Hook. |
| `produces_target` | Required sequence of Input IDs | Mandatory Hook-produced target bindings before success. |
| `hook` | Optional Hook | Omit for a supported declarative edge; use for required transformation. |
| `storage_transitions` | Empty when omitted | One writer for each target Storage. |
| `resource_transitions` | Empty when omitted | One writer for each target resource. |

An exact source need not be installed to install the target, but an executable
path requires source semantics and valid relations. Plans reserve nothing. Each
edge commits separately; failure can leave an earlier edge committed.

## Managed Input transitions

`source` is `{role: active or retained, input_id: <id>}`.

| `kind` | Required fields | Effect |
| --- | --- | --- |
| `carry` | `source`, `target_input_id` | Preserve opaque binding continuity into the target. |
| `declassify` | `source`, `target_input_id` | Explicit Secret-to-Normal transition; operator authorization is also required. |
| `keep` | `source` | Preserve retained binding/protection. |
| `discard` | `source` | Explicitly discard that managed Input binding. This is not permission to delete service data. |

Every target Input has at most one writer: a transition, an operator file, a Hook
output, or absence. Conflicts are errors; there is no overwrite precedence.
`requires_target` and `produces_target` must not overlap. Every declared produced
target must be registered before success. Transforming arbitrary Input bytes uses
a Hook, not a declarative copy/merge primitive.

## Storage and resource transitions

Storage `source` is `{role, storage_id}`; resource `source` is `{role, resource_id}`.

| Storage `kind` | Other fields | Meaning |
| --- | --- | --- |
| `create` | `target_storage_id` | Allocate new storage; the identity must not already be associated, including retained state. |
| `reuse` | `source`, `target_storage_id` | Map an active source allocation. |
| `reattach` | `source`, `target_storage_id` | Explicitly map a retained source allocation. |

| Resource `kind` | Other fields | Meaning |
| --- | --- | --- |
| `create` | `target_resource_id`, `presence` | Establish a locator contract; do not automatically create its contents. |
| `reuse` | `source`, `target_resource_id` | Preserve compatible active resource continuity. |
| `reattach` | `source`, `target_resource_id` | Explicit compatible retained-resource continuity. |
| `transform` | `sources` sequence, `targets` sequence of IDs | Explicit one-to-many/many-to-one transformation performed by a Hook. |

Resource-create `presence` is mandatory: `present` requires definitive presence,
`absent` requires definitive absence, and `any` imposes no existence prerequisite.
`any` does not waive safe path/grant checks or turn Unknown into known state.

Every target Storage/resource needs one writer, including absent resources.
One source identity belongs to one transition of its type; group splits/merges
inside one transform. Reuse uses active sources; reattach uses retained sources.
Matching names or paths do not establish compatibility. Unmapped source-only
associations are retained. Consuming a mapping source does not authorize deleting
service bytes. There is no generic service `discard` transition.

## Service transformation completion

For a transform Session, wait for acknowledged Open risk before changing the
service. Produce/register the required target outputs, establish target coherence,
then use `target-ready` and wait for its receipt. Do not independently resolve
risk or send ordinary successful completion for that transform. Normal zero
script/tree termination and the owner's target publication still matter.
Receipt is not durable Run success. Loss between service transformation and
publication may require manual recovery; do not replay or infer rollback.

See [Shell helper semantics](./shell-loader.md) and the
[direct message/state-machine reference](./hook-protocol.md) for the exact exchanges.
