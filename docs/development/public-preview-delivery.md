---
title: Public Preview delivery
---

# Public Preview delivery

**Status: Implementation and qualification in progress.** On 2026-09-30 the owner
authorized the public `Doner357/pactrun` repository, MIT licensing, and the commits,
merges, pushes, tags, GitHub Release, Pages and package publication needed for this
milestone. This authorization is not a claim that publication or qualification has
already succeeded. No formal 1.0.0 promotion is implied.

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
