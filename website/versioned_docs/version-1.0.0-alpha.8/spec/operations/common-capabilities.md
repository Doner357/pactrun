---
title: Common Operation Capabilities
---

# Common Operation Capabilities

This contract applies to the specified operation-scoped formats. Existing
formats retain their behavior. The format support table remains authoritative
until implementation and verification qualify the new formats.

### PR-REQ-0390 - Common parameters and specialized lifecycle

Action, Capture, Restore, Migration Hooks and Cleanup MUST support the same
parameter types, sensitive handling, defaults and validation. Parameters belong
to one invocation, not persistent Input Bindings. Operation-specific authority,
completion obligations, exclusivity and lifecycle constraints remain in force.
Supporting common capabilities MUST NOT permit Restore or Cleanup to select
Observe access or bypass their specialized publication rules.

Migration parameters MUST be scoped to an exact edge on the selected path, not
broadcast by parameter name. Every edge's parameters MUST be resolved and
validated before the first edge executes. Unknown edge selectors, undeclared
parameters, duplicates, missing required values and invalid types MUST fail
before Hook execution. A parameter declaration belongs to the destination
Revision's inbound edge, including when several edges reuse a parameter name.

Cleanup parameters MUST be validated before the Cleanup Hook starts. A new
Cleanup execution uses newly supplied parameters and declared defaults, not
values automatically copied from an earlier Run. Finalization-only continuation
MUST neither request Cleanup parameters nor rerun a completed Cleanup Hook.

The CLI accepts `--param`, `--param-file` and `--param-stdin` for Cleanup using
the ordinary parameter identity spelling. Migration prefixes each parameter
identity with `<target-revision-digest>/`; that target selects exactly one edge
in the chosen simple path, including its predecessor. Unique digest prefixes
resolve before acquisition. Unknown/off-path targets and duplicate edge-local
parameters are refused before reading parameter sources. At most one stdin
parameter source is allowed across a Migration invocation, and a path containing
an interactive Hook cannot consume parameter stdin. Parameters are not persisted
as Input Bindings. Abandonment and finalization-only have no Hook parameters.

**Verification: PR-TEST-0714, PR-TEST-0717, PR-TEST-0719, PR-TEST-0722, PR-TEST-0725, PR-TEST-0726, PR-TEST-0735.**

### PR-REQ-0391 - Common ordinary output authority and retention

All Hook operation kinds MUST support ordinary Managed Outputs using the Action
contract. Declarations authorize output identities; they do not oblige the Hook
to submit every declaration. No ordinary-output required/optional flag exists.
A protocol-accepted successful or failed completion may submit any valid subset.
Unaccepted completion, merely written files, guessed handles and duplicate
submissions MUST NOT authorize publication.

Valid submitted ordinary outputs MUST follow Action whole-set validation,
atomic publication and primary/secondary failure semantics. A failed Run may
retain diagnostic artifacts without changing its outcome to success. Earlier
committed Migration-edge reports MUST remain available if a later edge fails.
Ordinary artifacts belong to Run history and MUST survive Instance deletion
until explicit artifact/Run retention or cleanup removes them.

Ordinary output handles MUST remain distinct from Snapshot service content and
Migration target Input outputs. Optional ordinary submission MUST NOT weaken
specialized required submissions, state publication checks or protection rules.
An output publication failure does not roll back external service side effects.

For a successful Capture or Restore, ordinary artifact publication MUST share
the specialized managed-state publication transaction. For successful Cleanup,
ordinary artifacts MUST be preserved before its deletion workflow can finish;
finalization-only continuation MUST NOT recreate or resubmit them.

Migration ordinary artifact identities MUST be scoped by exact source/target
edge within the Run. Identically named reports from different edges MUST NOT
overwrite or reject each other. Each successful edge publishes its submitted
ordinary artifacts with its managed-state edge commit, not at the final chain
boundary. A failing edge may publish eligible diagnostics with its terminal
failure record; earlier edge commits remain intact. An artifact publication
failure MUST NOT advance the failed edge or replay its Hook.

**Verification: PR-TEST-0709, PR-TEST-0710, PR-TEST-0712, PR-TEST-0720, PR-TEST-0721, PR-TEST-0723, PR-TEST-0724, PR-TEST-0725, PR-TEST-0726, PR-TEST-0732, PR-TEST-0733, PR-TEST-0734, PR-TEST-0735, PR-TEST-0736.**
