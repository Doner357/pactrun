---
title: M6.5 S0 ServiceStorage Design Baseline
---

# M6.5 S0 ServiceStorage design baseline

**Status: M6.5 implementation direction approved on 2026-09-15. S1-S7 may proceed;
new format Freeze and implementation evidence remain separate gates.**

The user authorized the remaining slices and confirmed source consumption:
mapped sources are replaced atomically, only unmapped source-only associations
are retained. No persistent alias-lineage mechanism is introduced. The original
S0-only stop below records the earlier handoff and is superseded by this approval.
The [implementation record](../m6-5-implementation-status.md) tracks actual
candidate conformance and remaining runtime work; this baseline is not a
blanket implementation or Freeze claim.

This is a concrete proposed design, not a second normative source. Requirement
proposals live under Spec and are conditional on approval. Existing closed
ServiceStorage semantics, Frozen V1 contracts, integrated M6 recovery and
production V6 are unchanged. No new codec, runtime, migration runner, stable
identity vector or production error entry is delivered by S0.

## Agreed product choices

1. Local file and directory resources in Pactrun-provided Instance-scoped Storage.
2. Successful Instance creation provides each declared empty Storage directory;
   declared files/subdirectories may remain absent and are service-created.
3. User tooling is list/show/observe/locate plus existing external tools; there
   is no generic content read/write/import command or automatic tool launch.
4. Each target-owned inbound Migration explicitly declares continuity/mappings.
   Names, paths, bytes and implicit compatibility tags do not infer reuse.
5. Source-only published contracts are retained, queryable and explicitly
   reattachable; no same-name resurrection or automatic service deletion.

These choices were confirmed during planning. The precise encodings, access
policy, target-proposal mechanics and DDL below are submitted for review, not
silently promoted to approved or Frozen contracts by that confirmation.

## Complete design package

| Owning proposal | Exact deliverable | New proposed requirements |
| --- | --- | --- |
| [Revision Core V2](../../spec/contracts/revision-canonical.md) | Closed V1-relative schema, typed identities, locator grammar, authority/prerequisites, normalization/frame and complete mapping predicates | PR-REQ-0317, PR-REQ-0318, PR-REQ-0319 |
| [Pack Source YAML V2](../../spec/contracts/pack-source.md) | Closed projection/defaults, positive authoring and Migration examples, rejection table and V1 preservation | PR-REQ-0320 |
| [Hook Protocol V2](../../spec/contracts/hook-protocol.md) | Exact framing/Session delta, persistent grants, target_ready and target_ready_received messages, termination/publication and failure ordering | PR-REQ-0321, PR-REQ-0322 |
| [Persistence V7](../../spec/persistence/persistence-baseline.md) | Exact DDL proposal, logical invariants, preparation/protection, references, allocation layout and exact V6 upgrade/crash rules | PR-REQ-0323, PR-REQ-0324, PR-REQ-0325 |
| [Execution and M7 handoff](../../spec/execution/m6-5-service-storage-execution.md) | Whole-path compilation, edge admission, atomic target boundary, retained contracts versus failed allocations, M7 receipt/version boundary | PR-REQ-0326, PR-REQ-0327 |
| [Human CLI](../../spec/behavior/m6-5-service-storage-command-reference.md) | Exact command/flag/role selection, output, refusal and exit semantics; proposed diagnostic owners/codes | PR-REQ-0328 |

### Concrete choices submitted for approval

