---
title: Hooks, Recovery, and Cleanup
---

# Hooks, Recovery, and Cleanup

**Status: Normative Package contract specification.**

<!-- spec-navigation:start -->
## Reading map (informative)

Understand semantic Hook authority, risk reporting, and Cleanup obligations. Consult the wire specification for encoding and the roadmap for deferred runtimes.

Start with the [specification map](../index.md)
and [shared vocabulary](../glossary.md) if a term is unfamiliar.
Check [implementation status and remaining decisions](../../development/next-milestone.md)
before treating an approved contract as available runtime behavior.
The original status, rules, exceptions, and verification declarations below retain their meaning.
<!-- spec-navigation:end -->

## Hooks and the canonical protocol

### PR-REQ-0167 - Trusted external Hook

A Hook MUST be treated as a trusted external program implementing
Package-specific behavior. It MAY be written as a shell, PowerShell, Python,
Node.js, Go, Rust, or another executable supported by the host.

`RevisionCoreFormatV1` represents native and interpreter launch without host
file associations and excludes launcher `argv[0]` from the Pack-facing
contract. See [Revision Core Format V1](./revision-core-format-v1.md).

**Verification: PR-TEST-0095.**

### PR-REQ-0168 - One language-neutral protocol

Direct Hooks, helper CLIs, wrappers, and SDKs MUST converge on one versioned,
language-neutral canonical Hook Protocol. An adapter MUST NOT define a second
semantic API. The exact Frozen V1 wire and state-machine contract is defined by
[Hook Protocol V1](./hook-protocol-v1.md).

**Verification: PR-TEST-0093.**

### PR-REQ-0169 - Session authority

A `HookSessionSpec` MUST provide only the Instance context, parameters, managed
data authorities, protocol-version-defined facilities, and I/O contract granted
to that execution. Requests outside Pactrun-mediated authority MUST be rejected.

The exact Frozen V1 Session authority representation is defined by
[Hook Protocol V1](./hook-protocol-v1.md).

A future Hook may need explicit authority for persistent `ServiceStorage` or a
ServiceStorage-backed Managed Service Resource. That authority is distinct from
an execution-scoped Workspace and is not present in the Frozen V1 authority
union. Its handles, paths, visibility, access, storage layout, and wire
representation remain future design work and require a future Hook Protocol
version if exposed on the wire.
Revision Core and Hook Protocol versions remain independent, so a future
Revision Core format does not force every Hook to use a new protocol.

The Session MUST NOT let a Hook browse arbitrary managed stores, read another
Instance's managed data, delete arbitrary resources, invoke another Action,
acquire a mutation guard, commit a Snapshot, rewrite a Run, switch the active
Revision, enlarge authority, or redefine the workflow.

**Verification: PR-TEST-0094.**

### PR-REQ-0244 - Persistent storage authority and prerequisites

The protocol-mediated authority model for ServiceStorage-backed resources
MUST support both whole-ServiceStorage and individual-resource semantic scopes.
Each Hook Session MUST receive only the least authority required by its
operation; resource authority MUST NOT silently expand into storage-wide access,
and storage-wide authority MUST NOT grant access to another Instance's storage.
These authority scopes are Pactrun-mediated capabilities, not operating-system
confinement.

An operation MAY depend on a point-in-time resource presence or absence
observation, but that prerequisite MUST remain distinct from Managed Input
readiness and from access authority. When Pactrun is responsible for evaluating
a definitive presence prerequisite, `Unknown` MUST NOT satisfy it. A Package MAY
instead perform a service-specific runtime check in its Hook. The authority
wire shape, observer, admission mechanism, and path mapping are specified by
[Hook V2](./hook-protocol-v2.md) and the
[M6.5 execution contract](../execution/m6-5-service-storage-execution.md).
Workspace MUST remain execution-scoped scratch.

**Verification: PR-TEST-0364, PR-TEST-0368, PR-TEST-0370, PR-TEST-0386.**

Partial Session-construction coverage: individual-resource scope, exact admitted
grant matching, no scope expansion and authority-handle uniqueness. Physical
cross-Instance pin refusal and a live absent prerequisite are covered by the
native qualification helper in PR-TEST-0368. PR-TEST-0370 covers real V2 Action
grants and absence prerequisites with isolated live allocations, and V2
Capture/Restore access to those resources. PR-TEST-0386 exercises actual nested
target directory/file grants in a transforming Migration. Crash and target
publication evidence is separately owned by PR-REQ-0322 and PR-REQ-0326.

### PR-REQ-0170 - Session authority is not host isolation

Canonical Session restrictions MUST be described as Pactrun-mediated authority,
not operating-system confinement. A trusted native Hook may use its OS identity
to access files, networks, processes, or another Pactrun CLI. Doing so outside
the Session is a Package contract violation, not a sandbox escape.

**Verification: Pending automated coverage.**

### PR-REQ-0171 - Future isolation must be enforced

If a future Package declares host-access requirements, effective isolation MUST
combine that request with operator policy and an enforcing backend. When policy
requires isolation and the backend cannot enforce it, execution MUST fail
closed. Pactrun MUST NOT offer unenforced network or filesystem permission
claims. A host-isolation requirement that affects runtime semantics MUST be
identity-bearing RevisionCore content; the selected local backend and operator
policy MUST NOT become Revision identity.

**Verification: Pending automated coverage.**

### PR-REQ-0172 - Terminal and protocol channels

