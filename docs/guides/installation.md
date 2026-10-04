---
title: Install Pactrun
---

# Install Pactrun

Install through **Scoop on Windows** or **Homebrew on Linux**. The current product
is `1.0.0-alpha.4`, available through the Preview entry. There is no Stable release
yet: `pactrun` is reserved for Major 1 stable and does not silently install alpha.
The package you install is named `pactrun-preview`; the command you run is `pactrun`.

Pactrun is pre-release software. Use isolated data for evaluation. Preview does
not automatically isolate data from an existing normal installation. Read
[version management](./version-management.md) before replacing an existing package.

## Before you start

- Use an x86-64 Windows or Linux host. Alpha.4 qualification covers Windows 11,
  Debian 12 userland with glibc 2.36 in CI, and the same Linux payload on a native
  Debian 13/glibc 2.41 host. Container tests share a hosted kernel; not every
  kernel, filesystem or service is certified. No macOS, ARM64 or musl build is included.
- Use glibc 2.36 or newer for the supported Linux qualification baseline.
  Alpha.4's highest measured symbol requirement is 2.34, but this is not a claim
  that every glibc 2.34 distribution is supported. Older alpha.1/alpha.2/alpha.3
  Linux downloads retain their glibc 2.39 requirement; update Pactrun rather than
  replacing the system libc or setting a global LD_LIBRARY_PATH.
- Homebrew may still install its own glibc, GCC and related dependencies. That
  does not automatically change which libc an arbitrary prebuilt program loads.
  Alpha.4 is verified directly on system glibc 2.36 as well as through Homebrew.
- Install and verify [Scoop](https://scoop.sh/) or
  [Homebrew](https://brew.sh/) first, following its own prerequisites. Do not run
  these instructions as administrator/root merely to bypass an error.
- Git must be available for source setup. If Scoop reports that Git is missing,
  install it with `scoop install git` before adding the Pactrun bucket.
- The project-owned source is `https://github.com/Doner357/pactrun`. It is not a
  claim of endorsement or inclusion in either manager's official package catalog.
- Packages download versioned Release assets and verify their SHA-256. The alpha
  executables are not represented as platform-signed binaries. Obtain the source
  from the intended owner; a checksum alone does not establish publisher trust.

## Windows: Scoop

In a terminal where Scoop works, add the source once and install:

```powershell
scoop bucket add pactrun https://github.com/Doner357/pactrun
scoop install pactrun/pactrun-preview
pactrun --version
pactrun --help
```

If the bucket already exists, check `scoop bucket list` and continue only when it
points to the intended repository. Do not remove someone else's similarly named
source blindly. `Get-Command pactrun` should resolve to the Scoop installation,
not an old standalone executable or shell alias.

## Linux: Homebrew

In a terminal where Homebrew works, add the source once and install:

```sh
brew tap doner357/pactrun https://github.com/Doner357/pactrun
brew install doner357/pactrun/pactrun-preview
pactrun --version
pactrun --help
```

Use `brew tap-info doner357/pactrun` to inspect an existing source. `command -v
pactrun` should resolve to the Homebrew installation. Keep using the manager's
launcher; do not copy the internal `libexec` executable into another directory.

## Verify and continue

Both version and help commands must succeed. Expect the installed package's
version, currently `1.0.0-alpha.4`. These checks do not require a managed store.
If command resolution, download, checksum or launch fails, stop and inspect the
error; do not delete service data or disable checksum verification to proceed.

No Git branch switch or Pactrun-specific source helper is required. The manager
owns download, extraction, activation and program inventory. These steps do not
create an Instance, choose a service data location, or migrate existing data.

- **New user:** [Create your first Instance](../introduction.md), using its isolated store.
- **Pack author:** [Prepare an authoring workspace](../package-authors/setup.md).
- **Real service data:** [Choose a data location](./data-location.md), then
  [install a supplied Pack](./use-pack.md).
- **Updates and channel changes:** [Manage versions](./version-management.md).
- **Offline/engineering fallback:** [Install a supplied standalone archive](./standalone-installation.md).

<details>
<summary>Maintainer sources (optional)</summary>

[Public delivery evidence and platform limits](../development/public-preview-delivery.md).

</details>
