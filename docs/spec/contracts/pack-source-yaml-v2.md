---
title: Pack Source YAML V2
---

# Pack Source YAML V2

**Status: Candidate normative Package authoring contract; versioned, non-Frozen, and not a public compatibility promise.**

<!-- spec-navigation:start -->
## Reading map (informative)

This is the explicitly selected V2 authoring projection. Read
[YAML V1](./pack-source-yaml-v1.md), [Core V2](./revision-core-format-v2.md),
and the [S0 baseline](../../development/design-notes/m6-5-servicestorage-baseline.md).
<!-- spec-navigation:end -->

### PR-REQ-0320 - YAML V2 projection and examples

`pactrun.yaml` uses the exact V1 top-level schema with
`source_format: 2`. `package_id`, runtime source acquisition and portable
metadata retain V1 semantics. `revision` is Core V2 without format_version;
the projector inserts 2. Every V1 scalar-style, raw numeric-token, duplicate-key,
tag/alias/anchor/merge-key, null, unknown-field and source-root rule remains.
There is no include/template language, arbitrary host-path declaration, inferred
Package lineage, automatic Core-version selection, or Recipe support here.

All V1 authoring defaults remain. Additionally:

| Position | Default |
| --- | --- |
| revision.service_storages, revision.service_resources | empty array |
| every hook.service_access, hook.service_requires | empty array |
| migration.storage_transitions, migration.resource_transitions | empty array |
| resource.read_exposure | hidden |
| resource.user_mutation | `{ kind: unavailable }` |

Storage/resource identity, locator, kind, storage_id, mapping kind/operands,
authority mode/reference and prerequisite presence have no defaults. Missing
resource-create presence is also invalid; use any/present/absent explicitly.
Missing transition arrays still fail target-coverage validation when targets exist.
No new portable_metadata presentation/provenance target variants are added;
existing V1 variants address corresponding V2 capabilities only. Services and
resources use their semantic IDs for display until a separately approved
presentation extension exists. Metadata is not an operational state channel.

### Positive authoring example (not a golden identity vector)

```yaml
source_format: 2
package_id: 00000000000000000000000000000065
revision:
  service_storages:
    - id: state
  service_resources:
    - id: config
      storage_id: state
      locator: config.json
      kind: file
      read_exposure: readable
      user_mutation: { kind: direct }
    - id: database
      storage_id: state
      locator: db
      kind: directory
  actions:
    - id: initialize
      access: mutate
      parameters: []
      outputs: []
      hook:
        protocol_version: 2
        launch: { kind: direct, executable: initialize }
        args: []
        io: { terminal: none }
        service_access:
          - reference: { view: current, role: active, kind: resource, id: database }
            mode: write
runtime_content:
  files:
    - { id: initialize, source: initialize.exe, path: bin/initialize.exe, executable: true }
```

Installing requires the example's actual runtime file; this document does not
supply executable bytes. Instance creation provides the state Storage but not
config.json or db. The operator can locate config for direct creation/editing;
database is not user-exposed. The initialize Hook owns creating db and any risk
handshake required by its actual service mutation.

For an inbound edge whose exact source declares these same roles, the explicit
unchanged-state portion is:

```yaml
storage_transitions:
  - kind: reuse
    source: { role: active, storage_id: state }
    target_storage_id: state
resource_transitions:
  - kind: reuse
    source: { role: active, resource_id: config }
    target_resource_id: config
  - kind: reuse
    source: { role: active, resource_id: database }
    target_resource_id: database
```

This fragment is placed on a Migration alongside an actual full
source_revision_digest and the existing Input transition fields. A retained
database requires `kind: reattach` with `role: retained`, not reuse or create.
A split uses one transform with one source and multiple targets; a merge uses
multiple sources and one target. Neither example silently copies bytes.

### Negative projection/validation cases

| Input | Required refusal |
| --- | --- |
| source_format 1 plus service_storages | V1 unknown field; never infer V2 |
| revision.format_version supplied | source-field rejection |
| file resource without storage_id/kind/locator | missing required field |
| locator `../outside`, `C:/data`, `db\\state`, `CON`, or `db/name ` | invalid portable locator |
| locators `DB/state` and `db/state` in one storage | portable alias collision |
| Hook 1 with service_access | unsupported authority/version combination |
| presence prerequisite but no read/write grant | valid prerequisite only; grants no authority |
| omitted mappings for a target resource | missing target writer |
| two transforms naming the same target | writer conflict |
| reattach from active, or reuse from retained | role mismatch |
| same locator with incompatible representation and no explicit transition | no inferred compatibility |
| user_mutation operation names missing/non-Mutate Action | invalid mutation route |

The final two mapping predicates may require exact source semantics or Instance
association state; they are not falsely decided by YAML parsing alone.
Existing source_format 1, Core V1 identities and V1-only Hooks are untouched.
Switching source_format to 2 deliberately creates a new identity-bearing Core;
it is not an in-place upgrade of an installed Revision.

**Verification: PR-TEST-0336, PR-TEST-0337, PR-TEST-0379, PR-TEST-0383, PR-TEST-0384.**
