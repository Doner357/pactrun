---
title: Cleanup and Abandonment
---

# Cleanup and Abandonment

Cleanup and stopping management solve different problems.
[Kubernetes finalizers](https://kubernetes.io/docs/concepts/overview/working-with-objects/finalizers/)
illustrate retaining an object until cleanup obligations are satisfied;
[Terraform state removal](https://developer.hashicorp.com/terraform/cli/commands/state/rm)
illustrates forgetting a binding without destroying its remote object. Pactrun
separates these concerns into managed deletion and explicit abandonment.

## Instance deletion

### PR-REQ-0111 - Normal managed deletion

If the active Revision defines Cleanup, normal deletion MUST run Cleanup and
MUST NOT proceed to Pactrun-provided storage-lifetime finalization until Cleanup
has reported success with clear recovery risk and Pactrun has durably published
the Cleanup-completed boundary defined by PR-REQ-0247. With or without a Cleanup
capability, successful normal deletion requires the Pactrun-provided Instance
storage lifetime to have ended before Pactrun removes the Instance state.

**Verification: PR-TEST-0404, PR-TEST-0408.**

### PR-REQ-0112 - Cleanup failure result

Missing Cleanup requirements MUST prevent Hook launch and leave the Instance
unchanged. A terminal Cleanup non-success with clear risk MUST retain a Normal
Instance. Open risk MUST retain the Instance in `ManualRecoveryRequired`.
Pactrun MUST NOT create persistent `Deleting` or `DeletionFailed` states.

**Verification: PR-TEST-0405.**

### PR-REQ-0113 - AbandonManagement intent

`AbandonManagement` MUST be an explicit intent to destroy the Pactrun management
relationship, not a generic `--force` and not authorization to destroy service-
owned state. It MUST skip Package Cleanup and remove Pactrun-owned Instance
management state even when Cleanup is unavailable or the Instance requires
manual recovery. It MUST warn that service-owned resources, files, processes,
or credentials may remain. It MUST remain a managed execution with Run history
that distinguishes explicit abandonment from successful managed cleanup.

**Verification: PR-TEST-0406.**

### PR-REQ-0247 - Cleanup finalization and abandonment

A Cleanup Hook success with clear recovery risk MUST NOT by itself be treated as
a durable Cleanup-completed boundary. Before beginning Pactrun-provided storage-
lifetime finalization, Pactrun MUST durably publish that Cleanup completed and
MUST NOT be replayed. Only after that publication MAY Pactrun perform its own
storage-lifetime finalization, and normal deletion MUST NOT succeed until the
provided storage lifetime has ended.

The Frozen completion rule in
[PR-REQ-0216](../interfaces/hook-protocol.md#pr-req-0216---operation-completion-and-terminal-states)
remains authoritative: loss before accepted completion prevents protocol
success, Pactrun MUST NOT infer replay or compensation, and
`completion_accepted` does not itself imply a Run or Instance commit. Therefore,
loss after a Hook sends Cleanup success but before Pactrun durably publishes the
Cleanup-completed boundary MUST NOT be treated as proof that replay is safe or
that Cleanup durably completed. The evidence-directed recovery and qualified
confirmation rules in [Instance retirement](./retirement.md)
own coordination of that window. This requirement adds no replay, compensation,
manual-recovery or finalization inference.

After the durable Cleanup-completed boundary exists, a crash or failure during
Pactrun-provided storage-lifetime finalization MUST retry or resume only that
Pactrun-owned finalization obligation and MUST NOT replay Cleanup. Until it
finishes, deletion MUST NOT report success or allow ordinary management to
bypass the obligation. The obligation MUST NOT require a public persistent
`Deleting` or `DeletionFailed` Instance state. The [retirement](./retirement.md) and
[persistence](../persistence/persistence-baseline.md) contracts define the durable
obligation and finalization-only retry mechanism.

`AbandonManagement` MUST NOT destructively remove the abandoned service-owned
state during abandonment and MUST NOT authorize later ordinary garbage
collection, unreferenced-storage cleanup, or maintenance to remove it. The
durable non-destruction representation, later discoverability, operator handoff
and explicit discard follow the retirement and Persistence contracts. This
requirement supplies the safety invariant; it does not define an alternate
registry or grant implicit discard authority.

**Verification: PR-TEST-0407, PR-TEST-0408, PR-TEST-0409, PR-TEST-0410, PR-TEST-0411, PR-TEST-0412, PR-TEST-0413, PR-TEST-0414, PR-TEST-0433, PR-TEST-0445.**
