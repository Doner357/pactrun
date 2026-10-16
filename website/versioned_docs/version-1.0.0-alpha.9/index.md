---
title: Pactrun
slug: /
---

# Pactrun

[Source code](https://github.com/Pipapoen/pactrun) ·
[Preview releases](https://github.com/Pipapoen/pactrun/releases) ·
[Installation guide](./guides/installation.md)

Pactrun is a command-line tool for managing local services on Windows and Linux.
It brings together the instructions for operating a service, the configuration
for each setup, and records of the operations performed.

Service-specific instructions are supplied in a **Pack**: a versioned definition
of the settings a service needs and the operations available to its users. You
can use a Pack prepared by someone else, or write one for your own service.
Pactrun itself is a single executable; the service and any tools required by its
Pack remain separate.

![Pack authors define the service-specific procedures; users provide settings and choose operations. Pactrun manages their execution, while the service owns its application behavior and live data.](/img/intro-alpha9/responsibilities.svg)

## Why Pactrun?

A service's own commands may be enough to install and start it. Continued use
raises other questions: which settings belong to this setup, which version of
the operating instructions it uses, what happened during an earlier operation,
and what should be retained when it changes.

These concerns recur across services, even though their actual operating
procedures differ. Pactrun provides a common way to manage them:

- **Separate named setups.** An **Instance** represents one managed use of a
  Pack, with its own configuration and execution history. Multiple Instances
  can use the same Pack without sharing those settings.
- **Known instruction versions.** An installed Pack version is a **Revision**.
  It is immutable, and installing another Revision does not automatically
  switch existing Instances to it.
- **Defined operations and recorded results.** Packs expose operations with
  declared requirements. Managed executions have records that can be inspected
  for outcomes and available diagnostics.
- **Support for continued maintenance.** Packs can define backup, restore,
  migration and cleanup capabilities, subject to the service's requirements.

This lets authors concentrate on service-specific procedures rather than
rebuilding the same configuration and execution-record management for every
collection of scripts. Users get a consistent interface without having to
write those scripts themselves.

## Who does what?

Pactrun does not contain built-in knowledge of every service. That knowledge
comes from the Pack, while the service remains responsible for its application
behavior and live data.

- **The Pack author** defines settings, operations and the programs or scripts
  that implement them. Those executable steps are called **Hooks**.
- **The user** selects a trusted Pack, provides configuration and decides which
  operations to perform.
- **Pactrun** manages installed Revisions and Instances, coordinates executions
  and records their results.
- **The service** provides its actual functionality and manages its live data.
  Its consistency and upgrade requirements still apply.

For example, having a backup capability does not establish that a Pack can
capture every service safely while it is running. The Pack must implement an
appropriate procedure and describe its supported scope. Likewise, a recorded
failure does not automatically undo changes to the service, and an older
version is not automatically a safe data rollback.

Hooks execute code on the host. Separate Instances are not operating-system
sandboxes: ports, file locations and permissions still need suitable
configuration. Pactrun is not a security sandbox or a general-purpose secret
vault.

## Where it fits

Several kinds of tools already make services easier to operate. The useful
comparison is what each tool manages and what its users or authors must provide,
not whether it can run a command named "backup" or "upgrade".

### Command interfaces and task runners

[Task](https://taskfile.dev/docs/guide/defining-tasks), which uses `Taskfile.yml`,
already provides a customizable command interface: named tasks, descriptions,
variables, aliases and calls between tasks. A Taskfile can wrap service startup,
backup or upgrade commands. Those capabilities are not unique to Pactrun.

**Why use a Pack rather than a Taskfile?** Pactrun also defines the persistent
objects and lifecycle rules around those commands:

- **An Instance continues to exist between operations.** Its identity,
  configuration and execution history belong to a managed object, rather than
  being a naming and storage convention that the task author must establish.
- **Each Instance uses an exact installed Revision.** A Taskfile can be versioned
  in source control too; Pactrun additionally binds the Instance to immutable
  installed instructions. Installing newer instructions does not silently
  change the ones used by an existing Instance.
- **Maintenance operations follow shared contracts.** Pactrun defines how
  Snapshots, Migrations and Cleanup relate to managed objects. A Pack supplies
  the service-specific implementation; it does not have to invent the entire
  surrounding management model.

This is also a cost. Pack authors must work within Pactrun's definitions and
protocols. If you only need a convenient, parameterized interface to existing
commands, a Taskfile or ordinary scripts may be simpler. Neither approach makes
an arbitrary service backup or upgrade correct without suitable implementation.

### Service managers and container tools

These tools primarily handle how the service runs. They can be used by a Pack
rather than replaced by Pactrun.

| Existing tool | Its responsibility | Pactrun's role |
| --- | --- | --- |
| **Service managers, such as systemd** | Start and stop services, manage startup dependencies and supervise processes. | Manage Pack-defined configuration and maintenance operations. Ongoing process supervision stays with the service manager. |
| **Docker / Compose** | Run containers and describe services, networks and volumes, with their own lifecycle and logging tools. | Expose service-specific operations and their records through Packs. The same management model can also be used for native services. |

A Pack can combine tools: for example, a service's own backup command and a
service manager for stopping or restarting it. Another Pack might invoke Docker
Compose. The integration belongs to the Pack; Docker and WSL are not universal
prerequisites. Pactrun itself is not an always-on process supervisor.

![Pactrun manages Pack versions, configuration and execution history. Pack Hooks can combine service commands, service managers and container tools; each tool retains its own runtime responsibilities.](/img/intro-alpha9/tooling.svg)

### Application management platforms

Platforms such as YunoHost, Cloudron and Coolify already provide application
management beyond launching commands. Lifecycle management is not, by itself,
a reason to prefer Pactrun. The differences are the operating environment,
packaging model and scope of integration.

| System | Operating environment and application definition | What it manages |
| --- | --- | --- |
| **[YunoHost](https://doc.yunohost.org/admin/what_is_yunohost/)** | A Debian-based self-hosting platform with [application packages](https://doc.yunohost.org/en/dev/packaging/). | Package-defined installation and maintenance, including upgrades and backup/restore, integrated with web administration, users, domains and certificates. |
| **[Cloudron](https://docs.cloudron.io/)** | A platform installed on a [fresh Ubuntu server](https://docs.cloudron.io/installation/), with apps distributed through its App Store and packaging system. | App installation and updates, per-app backup/restore, centralized users, and integration with databases, DNS and certificates. |
| **[Coolify](https://coolify.io/docs/core/what-is-coolify)** | A deployment platform for Docker workloads on connected servers, using application source, images or Compose definitions. | Builds and deployments, resource configuration, logs, domains and HTTPS through a dashboard or API. |
| **Pactrun** | A standalone command-line tool on Windows and Linux. Packs define operations for native services or other tools chosen by the author. | Installed Revisions, Instances, configuration and managed execution records, with Snapshot, Migration and Cleanup mechanisms where the Pack implements them. |

**Choose an integrated platform when you want its wider management environment.**
For example, YunoHost or Cloudron can provide application maintenance together
with shared user and domain management; Coolify provides a container-deployment
workflow and dashboard. Those integrations are part of what you adopt, not
features Pactrun automatically supplies.

**Consider Pactrun when you want Pack-defined operations without adopting that
whole environment.** Its focus is local, command-driven management of persistent
Instances. It does not provide a hosting control panel, remote orchestration or
a scheduler. Host administration and service-specific integration remain your
responsibility or the Pack author's responsibility.

If an existing task runner, service manager, Compose setup or hosting platform
already covers your needs, using it directly may be the better choice.

## Install

### Windows · Scoop

Requires **Windows x86-64**, Git and [Scoop](https://scoop.sh/).

```powershell
scoop bucket add pactrun https://github.com/Pipapoen/pactrun
scoop install pactrun/pactrun-preview
pactrun --version
```

### Linux · Homebrew

Requires **Linux x86-64 with glibc 2.36 or newer**, Git and
[Homebrew](https://brew.sh/).

```sh
brew tap pipapoen/pactrun https://github.com/Pipapoen/pactrun
brew install pipapoen/pactrun/pactrun-preview
pactrun --version
```

:::warning Preview only

Only **Preview 1.0.0-alpha.9** is available; there is no Stable release yet.
The executables are unsigned. Use isolated data while evaluating this release.

:::

## Use a Pack

Using a Pack and writing one are different roles. A user needs a trusted Pack
and an environment that meets its requirements, not knowledge of Pactrun's
authoring format. The Pack's documentation describes its configuration, external
tools and supported operations. Operation names and capabilities vary between
Packs; there is no universal `start` or `backup` operation.

Installing a Pack makes its definition and files available to Pactrun. An
Instance associates a selected Revision with configuration for a particular
use. Creating an Instance may allocate declared storage, but does not
automatically start the service. Actual service setup and startup depend on the
Pack's procedures.

The [user guides](./guides/index.md) cover the operating tasks. The
[Pack author guide](./package-authors/index.md) covers creating and distributing
those procedures for other people to use.

## Keep exploring

| What you need | Where to look |
| --- | --- |
| Use a supplied Pack | [User guides](./guides/index.md) |
| Create or maintain a Pack | [Pack author guide](./package-authors/index.md) |
| Find a command, option or output field | [User reference](./pactrun-users/reference/index.md) |
| Read exact behavior and formats | [Specification](./spec/index.md) |

The Specification is a reference for formal rules, not a prerequisite for
following the user or author guides.

## Contributing & license

Pactrun is written in Rust and is [MIT licensed](https://github.com/Pipapoen/pactrun/blob/main/LICENSE).
See [Contributing](https://github.com/Pipapoen/pactrun/blob/main/CONTRIBUTING.md)
for development setup and verification. Distributed dependencies retain their
own licenses.
