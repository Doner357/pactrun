---
title: Installing the First Revision
---

# Installing the First Revision

You have a Pack source in `pack/`, but Pactrun has not installed it yet.
Installation checks the source and saves its management definition and runtime
content as an immutable **Revision** in the default Store. It does not create
an Instance or run a program.

## Install the Pack source

From your `hello-pack-tutorial` working directory, make sure you replaced the
placeholder Package ID in `pack/pactrun.yaml` with the ID you generated. Then run:

```sh
pactrun pack install ./pack
```

Pactrun reads `pack/pactrun.yaml` and validates the source. For example, a
successful first installation looks like this:

```text
Pack installed.

Revision: 0123456789ab:abcdef012345
Installed: 2026-10-17T08:00:00Z
```

The IDs and time are examples; your values will differ. Keep the `Revision:`
reference for the next step. If installation fails, read the
diagnostic and correct the source before continuing; a failed installation has
not given you a usable Revision.

The source directory remains yours to edit. Installing it saves these exact
contents; later edits to `pack/` do not change the installed Revision.

## Find the installed Revision

List the Revisions in your default Store:

```sh
pactrun revision list
```

If this is the only Revision in your Store, the result looks like this:

```text
REVISION                   INSTALLED (UTC)       SUMMARY
0123456789ab:abcdef012345  2026-10-17T08:00:00Z

1 record shown.
```

The list shows the most recently installed Revisions first. If you have used
Pactrun before, it may also show Revisions from other Packages. Find the Hello
Pack Revision by comparing its reference with the `Revision:` value from the
install command. This list shows what is installed; it does not create an
Instance.

Now inspect the Revision you just installed. Replace `REVISION_REFERENCE` with
the reference printed by `pack install`:

```sh
pactrun revision show 'REVISION_REFERENCE'
```

Using the example reference above, the result looks like this:

```text
Revision: 0123456789ab:abcdef012345
Installed: 2026-10-17T08:00:00Z
Core format: 1.0-alpha.2
Identity: 0123456789abcdef0123456789abcdef:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789

No Actions, Inputs or managed capabilities declared.
```

The `Identity:` line shows the full Package ID and content digest. Because
`revision: {}` contains no operations or Inputs, there are no capabilities to
list.

The `revision:` field in `pactrun.yaml` holds the management definition; it is
not a Revision ID. Pactrun identifies the installed Revision by its Package ID
and a digest calculated from its operational contents. `Core format: 1.0-alpha.2`
names the installed Revision's format, while
`source_format: "1.0-alpha.3"` in the YAML names the format used to read your
source. They serve different purposes.

## Install the same contents again

Without changing anything in `pack/`, run:

```sh
pactrun pack install ./pack
```

The output should now begin with `Revision already installed.` and show the
same Revision reference and installation time:

```text
Revision already installed.

Revision: 0123456789ab:abcdef012345
Installed: 2026-10-17T08:00:00Z
```

Repeating installation does not create a new Revision when the
operational contents are unchanged.

You now have a valid installed Pack, but it has no Action to invoke. Next, you
will add the first operation and its implementation before creating an Instance.
