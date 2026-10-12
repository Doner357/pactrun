---
title: Operation-scoped Revision Format
---

# Operation-scoped Revision Format

## Applicability

This contract defines Pack source `1.0-alpha.3` and Revision canonical
`1.0-alpha.2`. The [support table](../storage/format-domains.md) lists their exact
readers alongside the legacy contracts. Existing supported formats retain their
meaning; this page does not redefine their bytes or defaults.

### PR-REQ-0379 - Operation contract version selection

The following exact combinations MUST govern the operation-scoped contract:

| Domain | Legacy contract | Operation-scoped contract |
| --- | --- | --- |
| Pack source | `1.0-alpha.2` | `1.0-alpha.3` |
| Revision canonical | `1.0-alpha.1` | `1.0-alpha.2` |
| Hook Protocol | `1.0-alpha.1` | `1.0-alpha.2` |
| CLI machine interface | `1.0-alpha.3` | `1.0-alpha.4` |

Source `1.0-alpha.2` MUST continue projecting Revision `1.0-alpha.1`.
Source `1.0-alpha.3` MUST project Revision `1.0-alpha.2`. Authors MUST NOT
select the Core version independently in Source. Every Hook in the new Core
MUST select Hook Protocol `1.0-alpha.2`; a new Core with an old Hook, or an old
Core with a new Hook, MUST be rejected before execution. A Hook-free Revision
does not require a fabricated Hook declaration.

The [old Source](./source-format.md) and [old Core](./revision-format.md)
define unchanged fields, runtime content, service declarations, numbers,
identity grammars, strict decoding and validation. The differences below are
closed schema replacements, not permission for arbitrary extension fields.
New Source Hook defaults select `1.0-alpha.2`; old Source defaults do not change.

Pack distribution, Snapshot integrity and Snapshot bundle versions are not
advanced by these operation formats. Supporting the new Core does not change the Pack
envelope. Persistence alpha.5 is defined separately by
[operation artifact storage](../persistence/operation-artifacts.md) and uses the
supported atomic Store upgrade path without rewriting immutable content.

**Verification: PR-TEST-0723.**

### PR-REQ-0380 - Operation Input and common capability projection

In the new Core, global `InputDeclaration` has exactly `id` and `protection`.
Its old `required` field MUST be rejected, including `required: false`.
Descriptive metadata remains outside Core identity under its existing contract.

The following Core replacements MUST apply; all other fields remain as defined
by the old Core and service contracts:

| Record | New fields | Removed fields |
| --- | --- | --- |
| Action | `inputs: ActiveInputUse` | None |
| Capture | `inputs: ActiveInputUse`, `outputs: ManagedOutput[]` | None |
| Restore | `inputs: ActiveInputUse`, `outputs: ManagedOutput[]` | None |
| Migration | `source_inputs: BindingInputUse`, `target_inputs: ActiveInputUse`, `parameters: Parameter[]`, `outputs: ManagedOutput[]` | `requires_source`, `requires_target` |
| Cleanup | `inputs: BindingInputUse`, `parameters: Parameter[]`, `outputs: ManagedOutput[]` | `requires` |

```text
ActiveInputUse
|- required: InputIdentity[]
`- optional: InputIdentity[]

BindingInputUse
|- required: InputBindingRef[]
`- optional: InputBindingRef[]

InputBindingRef
|- role: active | retained
`- input_id: InputIdentity
```

Every new Core field and both arrays of each Input-use object MUST be present,
including empty arrays. `null` is invalid. In new Source, an omitted Input-use
object or omitted array defaults to empty. New parameter and output arrays also
default to empty in Source and MUST be explicit in Core. Explicit `null`, old
removed fields and `compatible_revisions` MUST be rejected.
There is no implicit grant to all Inputs. Existing Hook-free Migration support,
transitions and `produces_target` remain available.

Active references MUST resolve in their operation's active context. Binding
references MUST resolve with the declared role in the selected context.
Unknown roles, duplicate entries, and intersection between required and optional
MUST be rejected, rather than deduplicated. A reference whose source semantics
are unavailable remains not evaluated, never implicitly valid or executable.
The same Input identity in different legitimate source/target contexts is not
itself a duplicate. Within one source or Cleanup context, conflicting role
references to the same identity MUST be rejected.

Each active list is a semantic set sorted by Input identity. Each binding list
is sorted by `(role, input_id)` using the existing canonical ordering.
Parameters and ordinary outputs use the existing Action types, normalization,
duplicate rejection and ordering. Their identities are local to the owning
operation. A Migration edge without a Hook MUST have empty parameter and ordinary
output arrays; declarative transitions do not execute a fabricated Action.

The new Core marker and these fields MUST participate in canonical bytes and
identity. The old JCS, SHA-256, frame labels, length framing and runtime closure
rules remain unchanged. The explicit Core marker distinguishes the new format;
implementations MUST NOT rewrite or rehash an old Revision into the new format.

Target `required` and `optional` reads MUST be disjoint from Migration
`produces_target`: a produced Input is a completion obligation and output
authority, not a pre-Hook read grant. A transition may read data internally
without granting the Hook access to it. Source requirements still apply to
Hook-free edges; requirements do not manufacture a Hook Session.

**Verification: PR-TEST-0701, PR-TEST-0702, PR-TEST-0703, PR-TEST-0704, PR-TEST-0705, PR-TEST-0706, PR-TEST-0707.**

### PR-REQ-0389 - Existing object and contract continuity

Implementing these versions MUST preserve currently supported old Pack
installation and execution, old Input visibility, global required admission,
bound-required removal restrictions, exact-revision Restore, and historical
Run interpretation. New software MUST select policy from the exact pinned
Revision and protocol contracts, not from the executable's default version.

Old immutable Core bytes, digests and Snapshot bodies MUST remain unchanged.
Existing Instances, payload identities, unresolved Runs, pins, ownership and
recovery obligations MUST survive. An old Run MUST NOT be rejected as corrupt
merely because it does not satisfy the new authority model. Reopening a Store
MUST NOT replay Hooks, recreate Instances or infer a revision transition.

An explicit Migration to a new Revision changes subsequent operation policy;
it does not reinterpret the source Revision or historical Runs. The destination
Revision owns the inbound edge contract, while source references retain their
exact source semantics. Old-to-new and new-to-old transitions still require
independently valid declared edges, not automatic conversion.

Executable format support MUST NOT be advertised until its codecs, execution,
refusal and compatibility evidence are qualified. Documentation reservation is
not support. [CLI output selection](../interfaces/operation-machine-output.md)
MUST preserve the old machine contract without inventing readiness for a new
Revision.

**Verification: PR-TEST-0729, PR-TEST-0737.**

## Related contracts

- [Operation Input requirements and authority](../operations/input-authority.md)
- [Operation-scoped Restore](../snapshots/restore-compatibility.md)
- [Operation-scoped Hook Protocol](../interfaces/operation-hook-protocol.md)
- [Operation-scoped machine output](../interfaces/operation-machine-output.md)
