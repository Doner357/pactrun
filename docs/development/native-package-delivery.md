---
title: Native Package Engineering Delivery
---

# Native package engineering delivery

**Status: E implementation; actual-manager candidate acceptance remains required. No public publication is authorized.**

Scoop (Windows x86-64) and Homebrew (Linux x86-64) own download, checksum
verification, extraction, installed inventory, activation, cleanup and uninstall.
Pactrun does not self-update. No package hook creates, moves or deletes management
roots, Instances, Inputs, Secrets, Snapshots or service resources.

## Program layout and data boundary

Each native package contains a small launcher at bin/pactrun (normal) or
bin/pactrun-test (test), the adjacent libexec/pactrun payload, and the corresponding
pactrun-source or pactrun-test-source helper. Windows names have .exe suffixes.
The launcher passes native argument vectors, stdio and exit status without shell
expression evaluation. Linux replaces the launcher process with the payload;
Windows waits while the product owns cancellation and cleanup.

An explicitly supplied PACTRUN_STORAGE_ROOT is forwarded unchanged, including
an invalid empty value that the product will reject. Otherwise normal/test use
separate defaults:

| Host | Normal | Test |
| --- | --- | --- |
| Windows | LOCALAPPDATA/pactrun | LOCALAPPDATA/pactrun-test |
| Linux | XDG_DATA_HOME/pactrun, or HOME/.local/share/pactrun | corresponding pactrun-test |

These paths are outside program, Cellar, Scoop app and persist directories.
Management roots must be provisioned explicitly as trusted directories on a
supported filesystem, with database and runtime-content child directories.
Choose an existing trusted parent, create a new private root using native OS
permissions, and do not reuse or clear an existing directory to bypass a format
refusal. Package installation itself does not provision that data. The current
SQLite bootstrap atomically creates the baseline in a supplied pristine root.
Separate service resources are also necessary for meaningful test isolation.

## Native sources and saved eligibility

Use a **separate native clone for each installation**. For example, normal and
test Scoop buckets may be named pactrun-preview and pactrun-preview-test; Homebrew
taps may be pactrun/preview and pactrun/preview-test. Both can point at the same
source repository, but must not share their local Git clone. Always use qualified
bucket/tap package names for install, update, reinstall and uninstall.

The source helper accepts only:

```text
pactrun-source --source-root NATIVE_CLONE --major 1 --stable
pactrun-source --source-root NATIVE_CLONE --major 1 --preview
pactrun-source --source-root NATIVE_CLONE --exact 1.0.0-alpha.1
```

The test helper has the same syntax. Sources are Major-scoped; moving to another
Major requires a separate source and an explicit native installation/switch,
not an ordinary update. The helper refuses dirty/ignored worktree files and
unpublished local commits instead of discarding them. It fetches only the selected
metadata ref, sets native branch tracking, and explicitly sets origin/HEAD because
Homebrew does not use a checkout alone as the selection policy. It verifies those
postconditions. It never downloads, activates or removes an executable.

Changing eligibility therefore leaves the installed program unchanged. Run the
native manager separately to update. Exact selection is immutable. Normal updates
must not downgrade; an older executable requires an explicit native switch or
reinstall workflow, then an actual version check. Its ability to interpret the
selected data remains Pactrun's independent check. Do not force-unlock or stop an
operation to enable an update; if Scoop defers, retry after the operation ends.

A stable source with no eligible formal release contains NO-ELIGIBLE-RELEASE.txt,
not a prerelease fallback. Preview eligibility includes formal releases and can
later advance to a higher-Minor prerelease. The repository default ref is an
explicit publisher choice, and the initial E rehearsal enrolls in a clearly
identified preview source. No formal 1.0.0 is implied.

## Source-qualified build and metadata commands

Developer tooling uses Python 3 and Cargo; installed packages do not require
Python or Rust. All steps operate locally or on the authorized test host.

```text
python tools/release_artifacts.py source --root REPOSITORY --output NEW_SNAPSHOT_DIRECTORY
python tools/release_artifacts.py build --root EXTRACTED_SOURCE --snapshot SNAPSHOT_DIRECTORY --output NEW_BUILD_DIRECTORY --platform windows-x86_64 --url-base LOOPBACK_ASSET_BASE
python3 tools/release_artifacts.py build --root EXTRACTED_SOURCE --snapshot SNAPSHOT_DIRECTORY --output NEW_BUILD_DIRECTORY --platform linux-x86_64 --url-base LOOPBACK_ASSET_BASE
cargo xtask release-sources RELEASES.json NEW_PLAN_DIRECTORY
cargo xtask release-publish-local RELEASES.json LOCAL_BARE_REPOSITORY EXPLICIT_DEFAULT_REF
```

Source capture refuses dirty/uncommitted selected inputs and never archives
credentials or Git metadata. Build verifies committed source checksums before and
after Cargo and supplies their commit/manifest identities to build.rs. The built
program reports its actual product version, target, compiler and optional source
provenance in the version machine result. Build checks these values rather than
trusting an archive filename. Archives have deterministic entry ordering/times,
SHA-256, byte sizes and standalone counterparts. Windows release builds request
the static CRT; actual binary imports and Linux loader/symbol dependencies still
require inspection and recorded platform qualification.

Combine the matching Windows/Linux release-part.json records into one Release
record per product version (four normal/test artifacts). Supported format lists
and default format maps are separate; list order never selects a default. The
shared Rust version policy validates eight domains, explicit defaults, product
Major association, canonical identifiers, immutable version uniqueness and the
artifact matrix. RC/formal products require formal defaults. This is a gate,
not evidence that a hypothetical future formal codec has been implemented.

The local publisher merges existing immutable release records, rejects a changed
exact version, and advances channels using product precedence rather than upload
time. It commits only validated normal-manager definitions and updates refs in a
single Git transaction. Omitting old inputs cannot move a channel backwards.
An existing repository must be an explicitly marked, local bare source publisher;
working repositories are refused. **There is no push operation.**

The local artifacts, loopback downloads and native-manager tests are engineering
rehearsals. Publication, public GitHub tags/Releases/buckets/taps, minimum-environment
qualification and actual-use signoff remain separate gates. Current evidence must
identify the exact source, binary, manager revision, host profile and commands;
previous prototype results do not certify the E candidate.

## Verification scope

PR-TEST-0625 checks the real version support report without storage access.
PR-TEST-0626/0627 check launcher boundaries. PR-TEST-0628/0631 exercise source
selection syntax and real Git tracking. PR-TEST-0629/0630 cover shared metadata
selection and promotion gates. PR-TEST-0635 checks archive reproducibility and
source refusal; PR-TEST-0636 checks actual local ref immutability and monotonic
channel publication. Native Scoop/Homebrew acceptance and full E CI are still
tracked in [the E implementation ledger](./e-implementation-status.md).
