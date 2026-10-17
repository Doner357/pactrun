---
title: Getting Ready
---

# Getting Ready

In this chapter, you will build a small Pack that checks its environment,
installs Python into Service Storage, and uses that Python to print a hello
message. Service Storage is where this Pack keeps the Python installation that
its later operations need.

You will start with a working Pack and see how its actions depend on one
another. This gives us a small, concrete example before we build a Pack for
Gitea.

## Prepare your tools

Use a Linux machine with:

- A terminal and a POSIX shell (`sh`), which runs the shell scripts used in this
  chapter.
- A text editor that can save files as UTF-8 with Unix (LF) line endings.
- A directory where your normal user account can create files.
- Internet access for downloading Python during the Pack's setup operation.

[Install Pactrun](../../../index.md#install) if you have not already done so.
In the terminal you will use for the tutorial, run:

```sh
command -v sh
pactrun --version
pactrun --help
```

The first command prints the location of the shell. The other two print
Pactrun's installed version and command help. If a command cannot be found,
finish setting up that tool before continuing.

The Python used by this Pack will be installed by its setup action later in
the chapter.

## Create a working directory

During normal use, Pactrun chooses its default data directory, called a
**Store**, and creates it automatically when a command first needs to write
data, such as installing a Pack. On Linux, this is `$XDG_DATA_HOME/pactrun` when
`XDG_DATA_HOME` is set, or `~/.local/share/pactrun` otherwise.

We will use this default Store for the tutorial. It keeps your installed Pack
definitions, configured Instances, and execution records available between
terminal sessions.

The Pack's source files belong in your own working directory. From a directory
where you keep practice projects, run:

```sh
mkdir hello-pack-tutorial &&
cd hello-pack-tutorial &&
mkdir pack
```

Use a new directory for this exercise. If `hello-pack-tutorial` already exists,
choose another name for the first two lines. The `&&` between commands means
each step runs only if the preceding step succeeds.

`pack/` is where you will write the Pack definition and its scripts. Run
`pwd` to see the full path of your working directory and keep it for your next
session.

When you return to the tutorial, open a terminal and change to that directory:

```sh
cd /path/to/hello-pack-tutorial
```

Replace `/path/to/hello-pack-tutorial` with the path you saved. Pactrun will
continue to use the same default Store. Run the following lessons' commands
from this working directory unless a step tells you to change directories.

Your environment is ready. Next, we will create the Pack's source files in
`pack/`.