| Review gate | Proposed decision and observable consequence |
| --- | --- |
| Format opt-in | source_format 2 projects Core 2; source_format 1 and installed V1 bytes/digests never auto-upgrade. Hook versions remain independent. No stable V2 identity before S1 conformance/Freeze approval. |
| Locator and overlap policy | Portable ASCII locator supports uppercase and leading periods but excludes spaces/non-ASCII; exact spelling is preserved. Portable case aliases and file ancestors are rejected, nested directory/resource scopes are permitted. |
| Explicit mappings | Every target has a writer. reuse/reattach assert compatible same physical location/kind; transform requires a V2 Hook. Resource create has explicit any/present/absent, creates only a new contract and can deliberately name existing service-owned contents inside an owned allocation. Storage create never adopts an existing directory. |
| Authority and exposure | Write service authority requires Mutate operation access; it still needs the risk handshake and does not lock external services. Hidden hides path/content, not administrative metadata/existence. Retained paths can be disclosed for declared readable access but cannot be obtained for direct write until reattachment. |
| Sensitive service data and nested grants | A directory grant covers its subtree; a hidden child cannot subtract from that positive parent grant. Live contents have no inferred Managed Input Secret floor. Existing Input/Snapshot authorization is not waived; no vault or OS access-control guarantee is introduced. |
| Target readiness | target_ready is a terminal proposal, NOT ordinary success or risk clear. A receipt, observed zero exit, safe output acquisition and owner-held publication precede atomic target/risk commit. No future Hook work or inferred restart is needed after commit; Pack owns service coordination. |
| Ownership representation | Dedicated typed association tables preserve exact last declaration contracts. Allocation IDs/owners and protected custody are independent of Instance cascade. Presence, payload hashes and native paths are not authoritative DB fields. |
| Recovery references | Preparation pins its origin contract; published associations and Running path pins hold needed contracts. Historical allocation origin Revision/Run IDs are audit links, not permanent installation/history pins. Protection is monotonic and is not an execution pin. |
| Failed target allocations | Once exposed, they remain protected and inspectable as uncommitted attempts, not published resource associations. No automatic retry adoption or failed-attempt reattachment CLI is included. |
| M7 boundary | V7 has no unused Cleanup/Abandon receipt tables. M7 must add explicit versioned receipt/handoff representation before destructive finalization or Instance removal; it cannot erase V7 custody or silently extend V7. |
| CLI/diagnostics | New service-storage/resource human namespaces only. Existing migration, Snapshot, input and storage-upgrade spellings remain; new error owners/codes are pending append-only registry approval, not active S0 entries. |

Approval is required for this coherent package before implementation. In
particular, locator restrictions, hidden/retained access semantics, existing-byte
resource declaration and the failed-attempt limitation are explicit decisions,
not matters left for an implementer to invent later. A requested change to one
decision must be propagated through dependent schema/protocol/CLI contracts.

## Contract alignment review

| Owning constraint | Design reconciliation |
| --- | --- |
| PR-REQ-0235/0236/0241/0242 | Service bytes remain authoritative to the service; typed declarations/associations and point observations are separate. No Input mirror, content hashing, live-state version token or cross-Instance shared allocation. |
| PR-REQ-0237/0245 | Target-owned mappings and persisted last contracts govern retention/reattachment; create cannot replace an existing typed association. Package-defined compatibility is explicit, not derived from paths or bytes. |
| PR-REQ-0238/0246 | Service transformations are Hook-owned; target_ready leaves risk Open until the Pactrun-owned target boundary, with no service bytes in SQL. |
| PR-REQ-0056 | Ordinary ResolveRecoveryRisk still clears durably before clear ack. A transform Hook cannot use it to acknowledge target-only coherence; target_ready is a distinct V2 proposal and receipt, not a renamed clear ack. |
| PR-REQ-0057 and Frozen PR-REQ-0216 | Ordinary success-with-Open is still rejected. target_ready has no success status and creates no successful Hook completion row; only the later target transaction produces durable success. V1 messages/decoder remain unchanged. |
| PR-REQ-0061/0062/0063/0065 and M6 | No durable Plan or replayable proposal. Durable Open risk, committed boundaries, typed references and guards suffice after owner loss. Target association publication joins the existing atomic Pactrun boundary. |
| PR-REQ-0070 | Clearing a transform Run's risk does not clear a pre-existing manual-recovery guard. Restore keeps its existing qualified atomic guard-clear rule. |
| PR-REQ-0239 | Capture still submits a chosen immutable recovery representation. Existing integrity V1/V2 and bundle V1 are unchanged; no automatic scan or one-to-one resource provenance is invented. |
| PR-REQ-0243/0244/0170 | Read exposure, mutation route, authority, presence and OS identity remain separate. Whole-storage access is explicit, not silently derived from a resource grant or prerequisite. |
| PR-REQ-0240/0248 | Core/Hook evolution is independent; no V1 extension, metadata backdoor, opaque state map or speculative stable API. |
| PR-REQ-0247 | No M6.5 destructive finalization. M7 owns the ambiguous Cleanup receipt window, do-not-replay/finalization proof and non-destructive handoff via a separately versioned schema. |

The new target proposal is the highest-risk review item. It deliberately avoids
requiring a Hook to wait for risk clear before it can finish: receipt is not
clear, the Hook exits, and the owner then publishes target plus clear. A crash
in that interval conservatively requires manual recovery. This does not claim
proof that service bytes still match source or that peer receipt is durable.

## Requirement-to-future-test matrix

The D65 case names below are design scenarios, NOT PR-TEST IDs or test artifacts.
At S0 their new requirements had no automated evidence. The implementation
mapping below now identifies the concrete tests; prior green tests alone never
closed ServiceStorage runtime claims. Final-source results remain in the
[implementation record](../m6-5-implementation-status.md).

