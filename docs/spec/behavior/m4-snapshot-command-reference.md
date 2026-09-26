---
title: M4 Snapshot Commands
---

# M4 Snapshot Commands

The create-and-restore addition below was approved on 2026-09-17. Its delivery
is tracked separately from the completed M4 command surfaces.

**Status: Approved normative CLI, implemented and integrated into develop.
Verification and milestone integration are recorded in the M4 execution record.**

<!-- spec-navigation:start -->
## Reading map (informative)

Look up the implemented M4 Snapshot command contract. This page does not announce Snapshot deletion or later lifecycle commands.

Start with the [specification map](../index.md)
and [shared vocabulary](../glossary.md) if a term is unfamiliar.
Check [implementation status and remaining decisions](../../development/next-milestone.md)
before treating an approved contract as available runtime behavior.
The original status, rules, exceptions, and verification declarations below retain their meaning.
<!-- spec-navigation:end -->

### PR-REQ-0301 - M4 human command spelling

The M4 human CLI MUST implement exactly these additional forms:

```text
pactrun snapshot capture <instance> [execution-options]
pactrun snapshot restore <instance> <snapshot-id> [execution-options]
pactrun snapshot list [--instance <instance>]
pactrun snapshot show <snapshot-id>
pactrun snapshot verify <snapshot-id>
pactrun snapshot import <bundle-path>
pactrun snapshot export <snapshot-id> --output <base-path> --authorize-sensitive-export
```

The later [specialized export naming rule](./command-and-output-reference.md#pr-req-0357---specialized-envelope-export-filenames)
requires export to append `.snapshot` unconditionally to the supplied base path.
For example, `--output backup` writes `backup.snapshot`, while
`--output backup.snapshot` writes `backup.snapshot.snapshot`. Import continues to
use the exact supplied path. The success report includes the final output path.

Among Snapshot subcommands, only capture/restore accept execution-options:

```text
[--param <id>=<text>]...
[--param-file <id>=<host-path>]...
[--param-stdin <id>]
[--plan]
[--authorize-recovery-override]
[--startup-timeout-ms <milliseconds>]
[--execution-timeout-ms <milliseconds>]
[--termination-grace-ms <milliseconds>]
```

Instance operands keep M2 exact InstanceName semantics. SnapshotId is exactly
32 lowercase hexadecimal characters; no prefixes or fuzzy lookup. Typed
parameters, Protected file/stdin sources, redaction, deadline range checks,
and interactive-Hook stdin exclusion retain M3 semantics. Bundle stdin/stdout
is not enabled by parameter stdin. Omitted startup/execution timeouts are
unlimited; omitted termination grace is 5000ms; zero means immediate expiry or
no grace. Existing invoke --action-timeout-ms is unchanged.

Capture access MUST come from the exact authored capability; no caller access
option exists. Restore invocation expresses replacement intent, with warnings
but no generic yes/force confirmation flow. A recovery override bypasses only
the trust guard for that execution and cannot waive readiness, exact identity,
capacity, transition legality, or conflicts. --plan remains side-effect-free:
no Run, reservation, lease, pin, schema upgrade, or implicit reconciliation.

Success is exit 0, syntax failure 2, operation failure/cancellation 1.
Capture/Restore return 0 only after durable Succeeded publication. Import and
export accept filesystem paths only; '-' and force/overwrite/resume are not
supported. The development-era storage upgrade command is retired by E; an
unsupported persistence store is refused without an upgrade or data deletion.
Delete, machine-output envelopes, raw manifest, payload preview, and stable
public Rust APIs are outside M4.

**Verification: PR-TEST-0268, PR-TEST-0269, PR-TEST-0270, PR-TEST-0271, PR-TEST-0272, PR-TEST-0273, PR-TEST-0274.**

S7 implements the Snapshot commands through the existing typed Compiler,
Admission, owner continuation and dedicated result publishers. Both V1 and V2
have fresh-process import/verify/export/Restore journeys with real service bytes.

### PR-REQ-0302 - Snapshot inspection, verification, and diagnostics

list/show MUST be structural-only and read-only: no full implicit payload
verification, lease, migration, repair, or reconciliation. list --instance MUST
resolve a live Instance name to exact identity and filter Snapshot provenance;
it does not imply ownership. Plans and diagnostics MUST use typed safe
projections, not raw persisted text, ZIP errors, paths containing payload data,
Secret bytes, Secret-derived digests or lengths, or revealing parameter values.

Inspection MUST distinguish publication-time validation provenance, current
verification actually performed, local Restore capability, and eligibility
for a particular target. A stored Snapshot may be valid but not executable by
this build. An unspecified target MUST NOT be reported as definitely eligible.

verify MUST explicitly check the selected stored manifest and complete payload
closure with its original V1/V2 verifier. It creates no Run and does not repair
or reconcile. If producer semantics are unavailable but intrinsic/content
verification completes, return 0 and explicitly report relational validation
not_evaluated. If capacity/resource limits prevent completion, return 1 and
report incomplete verification; do not call the Snapshot corrupt or valid.

Error ownership MUST remain layered: intrinsic format violations belong to
their version-specific verifier; bundle profile violations belong to the
bundle adapter; operation capability refusals are not integrity corruption;
host I/O/resource errors are separate. Keep Frozen Error Taxonomy V1's existing
owners/codes/categories intact and use fixed safe operation diagnostics.

**Verification: PR-TEST-0209, PR-TEST-0211, PR-TEST-0214, PR-TEST-0215, PR-TEST-0218, PR-TEST-0268, PR-TEST-0269, PR-TEST-0270, PR-TEST-0271, PR-TEST-0272, PR-TEST-0274.**

S3 covers application-service boundaries; S7 adds public dispatch and structural
managed Run inspection. Snapshot show distinguishes publication-time intrinsic
and content verification from current verification. Historical producer-relative
verification was not persisted by V5 and is explicitly shown as not_recorded,
not inferred from the producer's current availability. Restore plan eligibility
is advisory Compiler qualification, not Admission or a writable reservation.

### PR-REQ-0346 - Create an Instance and Restore its Snapshot

The CLI MUST accept `instance create <name> --revision <reference>
--restore-from <snapshot-id>` with the existing Restore execution options except
`--plan`. Revision is required and MUST exactly match the Snapshot producer.
Initial `--input-file` and `--input-stdin` options MUST NOT be combined with this
form. Restore-only options without `--restore-from` MUST be rejected. Existing
parameter typing, source protection, redaction, timeouts, cancellation and
interactive-stdin restrictions apply without new defaults.

Before Create, read-only preflight MUST check the Snapshot, exact installed
Revision, Restore declaration and supplied parameters. It creates no Instance,
Run, pin or reservation and cannot substitute for formal Restore Admission.
Parameter sources are acquired once; stdin MUST NOT be reread after Create.
Restore MUST target the exact InstanceId returned by Create, never a later
name resolution. Snapshot staged bindings determine Restore readiness.

Create failure MUST NOT start Restore. After Create succeeds, Restore refusal,
failure or cancellation MUST preserve the created Instance and report partial
completion, its identity, current state when available, and any accepted RunId.
Only durable Restore success returns 0; syntax failure returns 2 and operation
failure or partial completion returns 1. Diagnostic output MUST remain safe.
The operations are not one transaction: process loss between them can leave an
Instance without a Restore Run. There is no automatic deletion, resume,
compensation, installation, import, Migration or compatibility relaxation.

**Verification: PR-TEST-0476, PR-TEST-0477, PR-TEST-0478.**
