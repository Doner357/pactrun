---
title: Hooks, Recovery, and Cleanup
---

# Hooks, Recovery, and Cleanup

**Status: Normative Package contract specification.**

## Hooks and the canonical protocol

### PR-REQ-0167 - Trusted external Hook

A Hook MUST be treated as a trusted external program implementing
Package-specific behavior. It MAY be written as a shell, PowerShell, Python,
Node.js, Go, Rust, or another executable supported by the host.

**Verification: Pending automated coverage.**

### PR-REQ-0168 - One language-neutral protocol

Direct Hooks, helper CLIs, wrappers, and SDKs MUST converge on one versioned,
language-neutral canonical Hook Protocol. An adapter MUST NOT define a second
semantic API.

**Verification: Pending automated coverage.**

### PR-REQ-0169 - Session authority

A `HookSessionSpec` MUST provide only the Instance context, parameters, managed
data authorities, protocol features, and I/O contract granted to that
execution. Requests outside Pactrun-mediated authority MUST be rejected.

The Session MUST NOT let a Hook browse arbitrary managed stores, read another
Instance's managed data, delete arbitrary resources, invoke another Action,
acquire a mutation guard, commit a Snapshot, rewrite a Run, switch the active
Revision, enlarge authority, or redefine the workflow.

**Verification: Pending automated coverage.**

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

**Verification: Pending automated coverage.**

## Recovery-risk duty

### PR-REQ-0173 - Risk-entry request

Before crossing a boundary after which permanent Hook loss may leave
service-owned state incoherent, a Hook MUST request `EnterRecoveryRisk` and wait
for Pactrun's durable acknowledgment.

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

**Verification: Pending automated coverage.**

### PR-REQ-0177 - Cleanup requirements and context

Cleanup MAY declare typed requirements over active and retained bindings and
MUST receive a corresponding Cleanup context. Those requirements govern
admission rather than access control. Cleanup MUST NOT inherit the active
Revision's whole ordinary readiness predicate.

**Verification: Pending automated coverage.**

### PR-REQ-0178 - Missing Cleanup requirements

If a Cleanup requirement is absent, Pactrun MUST reject before Hook launch and
leave the Instance unchanged. The user may supply the requirement, repair state,
retry, or explicitly abandon management.

**Verification: Pending automated coverage.**

### PR-REQ-0179 - Cleanup risk protocol

Cleanup MUST use the common recovery-risk protocol. Success with clear risk
permits deletion; non-success with clear risk retains a Normal Instance; open
risk on non-success or reported success retains the Instance in
`ManualRecoveryRequired`.

**Verification: Pending automated coverage.**

### PR-REQ-0180 - Retry guidance without magic flags

Package Cleanup SHOULD tolerate retries and already-absent external resources
where practical. Pactrun MUST NOT replace this author responsibility with an
`idempotent=true` or similarly magical correctness flag.

**Verification: Pending automated coverage.**

### PR-REQ-0181 - Abandonment skips Package code

`AbandonManagement` MUST NOT launch the Cleanup Hook or claim external cleanup.
It records explicit operator intent to stop management and remove Pactrun-owned
Instance state while external resources may remain.

**Verification: Pending automated coverage.**
