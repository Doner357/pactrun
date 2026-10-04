---
title: Public Preview delivery
---

# Public Preview delivery

**Status: Completed and publicly published on 2026-09-30.** On 2026-09-30 the owner
authorized the public `Doner357/pactrun` repository, MIT licensing, and the commits,
merges, pushes, tags, GitHub Release, Pages and package publication needed for this
milestone. The following records distinguish actual qualification and publication
from the original authorization. No formal 1.0.0 promotion is implied.

## Published identity and evidence

- Repository: [Doner357/pactrun](https://github.com/Doner357/pactrun), public, MIT,
  with `main` as its default branch.
- Release: [v1.0.0-alpha.1](https://github.com/Doner357/pactrun/releases/tag/v1.0.0-alpha.1),
  explicitly marked Preview/prerelease. Stable remains unavailable.
- Version tag: `2eb5b43107231dc10d3c8dc97cbe6aa54c6ed4f6`, the reviewed delivery record.
- Binary source: `4e8366975d95e0fa6973c7a1fb9eea6a99dcf628`; both targets used
  Rust 1.98.1. Later changes affect packaging, verification and documentation, not
  the runtime/dependency inputs. The Release includes the exact build-source
  archive, manifest, provenance and SHA256SUMS; no qualified binary was rebuilt
  during public upload.

| Verification | Result and boundary |
| --- | --- |
| Configured Linux complete gate | Passed `cargo xtask ci` plus catalog verification on source `4956d2b`, with opt-level 1, no debug symbols, debug assertions and overflow checks enabled |
| Hosted integration CI | [Passed on Windows and Ubuntu](https://github.com/Doner357/pactrun/actions/runs/36703489016) for `1afbab6`; the only changes after the remote gate were Windows Node/checkout prerequisites |
| Published main CI | [Passed all required jobs and Pages deployment](https://github.com/Doner357/pactrun/actions/runs/36709027660) after the test-isolation correction |
| Isolated native lifecycle | Passed 36 Windows and 34 Linux assertions, including update, hold/pin, exact switch, active Hook lifetime, checksum failure recovery and source relocation |
| Actual public acquisition | Passed 18 Windows and 15 Linux assertions from the public main-branch source and actual Release URLs, including real Pack/Instance/Hook use and data-preserving reinstall/removal |
| Retirement fixture follow-up | A repeated hosted run exposed transient lock contention consistent with fork inheritance; the uncontended proof was process-isolated, all 14 retirement tests passed, and five additional eight-thread runs passed without dropping contention assertions |
| Documentation checks | Passed all 89 source, link, reading-path and traceability checks; production-site acceptance is recorded at closeout |
| Public artifact bytes | All 11 Release assets downloaded anonymously and matched the retained size and SHA-256, including the checksum file and source receipts |
| Pages | [Public website](https://doner357.github.io/pactrun/) deployed; all 18 browser checks passed on the actual GitHub Pages URL, including search, exact anchors, agent text, mobile layout and error recovery |
| Hosted candidate rebuild procedure | [Passed on Windows 2025 and Ubuntu 24.04](https://github.com/Doner357/pactrun/actions/runs/36705688007); does not replace published assets |

Assertion counts include artifact checks; they are not independent product-test or
certification counts. Public testing used isolated native-manager prefixes and
stores, not the operator's installed applications or real service data. The
temporary Windows PATH entry was removed and its before/after fingerprint matched.
Local/remote detailed receipts are retained in the delivery evidence workspace.

Initial attempts exposed environment/setup issues rather than grounds to weaken
contracts: an unsafe group-writable IPC temporary root, missing pinned-toolchain
components, Windows CRLF Markdown and a preinstalled Node 22 instead of Node 24.
Those prerequisites were corrected. Scoop's hook behavior also required using its
explicit abort operation to reject a command conflict; the failed test was retained
and the corrected native scenarios passed. Superseded/failed attempts are not
counted as passing qualification.

The post-publication retirement change is confined to test isolation and Linux CI
scheduling. A controlled probe reproduced the fork-before-exec lock lifetime;
production remains fail-closed on contention. No runtime code, artifact, checksum,
format or published tag was replaced as part of that correction.

## Platform and remaining product limits

Native artifact tests used Windows 11 build 26200 on NTFS and Debian 13 on ZFS.
Linux product imports reach GLIBC_2.39; this is a library floor, not proof for every
kernel, distribution or filesystem. Windows imports are OS DLLs, with no separate
MSVC runtime dependency observed. No Windows 10, macOS, ARM64 or musl artifact is
promised. Native tests used pinned Scoop/Homebrew revisions recorded in their
receipts; arbitrary future manager versions are not certified.

This delivery does not claim complete Authentik black-box acceptance, platform code signing,
formal 1.0.0 promotion or general prerelease data downgrade compatibility. All eight
format domains remain `1.0-alpha.1`. The alpha.2 software-only fixture was used only
for isolated update testing and was not published.

## Authentik black-box follow-up

The 2026-09-30 external evaluation is **Partially verified**, not full acceptance.
Its Linux executable hash matched the released alpha.1 artifact. It demonstrated
public Homebrew acquisition, formal Pack export/import with exact Revision
preservation, Hooks without the author source present, four healthy services,
authenticated API access, stop/start persistence of an API-created user, and
independent startup in a second fresh management store on the same host.
This proves Pack portability in that environment, not service-data migration.

Three deletion cases failed at `finalize_storage` with
`service_storage/allocation_unavailable` despite successful Cleanup. Inspection
of two initialized deployments showed foreign-owned PostgreSQL directories that
the CLI user could not list or write and Redis directories it could not write.
This strongly supports a filesystem-permission cause; the handoff did not capture
a raw OS error proving the cause of every failure. Retained obligations and
partial-finalization evidence remained. Administrator-assisted discard completed
cleanup separately; it does not qualify ordinary-user retirement.

The evaluation did not qualify trusted TLS (certificate checks were disabled),
browser SSO, Windows, cross-host data movement, Snapshot/Restore, upgrades or
credential rotation. Expected negative cases must not be counted as product
failures or converted into a statistical pass rate. The supplied archive remains
private local evidence; this summary does not publish its service data or scripts.

The [retirement guide](../guides/retirement.md#container-permissions-in-the-first-preview)
records safe interpretation and recovery boundaries. Runtime follow-up must
reproduce cross-UID access failures, review Core/Pack responsibilities, investigate
safe diagnostics and read-only handoff permission coupling, and rerun the real
service lifecycle. Documentation correction alone does not resolve that failure.

## Deferred documentation experience

Recorded on 2026-10-01: retain Docusaurus for now. A future visual redesign may
use the Kano Proxy documentation's restrained reading layout as a reference;
framework migration is not approved or required by this record.

Also deferred: release-aligned documentation snapshots, starting with a clearly
identified alpha.1 view, distinct unpublished development content, and corrections
that preserve each version's applicability. Stable/Preview installation channels
are not documentation versions. Decide snapshot granularity and validate old
URLs/anchors, version-aware search, command references, agent text and publication
digests before enabling this feature. Use version directories in the same
repository, not permanent package-channel branches. Neither deferred item is part
of the documentation correction or alpha.2 runtime fix.

## Approved delivery model

The project retains Git Flow: development through `develop`, reviewed version
records on `main`, and release tags identifying those records. Package definitions
are ordinary source-controlled files on main, not a parallel set of channel refs.
The default repository branch for native acquisition is main.

- `pactrun` is Major 1 stable; `pactrun-preview` is Major 1 formal-plus-prerelease.
  With no eligible formal release, the stable definition is absent, not an alias
  for alpha. Future Majors require distinct package names and an explicit switch.
- Both entries expose the same `pactrun` command and default management root.
  Uninstall one program package before installing another. Command-owner guards
  reject overlapping installation; no package hook creates or deletes product data.
- `pactrun-test-preview` and exact test entries expose `pactrun-test` with its
  separate default root. Preview is not automatically a data sandbox.
- Exact entries, for example `pactrun-exact-1-0-0-alpha-1`, refer to immutable
  versioned artifacts. Native hold/pin freezes an installed moving entry without
  changing its executable; exact installation is an explicit program switch.
- Repository relocation changes the native source URL, not the executable or
  management data. No source-selection helper is shipped in these packages.

The E source-ref publisher/helper remain developer fixtures for their historical
tests; they are not the public distribution mechanism and no such branches are
published. `cargo xtask release-catalog` generates the new ordinary project files.
It validates the full artifact matrix and compares previous published records so
release identity cannot be rewritten or silently removed.

## Work and acceptance

1. Implement and test catalog generation, immutable records, conflict refusal and
   package/legal payloads; record dependency notices.
2. Qualify actual candidate and software-only update fixtures with real isolated
   Scoop/Homebrew: install, hold/pin, refresh, upgrade, exact switch, failed install
   recovery, source relocation, active Hook lifetime and data preservation.
3. Run full source-qualified CI on the configured persistent Linux workspace and
   Windows-specific checks. Record actual hosts and binary prerequisites without
   asserting unsupported platform coverage.
4. Audit publishable history, integrate reviewed work, publish the qualified
   candidate artifacts and ordinary package definitions, and verify actual public
   acquisition on both managers. Never publish the alpha.2 fixture.
5. Run hosted GitHub CI, activate the authorized Pages site, and validate actual
   URLs. Update the installation guide and close out exact source/artifact evidence.

Building occurs before inserting its archive hashes into package metadata. A
delivery receipt links the binary source commit to the later metadata/release
commit; the same tested archives are published rather than rebuilt at the end.
Local qualification, hosted CI, public native acquisition and Pages acceptance are
separate results. Any remaining gate must be reported rather than hidden by a
successful upload.
