---
title: Revision Core Format V2
---

# Revision Core Format V2

**Status: Frozen normative Package contract specification.**

This independently versioned contract does not amend Frozen V1 behavior.
The [format review](../../development/design-notes/m6-5-format-activation-review.md)
records the approved design, conformance and pre-activation full-CI evidence.
Format Freeze does not itself certify milestone or platform completion.

<!-- spec-navigation:start -->
## Reading map (informative)

Read the [S0 baseline](../../development/design-notes/m6-5-servicestorage-baseline.md),
[V1 schema](./revision-core-format-v1.md), [YAML V2](./pack-source-yaml-v2.md),
[execution proposal](../execution/m6-5-service-storage-execution.md), and
[Hook V2](./hook-protocol-v2.md). Closed ownership semantics remain in
[identity and state](../foundations/identity-and-state.md).
<!-- spec-navigation:end -->

### PR-REQ-0317 - Core V2 closed schema

V2 is the following exact delta over the closed V1 schema. A referenced V1 type
retains all its fields and validation unless explicitly replaced here. Unknown
fields, duplicate decoded keys and generic null values are rejected. Normalized
arrays below are required even when empty; optional capabilities are omitted.

```text
RevisionCoreV2 = RevisionCoreV1 with:
  format_version: 2
  service_storages: ServiceStorageV2[]
  service_resources: ServiceResourceV2[]
  every HookV1 position replaced by HookDeclarationV2
  every MigrationV1 replaced by MigrationV2

ServiceStorageV2 = { id: ServiceStorageIdentity }
ServiceResourceV2 = {
  id: ServiceResourceIdentity,
  storage_id: ServiceStorageIdentity,
  locator: ServiceLocatorV2,
  kind: file | directory,
  read_exposure: hidden | readable,
  user_mutation: UserMutationV2
}
UserMutationV2 =
  { kind: unavailable }
  | { kind: direct }
  | { kind: operation, action_id: ActionIdentity }

HookDeclarationV2 = HookV1 with:
  service_access: ServiceAccessV2[]
  service_requires: ServicePrerequisiteV2[]
ServiceAccessV2 = { reference: ServiceReferenceV2, mode: read | write }
ServiceReferenceV2 = {
  view: current | source | target,
  role: active | retained,
  kind: storage | resource,
  id: ServiceStorageIdentity-or-ServiceResourceIdentity
}
ServicePrerequisiteV2 = {
  reference: ServiceReferenceV2 with kind exactly resource,
  presence: present | absent
}

MigrationV2 = MigrationV1 with:
  hook?: HookDeclarationV2
  storage_transitions: StorageTransitionV2[]
  resource_transitions: ResourceTransitionV2[]
StorageSourceV2 = { role: active | retained, storage_id: ServiceStorageIdentity }
ResourceSourceV2 = { role: active | retained, resource_id: ServiceResourceIdentity }
StorageTransitionV2 =
  { kind: create, target_storage_id: ServiceStorageIdentity }
  | { kind: reuse, source: StorageSourceV2, target_storage_id: ServiceStorageIdentity }
  | { kind: reattach, source: StorageSourceV2, target_storage_id: ServiceStorageIdentity }
ResourceTransitionV2 =
  { kind: create, target_resource_id: ServiceResourceIdentity, presence: any | present | absent }
  | { kind: reuse, source: ResourceSourceV2, target_resource_id: ServiceResourceIdentity }
  | { kind: reattach, source: ResourceSourceV2, target_resource_id: ServiceResourceIdentity }
  | { kind: transform, sources: ResourceSourceV2[], targets: ServiceResourceIdentity[] }
```

Storage and Resource identities are separate types using V1 semantic-identifier
grammar and length. They are Package-lineage scoped and non-reusable in meaning;
equal spellings across these two types do not equate identities. Every resource
names a declared storage. A file means a regular file, not a device, socket or
symbolic link. A directory includes its descendant service contents for authority
purposes, not for identity calculation or automatic scanning.

Live Storage contents have no Managed Input payload-size limit or Pactrun quota
in this proposal; ordinary host filesystem capacity applies. Existing Input,
Snapshot capture/restore and artifact capability limits remain independent.