| Case | Requirement scope | Required future evidence |
| --- | --- | --- |
| D65-01 | PR-REQ-0317/0320, PR-REQ-0240 | Closed fields/defaults, raw YAML/JSON numeric and duplicate-key rejection; V1 does not accept new fields or auto-select V2 |
| D65-02 | PR-REQ-0318 | Reordered semantic sets yield identical component/frame bytes; ordered args differ; version marker/frame mismatch fails; independent Node parity, no runtime paths/bytes in identity |
| D65-03 | PR-REQ-0317/0319, PR-REQ-0241 | Duplicate identities, portable aliases, wrong resource kind and file-ancestor paths fail; nested directory/file contracts remain representable |
| D65-04 | PR-REQ-0323/0324, PR-REQ-0236 | New Instance has every Storage or no published Instance; declared resources remain absent; real Windows/POSIX filesystem barriers and allocation collision rejection |
| D65-05 | PR-REQ-0321/0326, PR-REQ-0244 | Cross-Instance/foreign handles rejected, resource grant never expands to storage, write access requires Mutate, no prerequisite-derived grant |
| D65-06 | PR-REQ-0326/0328, PR-REQ-0242 | Present/Absent/Unknown, wrong-kind Present, unsafe link/prefix and permission failures; external live change does not advance InstanceStateVersion |
| D65-07 | PR-REQ-0328, PR-REQ-0243 | Hidden metadata versus path disclosure; direct versus operation routes; retained readable lookup but no retained write; no tool launch/content transfer |
| D65-08 | PR-REQ-0319/0326 | Each target has one writer; role errors and silent resurrection fail; resource create present/absent/any checked without byte parsing or external-directory adoption |
| D65-09 | PR-REQ-0319/0327, PR-REQ-0237/0245 | Real allocation continuity under rename mapping; source-only contracts survive; explicit reattach reuses exact retained allocation and locator, no automatic same-name reconnect |
| D65-10 | PR-REQ-0319/0322, PR-REQ-0238 | Real Hook locator/kind transformation, split/merge and nested authority; Pactrun performs no service transformation itself |
| D65-11 | PR-REQ-0321/0322 | Exact V2 preamble, Session and proposal bytes; wrong/missing/duplicate handles, non-transform proposal and ordinary success-on-transform all rejected |
| D65-12 | PR-REQ-0322/0326, PR-REQ-0056/0057/0246 | Crash before/after risk entry, proposal receipt, Hook termination, output acquisition and target commit; no premature clear or false success |
| D65-13 | PR-REQ-0322/0326 | Nonzero exit, cancellation, extra message, receipt loss, acquisition and publication failures; owner retry is not Hook replay; final-edge success is atomic |
| D65-14 | PR-REQ-0323/0326, PR-REQ-0063/0065 | Fresh-process recovery needs no Plan; current associations and exact pins are consistent; intermediate edges survive, protected service roots outlive execution cleanup |
| D65-15 | PR-REQ-0323/0324/0327 | Unexposed empty preparation cleanup only after confirmed owner loss; unexpected bytes/link paths preserved; protected failed target has origin evidence but no published resource/automatic adoption |
| D65-16 | PR-REQ-0323/0325 | Exact DDL, STRICT/WITHOUT ROWID/FKs, wrong owner and impossible logical matrices rejected; no content/presence metadata mirror |
| D65-17 | PR-REQ-0325 | Exact V6 upgrade/no-op V7, live/Unknown writer blocking, late V6 writer rejection, before/after upgrade crash and unchanged V1 identities/Runs/Snapshots/Inputs |
| D65-18 | PR-REQ-0327, PR-REQ-0247 | V7 custody cannot cascade away; M7 later needs real Cleanup ambiguity, receipt, retry and abandonment/GC tests. S0/M6.5 tests do not certify M7 behavior |
| D65-19 | PR-REQ-0321/0326, PR-REQ-0239/0070 | Real Capture selects only submitted service representation; Restore gets current target service authority plus staged Input view; pre-existing guards follow M6 rules |
| D65-20 | PR-REQ-0328 | Exit 0/1/2, explicit role selection, Unknown observation, safe human escaping, broken stdout; queries allocate/execute/upgrade nothing |

## Implementation audit of the S0 scenarios