The user terminal channel and Hook Protocol channel MUST remain separate.
Protocol messages MUST NOT be mixed into standard output. `IOContract` MUST
describe channels and transition policy without pretending to be a complete
host-device sandbox.

**Verification: PR-TEST-0098, PR-TEST-0102, PR-TEST-0155.**

### PR-REQ-0280 - Hook Protocol runtime transport discovery

Hook Protocol transport discovery is a stable Pack-facing runtime-integration
contract that selects the dedicated stream required by PR-REQ-0205 without
changing the Frozen `HookProtocolV1` framing, message schema, or state machine.
Before launching a Hook, Pactrun MUST create one owner-private listener and MUST
replace both of these variables in the exact child environment:

```text
PACTRUN_HOOK_PROTOCOL_TRANSPORT
PACTRUN_HOOK_PROTOCOL_ENDPOINT
```

Ambient values MUST NOT be trusted. The closed transport values and endpoint
meanings are:

- `unix-domain-socket`: the endpoint is an absolute pathname for an
  owner-private reliable ordered full-duplex Unix-domain stream socket;
- `windows-named-pipe`: the endpoint is a complete Windows named-pipe pathname
  whose access is restricted to the execution owner.

The variables are injected into the launched Hook process. Descendants receive
them only through ordinary operating-system environment inheritance; Pactrun
does not provision a second endpoint for descendants. The listener accepts one
connection for one Session and MUST NOT be reused. It exists from before process
creation until that connection is accepted or launch/startup termination makes
the Session impossible, and Pactrun MUST close and remove it on every terminal
or cleanup path.

The endpoint is a capability locator, not a Run, Session, Hook, or Package
identity and not an alternative authentication or protocol-version mechanism.
The first bytes in each direction after connection remain the exact Frozen V1
preamble from PR-REQ-0205.

**Verification: PR-TEST-0095, PR-TEST-0098, PR-TEST-0099, PR-TEST-0102,
PR-TEST-0155.**

## Recovery-risk duty

### PR-REQ-0173 - Risk-entry request

Before crossing a boundary after which permanent Hook loss may leave
service-owned state, including a ServiceStorage-backed Managed Service Resource,
incoherent, a Hook MUST request `EnterRecoveryRisk` and wait for Pactrun's
durable acknowledgment.

**Verification: Pending automated coverage.**

### PR-REQ-0174 - Risk-resolution request

After the Package has made service-owned state coherent for ordinary
management, the Hook SHOULD request `ResolveRecoveryRisk`. Resolution does not
mean rollback and does not require the overall operation to succeed.

**Verification: Pending automated coverage.**

### PR-REQ-0175 - No nested risk taxonomy

The initial protocol MUST use a single `Clear | Open` risk state. Package
authors MUST NOT depend on nested risk stacks or low, medium, and high risk
levels.

**Verification: Pending automated coverage.**

## Cleanup

### PR-REQ-0176 - Cleanup capability

Cleanup MUST be an optional Package-defined implementation of Pactrun's
Instance deletion lifecycle, not an Action. When present, normal deletion MUST
run it before Pactrun removes Instance state.

**Verification: PR-TEST-0404, PR-TEST-0426.**

### PR-REQ-0177 - Cleanup requirements and context

Cleanup MAY declare typed requirements over active and retained bindings and
MUST receive a corresponding Cleanup context. Those requirements govern
admission rather than access control. Cleanup MUST NOT inherit the active
Revision's whole ordinary readiness predicate.

**Verification: PR-TEST-0396, PR-TEST-0404, PR-TEST-0418, PR-TEST-0424.**

### PR-REQ-0178 - Missing Cleanup requirements

If a Cleanup requirement is absent, Pactrun MUST reject before Hook launch and
leave the Instance unchanged. The user may supply the requirement, repair state,
retry, or explicitly abandon management.

**Verification: PR-TEST-0418, PR-TEST-0424.**

### PR-REQ-0179 - Cleanup risk protocol

Cleanup MUST use the common recovery-risk protocol. Success with clear risk
permits deletion; non-success with clear risk retains a Normal Instance; open
risk on non-success or reported success retains the Instance in
`ManualRecoveryRequired`.

**Verification: PR-TEST-0405, PR-TEST-0425.**

### PR-REQ-0180 - Retry guidance without magic flags

Package Cleanup SHOULD tolerate retries and already-absent external resources
where practical. Pactrun MUST NOT replace this author responsibility with an
`idempotent=true` or similarly magical correctness flag. This guidance MUST NOT
be interpreted as permission to replay Cleanup after an ambiguous completion;
the Frozen completion rule in PR-REQ-0216 prohibits inferred replay or
compensation.

**Verification: PR-TEST-0407, PR-TEST-0425.**

### PR-REQ-0181 - Abandonment skips Package code

`AbandonManagement` MUST NOT launch the Cleanup Hook or claim external cleanup.
It records explicit operator intent to stop management and remove Pactrun-owned
Instance state while service-owned state may remain. That intent MUST NOT
authorize Pactrun to destroy the abandoned service-owned state during the
operation or later through ordinary garbage collection, unreferenced-storage
cleanup, or another maintenance path. The durable representation of this
non-destruction obligation, later discovery, operator handoff, and explicit
discard remain future persistence and runtime design gates.

**Verification: PR-TEST-0406, PR-TEST-0409, PR-TEST-0417, PR-TEST-0418, PR-TEST-0437.**