`ServiceLocatorV2` is 1..1024 ASCII bytes of slash-separated 1..128-byte segments
matching `[A-Za-z0-9._-]+`. Reject empty, `.` and `..` segments, trailing periods,
Windows device-name stems (V1's case-insensitive reserved-name rule), and any
absolute/drive/backslash/colon syntax. Leading periods and uppercase letters
are allowed and preserved. Spaces and non-ASCII locator characters are outside
this initial portable contract; native storage-root paths remain separate.
Do not lowercase or Unicode-normalize canonical data. Within one storage,
active declarations must not have ASCII-case-fold-equal locators. A declared
file cannot be a locator ancestor of another resource; a directory may be.
Nested directory/file resources deliberately have overlapping authority scopes.

`user_mutation.kind=operation` names an existing Mutate Action whose write
authority covers this current resource. It does not auto-invoke that Action.
Every Action/Capture Hook with write service authority must have Mutate access;
Migration/Restore are already Mutate. This classification does not substitute
for the runtime Clear/Open risk handshake or lock out an external service.

Ordinary Action/Capture/Restore references use `current/active`. For Restore,
current service authority is the target Instance's existing live storage, while
Input authority is still the staged Snapshot binding view. Migration references
use `source/active`, `source/retained`, or `target/active`; `current` is forbidden
there. Cleanup can encode `current/active` and `current/retained`, but Cleanup
execution remains unsupported until M7. Source/active authority resolves against
the exact source Core; source/retained authority resolves the explicitly named
retained association and its last declaration. A grant need not consume a
mapping source: an explicitly granted source Storage can contain a resource
being transformed into a new target Storage. Target/current references resolve
to target/current Core declarations. Exact retained availability is a runtime
predicate, not an install-time claim.

Write includes read. Duplicate references with different modes are rejected,
not silently unioned. A whole-storage grant that subsumes another grant of the
same view/role is redundant and rejected; a directory resource grant may
deliberately include descendants. Authority is Pactrun-mediated, not OS
confinement. A prerequisite never grants authority. Contradictory/duplicate
presence prerequisites for one reference are rejected. A definitive Unknown
observation satisfies neither present nor absent.

Only Hook versions 1 and 2 are supported by the proposed initial runtime. A
Hook with nonempty service_access or service_requires, or on a transform edge,
must select 2. Version 1 with empty new arrays receives an unchanged V1 Session.
Other positive versions remain intrinsically representable as in V1, but fail
runtime qualification before acceptance; Core version never selects Hook version.

A minimal normalized Core example and its empty runtime-content sibling are:

```json
{"actions":[],"format_version":2,"inputs":[],"migrations":[],"service_resources":[],"service_storages":[{"id":"state"}]}
```

```json
{"files":[]}
```

These illustrate shapes, not a Frozen vector or published digest. Adding
allocation_id, native_path or service bytes to this Core is an unknown-field
error. Repeating state is a duplicate identity; changing format_version to 1
while retaining service fields is invalid V1, not an automatic downgrade.

**Verification: PR-TEST-0331, PR-TEST-0333, PR-TEST-0334, PR-TEST-0337, PR-TEST-0367, PR-TEST-0384, PR-TEST-0386.**

S1 coverage proves the closed format, typed intrinsic rules and authoring
projection. Live authority/prerequisite/Instance behavior still requires S2-S7
runtime evidence; these format tests do not certify it.

### PR-REQ-0318 - V2 normalization and identity

Apply the V1 strict JSON, exact numeric, Unicode-scalar, finite binary64 and
RFC 8785 rules unchanged. Normalization sorts existing V1 sets by their existing
keys and preserves ordered args. New storage/resource declarations sort by id.
Service references sort by `(view,role,kind,id)` using ASCII token bytes;
service_access and service_requires use that reference key. Reject duplicate
keys before sorting. Transform sources sort by `(role,resource_id)` and targets
by identity; both lists must be nonempty and duplicate-free.

Storage/resource transition arrays sort by their sole target, or the first
sorted transform target. Target uniqueness below makes that a total unique
key; one transform group owns all its targets. Different explicit mappings
remain distinct identities even when a Pack believes them equivalent.

Use RuntimeContentClosureIdentityV1 unchanged as the sibling component. Let
core_jcs and content_jcs be their normalized JCS UTF-8 bytes. The exact frame is:

```text
ASCII("pactrun.revision-content-digest\0")
U32BE(2)
ASCII("revision-core\0")
U64BE(len(core_jcs))
core_jcs
ASCII("runtime-content-closure\0")
U64BE(len(content_jcs))
content_jcs
```

Hash with SHA-256, render `sha256:` plus 64 lowercase hexadecimal characters.
Lengths count bytes. Core format_version and frame version must both be 2;
mixed version pairs fail. Native paths, allocation IDs, resource observations,
service bytes, exposure history and persistence bookkeeping never enter identity.
V1/V2 decoders dispatch explicitly; no decode-and-reencode upgrade of stored V1.
PackageId plus the resulting digest remains RevisionIdentity; no new public ID
envelope is required. Node parity vectors in S1 must consume independently
checked-in inputs, not Rust-produced expected bytes. S0 defines no golden digest.

**Verification: PR-TEST-0331, PR-TEST-0332, PR-TEST-0333, PR-TEST-0338, PR-TEST-0339, PR-TEST-0379, PR-TEST-0383.**

### PR-REQ-0319 - Target-owned resource mapping

Each target storage and target resource has exactly one transition writer in
each inbound edge, even when the live resource is absent. Omission is not
compatibility. Each source identity may occur in only one transition of its
type; group one-to-many/many-to-one changes in a single transform. Unmapped
source-only declarations default to retention, not physical deletion. No service `discard`
transition is introduced in M6.5; existing Input discard is unchanged.

A source named by reuse, reattach or transform is consumed by that mapping.
At target commit its old association is removed and its target associations
are published atomically. If its identity is also a target, the target replaces
that association. This applies independently to Storage and Resource mappings.
Do not retain a second live alias solely because a mapped identity was renamed.
Unmapped sources retain their associations and exact last contracts. Before a
failed/uncommitted edge, all original associations remain unchanged. Association
consumption never authorizes deleting service bytes or allocation custody.

One typed source identity must have one consistent active/retained role across
all of that edge's mappings, grants and prerequisites. Reject contradictory roles
even inside one transform group; storage and resource identity types are checked
independently, and target-view references are not source-role assertions.

`create` requires no existing association for that identity, including retained
ones. Storage create reserves a new directory. Resource create establishes a
new locator contract, not a live object. Its mandatory presence predicate is
checked before grant/publication: present/absent require that exact definitive
observation; any asserts no existence condition and never converts Unknown to
presence or absence. This explicitly permits naming already service-owned files
inside a reused allocation without moving them or adopting an external directory.
It asserts no content validity or coherence; the Pack owns that interpretation.
Only parents necessary for the Storage root are created by Pactrun; service
directory trees belong to service/Hook behavior. A target create predicate and
Hook prerequisite that contradict at the same observation boundary are invalid.

At Admission, a target locator equal under portable case comparison to another
published active/retained resource in the same allocation is a location claim
conflict unless that resource is an explicit source of the target's writer.
Thus create cannot disguise a rename as a new association over an already named
object; it may name previously unassociated contents. This rejects ambiguous
claims without inferring identity or compatibility from a locator. Directory
containment alone is not equality and retains the explicit subtree semantics.

`reuse` requires an active source; `reattach` requires a retained source. Both
are explicit Pack assertions of representation compatibility. Storage reuse or
reattach transfers the same allocation to the target role. Distinct active
Storage declarations may not share an allocation. Resource reuse/reattach
requires equal kind, exact case-sensitive locator and the same resolved physical
allocation. Identity spelling changes are permitted only through that explicit
source-to-target mapping. No live existence or coherence is inferred.

`transform` requires a Hook selecting V2 and the target-publication protocol.
It covers locator/kind/allocation/representation changes, splits and merges;
Pactrun does not implement file moves, copying, parsing, merging or deletion.
The Hook coordinates transformations of nested/overlapping service scopes.
Every source needs declared read authority and every target write authority;
source mutation additionally needs explicit write authority. Unmapped source-only
association records remain retained even if a later service observation is
Absent. A target identity already present in the
Instance must occur as a source of its own writer group: no silent overwrite
of an unrelated active/retained association. Identity changes never authorize
reuse of an old spelling for an unrelated semantic purpose.

For example, explicit config -> settings reuse over config.json leaves only
settings associated after commit. Subsequent settings -> settings reuse has no
ghost config alias to conflict with. Dropping config with no mapping instead
retains config and requires a later explicit reattach. Historical Revision/Run
identity is separate from a live association; no alias-lineage registry is added.

Intrinsic validation checks syntax, target coverage, unique writers, local
references and Hook requirements. Exact-source relational validation checks
roles and declared compatibility, with unavailable source semantics remaining
NotEvaluated under existing install policy. Admission additionally resolves
retained contracts/allocations and presence observations. It cannot infer
compatibility by reading service bytes. V1 sources have no service declarations;
V1-to-V2 transitions create their initial storages/resources explicitly.

**Verification: PR-TEST-0334, PR-TEST-0335, PR-TEST-0371, PR-TEST-0372, PR-TEST-0373, PR-TEST-0374, PR-TEST-0382, PR-TEST-0383, PR-TEST-0386, PR-TEST-0388.**

S1 covers intrinsic target writers, source roles and exact-source format
relations. Actual retained availability, allocation equality and atomic
consumption/publication remain runtime verification responsibilities.
