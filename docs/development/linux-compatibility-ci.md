---
title: Debian 12 artifact qualification
---

# Debian 12 artifact qualification

**Status: CI implementation and qualification in progress; no new public release.**

The owner requested a GitHub CI path on 2026-10-04 after the public alpha.3 Linux
payload failed on Debian 12 with system glibc 2.36. Its weak GLIBC_2.39 symbol
requirements are still loader requirements: merely installing Homebrew glibc
did not make the executable use it. Published alpha.3 remains immutable and
still has its documented glibc 2.39 requirement. This work does not update public
package definitions, product versions or installed user software.

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

Verification results will be recorded after actual GitHub execution. The Docker
digests and Homebrew commit are explicit workflow inputs; base userland, compiler,
ABI reports, dependencies and runtime receipts are retained with Actions artifacts.
