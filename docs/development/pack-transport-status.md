---
title: Pack Transport Status
---

# Pack transport status

**Status: C S0-S3 implemented, verified and integrated into local develop,
including the approved export naming follow-up.**

The operator approved the [Pack contract](../spec/contracts/pack-distribution-v1.md)
on 2026-09-22: directory/ZIP source and distribution installation, exact export,
private typed common installation, optional portable metadata and overwrite/keep.
The [approved baseline](./design-notes/pack-transport-baseline.md) records scope
and rationale. Schema V11 and Frozen identity contracts remain unchanged. D and E
follow C; D still requires its own design approval. Local commit and merge were
separately authorized. No push, release or publication is authorized.

## Delivered behavior

- One `pack install` entry accepts source/distribution directories and ZIP-based
  `.pack` files. Source keeps `pactrun.yaml`; distribution carries exact canonical
  components and complete runtime blobs, without authoring reconstruction or Hooks.
- `revision export <reference> --output <base-path>` emits Deflate distribution Packs,
  warns about unencrypted authored content, and never overwrites an existing output.
  ZIP64 fields are reserved before blob compression because Deflate can expand a
  near-u32-boundary input; source byte length alone cannot choose a safe header.
- Portable metadata is opt-in on export. Default conflicts refuse atomically;
  explicit overwrite/keep applies only carried presentation fields. Keep reports
  omitted changes. Labels/provenance are idempotent sets; local metadata is excluded.
- Closed manifests, exact names, bounded ZIP preflight, full Deflate/CRC/hash
  validation, private staging and cancellation preserve the publication boundary.
  Export serializes content acquisition with deletion/collection, then compresses
  owned staged bytes outside the transaction. Failed success reporting identifies
  an already-published destination.
- A private frontend-independent Rust candidate and shared installation core serve
  both paths. No public Candidate API, SDK, Bundle object, schema migration,
  machine-output envelope, or service-state transport is introduced.

## Initial C verification

The configured-remote complete `cargo xtask ci` gate passed against source archive
SHA-256:

```text
e73936f0df078979f01ffbce72dcd8527e08c401983ae34b8674f8565b59b4db
```

Archive members and the per-file source manifest were checked before and after
verification in the persistent supported-filesystem workspace. Tests did not run
from /tmp or tmpfs. `CARGO_PROFILE_TEST_DEBUG=0` reduces executable-fixture copying;
no product deadline, test filter or acceptance rule was weakened.

| Status | Scope |
| --- | --- |
| Passed; fresh full gate | Conformance/vectors, traceability, formatting, workspace/all-target/all-feature Clippy, workspace tests, site typecheck and production build |
| Passed | Linux library: 476 passed, four explicit capacity tests ignored in the ordinary suite |
| Passed; separate capacity acceptance | PR-TEST-0548 streamed 536,870,913 runtime bytes through install/export/fresh-store install/re-export on the same final source; the other three pre-existing ignored tests are not claimed as separately executed |
| Passed | Linux system suite: 70; actual CLI: 4; Pack CLI: 1; catalog CLI: 2; Migration CLI: 10; retirement CLI: 2; lifecycle CLI: 2; Artifact CLI: 1; xtask: 40 |
| Passed; focused Windows | Pack tests: 14 passed, capacity test reserved for remote execution; actual Pack CLI: 1; exact source-acquisition selection: 3; output publication/crash tests: 3; unchanged Snapshot transport: 9 |
| Passed; dependency-reused Windows evidence | Existing CLI library: 31; subsequent changes affect Pack-specific paths covered by the final Pack tests, not those existing scenarios |
| Passed | Actual Windows-to-Linux-to-Windows Core V3 Pack transfer with opaque bytes, executable descriptors, Shell-loader declaration and portable metadata; exact identity preserved without Hook execution |
| Passed | Documentation/link/Spec catalog checks: 25 before closeout; site typecheck and production build |
| Not performed | Commit, merge, push, release, Pages workflow changes or deployment |

Initial attempts are not passing full-gate evidence: one superseded run was stopped
after bounded repairs, and another passed Rust verification but exposed the missing
Spec catalog/reading-map registration. Both were superseded by the complete final
gate above. Focused tests also caught a cancellation retry loop and an insufficient
Windows directory handle; those were corrected without weakening the assertions.

This closeout changes only documentation, navigation and its assertions. Unchanged
runtime/test/dependency inputs reuse the source-matched complete gate; source/link/
traceability and configured-remote site checks are rerun for closeout, not described
as another fresh complete product-suite run. This is implementation verification,
not independent review or formal-release acceptance.

## Approved export naming follow-up

