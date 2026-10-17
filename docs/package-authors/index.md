---
title: Pack authors
---

# Pack authors

## From maintaining a service to helping others maintain it

Imagine you maintain a [Gitea](#gitea) server for a small team. You have a
[Docker Compose](#docker-compose) file that describes how to run it, and you know
the commands to start and stop it. For day-to-day work, that is enough.

Before changing the setup, you want to save the service data so you can restore
it if needed. Starting and stopping [containers](#docker-and-containers) does not describe that whole
procedure: you still need to decide what to save and follow the service's rules
for producing a usable copy. You write a [script](#scripts). Later, you add scripts
to restore the data, check the environment, and prepare an update.

Now maintaining the service means knowing several entry points. One task uses a
Compose command; another runs a script with its own arguments. Some scripts
require the service to be stopped first. Others expect files in a particular
working directory. You know these details because you wrote the procedures.

Configuration has spread out too. Some values are in [`compose.yml`](#compose-file),
others are in [`.env`](#env-file), and others belong in Gitea's own configuration
file. A colleague taking over must find each setting, understand which values
belong together, and know what to run after changing them. Your notes help, but
they reflect your own names, organization, and assumptions. The next service may
have a completely different set of scripts and instructions.

The difficulty becomes more apparent when something changes. A script fails
halfway through: what has it already changed, and is it safe to run again? You
publish an improved setup: how should someone using the previous version adopt
it without losing their configuration or service data?

**A Pack author turns these service-specific procedures into a defined interface
that other people can use and that the author can continue to maintain.** A Pack
describes the settings people supply, the operations they can request, and the
programs that carry out those operations. Pactrun provides a common way to use
that definition and inspect execution results.

This does not make Compose, scripts, or service configuration files unnecessary.
The author connects them. Users should not have to discover every implementation
detail just to perform a supported task, but they still need to understand its
requirements and consequences.

## Where your work fits

![The author supplies a Pack; the user supplies settings and selects an operation. Pactrun runs the Pack's programs, which use existing tools to operate the service.](/img/pack-authors/roles.svg)

- **You, the Pack author, define and maintain the integration.** You choose the
  supported environment, describe the settings and operations, and supply their
  implementations. A program Pactrun calls to perform this work is a **Hook**.
- **The user chooses what to do.** They provide the required settings, meet the
  documented prerequisites, and decide whether to proceed with operations that
  affect their environment or data.
- **Pactrun checks declarations and coordinates execution according to its
  contracts.** It manages installed definitions and their configured uses,
  called **Instances**, and records execution results. It does not infer how to
  maintain a service from its name.
- **Existing tools still do their jobs.** A Hook might call Compose or a
  service-provided utility. The service itself implements its application
  features, behavior, and data formats.

The diagram shows an operation being requested, not a continuously running
monitor. A successful Hook also does not automatically prove that the service is
healthy: its implementation needs to check whatever result the operation claims
to deliver.

## What you are responsible for

Your responsibility is the behavior your Pack promises, including what happens
to people already using it.

**Set a clear scope.** Say which environments and deployment arrangements you
support and what must already be available. An undeclared dependency that
happens to exist on your computer is not a reliable prerequisite for someone
else's installation.

**Know what each setting and piece of data belongs to.** Distinguish
user-supplied configuration, installed programs, changing service data, and
temporary working files. Decide where each belongs, what operations may change
it, and what must survive an update or removal. Avoid exposing credentials in
command output or diagnostics.

**Make operation outcomes meaningful.** Explain the intended changes, check
necessary conditions when the operation runs, and verify its result. Decide
what a second execution does. If an operation cannot safely be repeated, make
that limitation clear rather than leaving users to discover it after failure.

**Account for incomplete work.** A failed command may already have changed
external files or service state. Provide enough information to distinguish a
refused operation from one that started and failed. Define what can be retried
and when the user needs to inspect or repair the service first.

**Maintain a path for existing users.** Editing your source files does not
update an existing Instance. Pactrun's [Migration](./reference/service-fields.md)
mechanism lets an author define how an Instance moves from a supported installed definition to a target
definition, including what happens to its configuration and declared resources.
Consider that path whenever you change a delivered Pack. Test it against an
existing setup, not only a fresh installation.

A Pack Migration and a service upgrade are related but distinct. If a change
also upgrades the service, follow that software's supported upgrade procedure.
Declaring a Migration
does not make an arbitrary application upgrade safe or guarantee that external
changes can be rolled back.

**Back your promises with evidence and instructions.** Test the supported
operations, relevant failure cases, and version transitions. Tell users what
they need, what will change, how to recognize success, and what to do when the
operation cannot finish. You do not need to support every possible capability;
you do need to understand and communicate the limits of what you deliver.

## What you do not need to reimplement

Writing a Pack does not mean reimplementing the service it manages. Prefer the
software's supported tools and maintenance procedures over recreating its
internal behavior. For example, integrate its supported upgrade process rather
than inventing a replacement for its database conversion logic.

A Pack also does not automatically take responsibility for every aspect of host
administration, monitoring, backup, or high availability. Those responsibilities
depend on its stated scope and the surrounding environment.

However, a dependency of a promised operation is part of that operation's
contract. If a supported procedure requires the service to be stopped, either arrange
that safely or make it an explicit prerequisite and check it. Leaving necessary
coordination to an undocumented assumption is not a smaller support scope.

Hooks execute trusted host programs. Pactrun's declarations do not provide an
operating-system sandbox, so a Pack must be reviewed with the access and effects
of its programs in mind.

## Before you begin this tutorial

These are the foundations for following the tutorial, **not a complete
qualification for independently maintaining a production Pack**. The service
knowledge and programming ability needed for a real Pack depend on the software,
operations, and environments it supports.

You should be able to:

- Run commands in a terminal and work with current directories, relative and
  absolute paths, files, environment variables, and basic permissions.
- Read and make small changes to simple programs, following arguments,
  conditions, exit statuses, and error messages.
- Consult a program's documentation to check prerequisites and understand a
  command before allowing it to change the system.
- Use basic version control to save work and compare changes.

You do not need to know Pactrun's declaration format, Hook protocol, or Migration
mechanism beforehand. You also do not need to have mastered every aspect of
service administration, complex recovery design, or an entire service-reliability
framework. This is a starting point for learning, not a claim that those
subjects are unimportant.

An agent can help draft programs, explain documentation, and investigate failures.
Its output still needs to be checked and tested. Whether or not you work with an
agent, you remain responsible for understanding the Pack's support boundaries
and the effects of the operations you deliver.

## Terms used on this page

### Gitea {#gitea}

A service for storing code, tracking changes, and collaborating with other
people. In the story, Gitea is the software being maintained, not the tool that
organizes its maintenance procedures.

Read more: [What is Gitea?](https://docs.gitea.com/)

### Docker and containers {#docker-and-containers}

Docker is a tool for running applications in containers. A container runs an
application in a separate environment with the files it needs. Containers still
use the host computer; they are not separate machines.
In the story, containers are the way the maintainer runs the service.

Read more: [What is a container?](https://docs.docker.com/get-started/docker-concepts/the-basics/what-is-a-container/)

### Docker Compose {#docker-compose}

A tool for describing an application's containers and how they work together in
a configuration file, then managing them with commands. The maintainer uses it
to start and stop the service instead of setting up each container by hand.

Read more: [Docker Compose overview](https://docs.docker.com/compose/)

### Scripts {#scripts}

Programs saved in files so that a sequence of steps can be run again. A shell
script contains commands for the shell, the program that interprets commands in
a terminal. In the story, the maintainer writes scripts to coordinate work that
is not covered by a single existing command.

Read more: [Shell scripts in the Bash manual](https://www.gnu.org/s/bash/manual/html_node/Shell-Scripts.html)

### compose.yml {#compose-file}

A filename used for a Compose configuration. This file describes the services
to run and settings such as their network connections and data storage. In the
story, it is one of several places the maintainer must check when changing the
setup.

Read more: [Compose file reference](https://docs.docker.com/reference/compose-file/)

### .env {#env-file}

A text file containing named values, such as `SERVICE_PORT=3000`. Compose can use
these values to fill in parts of its configuration. Putting a value in this
file does not, by itself, pass it into the running application; the configuration
must arrange that where needed. In the story, this is another place where
settings are kept.

Read more: [Using .env values in Compose](https://docs.docker.com/compose/how-tos/environment-variables/variable-interpolation/)