| Scenario | Concrete implementation evidence |
| --- | --- |
| D65-01 | PR-TEST-0331, PR-TEST-0333, PR-TEST-0336, PR-TEST-0337, PR-TEST-0384: closed source versions, raw token rejection and V1 preservation |
| D65-02 | PR-TEST-0331/0332 and PR-TEST-0383: fixed independent byte/digest vectors, set reordering versus ordered args, stable installed identity despite live changes |
| D65-03 | PR-TEST-0334, PR-TEST-0371 and PR-TEST-0386: intrinsic aliases/ancestors, Package lineage and real nested directory/file targets |
| D65-04 | PR-TEST-0345 through PR-TEST-0348: native no-replace barriers, eager isolated roots, absent objects and allocation publication crashes |
| D65-05 | PR-TEST-0364, PR-TEST-0368, PR-TEST-0370 and PR-TEST-0386: exact grants, foreign/owner pin refusal, real resource and nested scope execution |
| D65-06 | PR-TEST-0354/0355/0357 and PR-TEST-0387: real observation, links, unavailable roots and Linux permission denial; no continuous-observation or external-writer exclusion claim |
| D65-07 | PR-TEST-0350, PR-TEST-0357/0359 and PR-TEST-0383: hidden/readable/direct/operation routes and retained read without retained write |
| D65-08 | PR-TEST-0334/0335, PR-TEST-0371/0372 and PR-TEST-0388: explicit writers/roles and actual present/absent/any creation checks without byte adoption |
| D65-09 | PR-TEST-0374/0375 and PR-TEST-0383: real consumed renames, old/new crash boundaries, retention and cross-version reattachment |
| D65-10 | PR-TEST-0382/0386: real split/merge and file-to-directory transformation with explicit nested grants; Hook-created service parents |
| D65-11 | PR-TEST-0360 through PR-TEST-0365: framing, handshake, closed messages and Session-wide authority/target-handle checks |
| D65-12 | PR-TEST-0370, PR-TEST-0377/0378, PR-TEST-0380/0381: durable risk requests, proposal/commit crashes, intermediate output boundaries and no premature clear |
| D65-13 | PR-TEST-0377/0378/0381: nonzero exit, extra messages, cancellation/deadline, invalid output acquisition, cleanup failure and owner loss without target replay |
| D65-14 | PR-TEST-0368, PR-TEST-0375/0378/0380: durable pins, fresh-process reconciliation and complete intermediate commits |
| D65-15 | PR-TEST-0351/0352 and PR-TEST-0376: confirmed-loss absent-intent retirement, conservative existing-object preservation and protected failed targets |
| D65-16 | PR-TEST-0340, PR-TEST-0349/0350 and PR-TEST-0368: exact documented DDL, structural constraints and independent logical custody checks |
| D65-17 | PR-TEST-0341 through PR-TEST-0344 and historical V6 preservation tests: explicit exact upgrade, quiescence, late writers and crash boundaries |
| D65-18 | PR-TEST-0340 and PR-TEST-0376: V7 protection/custody cannot cascade away. M7 receipt, deletion and abandonment execution remain expressly excluded |
| D65-19 | PR-TEST-0370: installed V2 Capture/Restore, explicit service recovery representation and retained M6 guard rules |
| D65-20 | PR-TEST-0357/0359 and PR-TEST-0383: human CLI roles, exit behavior, no implicit execution and safe path disclosure |

This is an evidence map, not a claim of independent review or a substitute for
running the final exact-source suite. Native process-crash tests and persistence
barriers do not constitute empirical power-cut certification. V7 intentionally
preserves existing preparation directories when their creation cannot be proven;
an intent row is not authority to remove them.

## Historical S0 validation and stop boundary

Validate document links, unique requirement definitions, catalog statuses and
traceability without adding fake tests. Validate the proposed DDL's syntax,
foreign-key graph and empty-schema delta in a disposable in-memory SQLite
database built from the current V1-V6 definitions. Such a check does not run an
upgrade against user storage, prove the proposed logical invariants, or deliver
a V7 migration implementation. Inspect examples and cross-document consistency;
leave independent codecs, oracles, golden vectors and runtime scenarios to S1+.

Run the existing full configured-remote `cargo xtask ci` after the final document
and navigation edit, using the persistent checkout's own target/dependencies,
and compare the exact source manifest before and after. Report local checks,
DDL design check and full CI separately. A passing existing suite proves only
document integration and regression preservation, NOT ServiceStorage runtime.

S0 delivery stops with these complete proposals, validation evidence and the
approval table above. M6.5 remains Proposed; none of Core V2, YAML V2, Hook V2,
V7 or new error owners is activated or Frozen. No S1-S7 implementation, new
production codec/migration, commit, merge, push or deployment is authorized.
The existing [M6 closeout](../m6-implementation-status.md) remains complete and
the [staged roadmap](../implementation-roadmap.md#m65---servicestorage) retains
M6.5 before M7. Further work requires a new user instruction.