After C acceptance, the operator approved a bounded CLI-only naming change on
2026-09-22. Revision export now appends `.pack` and Snapshot export appends
`.snapshot` to `--output <base-path>` unconditionally. An already-suffixed base
receives another suffix. Success reports the safely displayed final path;
no-clobber checks use that final path, not the base. Parent directories are not
created automatically. Input/Artifact raw filenames and Input stdout are unchanged.

The owning rule is [PR-REQ-0357](../spec/behavior/command-and-output-reference.md#pr-req-0357---specialized-envelope-export-filenames).
This supersedes the original complete-output-path CLI wording, not the private
application API that receives an exact publication destination. Import continues
to accept exact filenames including legacy Snapshot `.zip` files. Container bytes,
integrity/identity, sensitive-export authorization, storage and publication
mechanisms are unchanged. Validation is focused on naming, affected CLI/process
journeys and documentation, rather than a new full product CI claim.

The follow-up source archive SHA-256 is
`d21b42ecc6ec119569b834166d98b35ff67027acb06ea162e2cc8d64b042b4b0`;
its per-file manifest was verified before and after remote checks. The final
results-only update below does not change tested runtime inputs.

| Status | Follow-up verification |
| --- | --- |
| Passed | Formatting and Windows/Linux workspace, all-target, all-feature Clippy |
| Passed | Windows and Linux CLI library selections: 34 each, including exact suffix appending, native paths, base/final collisions, unchanged raw exports and Snapshot authorization/import |
| Passed | Actual Pack CLI: 1 on each platform; Linux Artifact CLI: 1; Windows published-output failure regression: 1 |
| Passed | Linux Snapshot system journeys: 11, including V1/V2 import/export/restore with final `.snapshot` paths |
| Passed | Bidirectional traceability, 25 documentation/link/catalog checks, site typecheck and production build; documentation closeout rechecked separately |
| Not run for this bounded follow-up | A fresh full `cargo xtask ci`; the earlier full gate above remains historical C baseline evidence, not a claim about a fresh pipeline after the naming change |

## Local develop integration

After explicit operator authorization, implementation commit
`c09bf196fdf40771bcf9b3ebfbd2edbd73fa9d93` on
`feature/revision-pack-transport` was merged into local develop with no-fast-forward
merge `171545fa09b4a798ddc351e4186c3116184be591`. Both have tree
`0de72ec32d82725d80f7d6117da0f8c8b2fdba75`. There were no conflicts or runtime
changes during merge, and no consumed Git-derived build input was found.

The integration candidate, including the final export naming policy, passed a
fresh complete configured-remote `cargo xtask ci` gate against archive SHA-256:

```text
9e0f85af7a00920f965127bcd47c41077b83e758d91031d5e067c6eb7749e733
```

The archive and per-file source manifests were checked on the persistent supported
filesystem before and after the run; local source matched before commit. The
complete passing run used `CARGO_PROFILE_TEST_DEBUG=0` and `RUST_TEST_THREADS=1`.
The latter serializes the test harness, not the explicit multithreaded and
cross-process scenarios inside tests. No test filter, assertion or product timeout
was weakened. Linux library acceptance was 479 passed and four explicitly ignored;
system acceptance was 70 passed, xtask 40 passed, and the CLI process suites passed.
Formatting, all-target/all-feature Clippy, conformance, traceability, site typecheck
and all 25 documentation checks plus the production build passed. Earlier explicit
capacity evidence above is reused for unchanged transport inputs, not claimed as
a newly executed capacity run during integration.

Two preceding attempts are not passing complete-gate evidence. The first exposed
an old Snapshot export concurrency test still opening its former exact output
path. Its fixture now supplies a base path and reads the appended `.snapshot`
path, retaining the same cross-process deletion and integrity assertions; the
focused test passed on Windows and Linux. A subsequent parallel run encountered
`WouldBlock` in the unchanged Linux retirement interruption test. That test passed
12 isolated repetitions, and then passed in the complete serial-harness run. This
does not claim a conclusively established cause for the parallel-run interruption.

This integration closeout changes documentation and navigation assertions only.
The unchanged runtime gate is reused for the identical merge; documentation/link/
traceability checks and configured-remote site typecheck/build are rerun separately.
D is the next design milestone and still needs its own S0 approval.

## Retained resources and publication boundary

The feature branch is retained after local integration. Existing unrelated untracked
Pages configuration and the user archive were not changed. Source archives,
manifests, logs, Cargo artifacts and the generated documentation site remain in
the dedicated remote workspace; local evidence and isolated dependency cache are
under target/. Cross-platform test Packs/stores are retained as test artifacts.
No preview server was started or stopped, and no sharing or network exposure changed.
