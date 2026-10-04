---
title: Debian 12 artifact qualification
---

# Debian 12 artifact qualification

**Status: Implemented, verified on GitHub and integrated into develop on
2026-10-04; the original CI milestone did not publish a product.**

The owner requested a GitHub CI path on 2026-10-04 after the public alpha.3 Linux
payload failed on Debian 12 with system glibc 2.36. Its weak GLIBC_2.39 symbol
requirements are still loader requirements: merely installing Homebrew glibc
did not make the executable use it. Published alpha.3 remains immutable and
still has its documented glibc 2.39 requirement. This work does not update public
package definitions, product versions or installed user software.

**Subsequent delivery:** The separately requested [alpha.4 release](./alpha4-publication.md)
uses this qualified recipe. The no-publication boundary below describes the
original CI task, not the current release availability.

## One build path, independent runtime environments

The reusable `linux-compatibility.yml` workflow is used by normal runtime CI and
by the Linux leg of the manual Release candidate workflow. Windows packaging is
unchanged. A single Linux build supplies both runtime gates; neither test host
has Cargo or compiles a replacement Pactrun binary.

1. **Build and inspect:** official Rust 1.98.1 Bookworm image, pinned by its
   linux/amd64 digest. The existing clean snapshot/artifact builder captures
   provenance, legal notices and checksums. Audit both launcher and payload with
   readelf: x86-64 ELF, ordinary system interpreter, no private RPATH/RUNPATH,
   and no required glibc version above 2.36, including weak versioned references.
2. **Direct runtime:** a separate pinned Debian 12 slim image with no Homebrew
   or Cargo. As an ordinary user, unpack and test the exact normal, test and
   standalone archives, version/source identity, help parity, Secret export
   refusal, a real Shell Loader/Hook and managed-storage retirement. The immutable
   public alpha.3 archive is a negative control: checksum verified, it must fail
   specifically for the previously observed GLIBC_2.39 mismatch.
3. **Homebrew runtime:** another clean Debian 12 image, ordinary user, standard
   `/home/linuxbrew/.linuxbrew` prefix and a pinned Homebrew commit. Stage the
   checked-in Preview formula by changing only candidate URL/version/checksum;
   use a loopback artifact server instead of the published alpha.3 URL. Install
   normally, without dependency bypasses or a custom libc search path, and
   compare installed launcher/payload hashes with the ABI-audited build. Exercise
   the real Hook/storage path, formula test and uninstall. Record actual package
   dependencies separately from successful execution; no promise that GCC/glibc
   downloads disappear is made.

Both runtime jobs verify Debian 12, x86-64 and system glibc 2.36. Store fixtures
use runner-backed temporary directories, not an unsupported container overlay
filesystem. The full Ubuntu/Windows source gates remain; this is an added
artifact compatibility gate, not a replacement for product tests.

## Gate, provenance and limits

Runtime CI requires this workflow to succeed in the existing aggregate CI gate.
Documentation-only scope skips it explicitly; failure/cancellation/unexpected
skips cannot satisfy runtime verification. No branch-protection settings change.
Manual release builds remain read-only and keep their source-ref checks. Artifacts
are only qualified when the whole workflow, including both runtime jobs, passes.
Failed build artifacts are not permission to publish.

Containers share the hosted runner kernel. Passing this workflow qualifies the
tested Debian 12 userland/ABI and selected lifecycle paths, not every Debian 12
kernel, filesystem, service or network configuration. The reported Debian 12
machine remains useful for a later bounded real-host confirmation, not as a
mandatory build host. No host libc replacement or global LD_LIBRARY_PATH is used.

For archived releases, use their recorded source, original workflow revision and
build environment; do not rebuild alpha.3 with this new recipe and replace its
published bytes. A future public executable needs its own new version, complete
release qualification and explicit publication authorization. This CI work does
not authorize a main merge, Pages deployment or alpha.4 publication.

## Actual verification and integration

- Implementation: `de3ef214e85f64e0dd142146bbbf8d004e4fb11b`.
  PR #23 merged into develop at `bbeb2a7eab115c957a2d77435b8d6d06fffe62ca`.
- Full GitHub CI run `37172707145` passed all required jobs: existing Ubuntu and
  Windows source verification, the three new Debian jobs, and the aggregate gate.
  Pages was skipped. The candidate's exact source was the synthetic PR merge
  `4dc6ec848052997cb527a82e6d93224c0488f590`; the merged implementation has the
  same content. CI artifacts are not an alpha.3 replacement release.
- Both audited executables used the ordinary system interpreter, no embedded
  RPATH/RUNPATH, and had a highest observed glibc symbol requirement of **2.34**,
  below the enforced 2.36 ceiling. This observation is not qualification of
  every glibc 2.34 distribution: actual userland validation was Debian 12/2.36.
- Clean direct runtime: UID 1001, no Cargo/Homebrew, 33 CLI invocations across
  normal, test and standalone archives. Help/source identity, Secret refusal,
  real Hook execution and storage retirement passed. The known public alpha.3
  checksum matched and its GLIBC_2.39 loader failure was reproduced.
- Homebrew runtime: UID 1001, system glibc still 2.36, standard Linux prefix.
  Installed payload and launcher hashes matched the same ABI-audited build.
  Eleven CLI lifecycle invocations, formula test and uninstall passed.
- Homebrew still installed 12 extra formulae, including `glibc 2.39_1` and
  `gcc 16.2.0`; its dependency list is retained, not hidden or bypassed. Direct
  runtime success independently proves that those private libraries did not
  mask a remaining system-glibc incompatibility.
- Offline tooling checks passed on Linux (13 tests). Windows preflight passed
  12 of those tests with the POSIX symlink case skipped. The Linux CI scope test
  exercises the aggregate gate with Debian success/skip/failure/cancellation and
  missing-result cases. Workflow syntax and current documentation checks passed.

Workflow bring-up caught scoped container Git trust/path translation and an
incorrectly dereferenced Homebrew bin shim. Those setup issues were fixed and
regressed; neither the ABI ceiling nor normal dependency installation was weakened.
The Docker digests and Homebrew commit are explicit workflow inputs; base
userland, compiler, ABI reports, dependencies and runtime receipts are retained
with Actions artifacts and locally under `target/debian12-ci-20261004/`.

The documentation-only closeout reuses unchanged runtime/tooling validation and
retained candidate evidence; only informative records differ. It runs its own
documentation/CI checks, not another fresh full runtime gate.
At CI closeout, main, alpha.3 binaries, exact definitions, package catalogs and
Pages remained unchanged. Real-host confirmation and public delivery are separate
steps with their own evidence and authorization; see the subsequent alpha.4 record.
