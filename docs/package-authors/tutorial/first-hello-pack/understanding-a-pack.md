---
title: Understanding a Pack
---

# Understanding a Pack

Before writing Hello Pack's first definition, we need to know what Pactrun
will recognize: a Package with a lasting identity, a Pack that describes how
to manage something, and Revisions that preserve its exact contents.

## A Package has a continuing identity

A **Package** represents a product that you maintain over time. Its
`package_id` identifies that product throughout its development.

This continuing identity is called the **Package lineage**. Hello Pack can
start with an empty definition, gain functionality, and receive fixes while
remaining the same Package. Keep its Package ID when making those changes.

The identity also survives moving the source directory, changing the display
name, or sharing the Pack with someone else. If a fork becomes a separately
maintained product lineage, give it a new Package ID.

## A Pack is a kind of Package

**Pack** is the only Package kind Pactrun currently supports. When you create
Hello Pack, you are creating a Package of this kind.

A Pack supplies management definitions and the files needed to carry them out,
such as scripts. Those declarations tell Pactrun how to use the supplied
implementation.

![Pack is the supported Package kind. A Pack supplies management definitions and accompanying runtime files.](/img/pack-authors/pack-composition.svg)

## A Revision preserves exact contents

As you maintain a Pack, its contents change. A **Revision** is an immutable
management definition together with its accompanying runtime content. One
Package can have several Revisions.

![Hello Pack keeps one Package ID across three Revisions. Each Revision contains its own management definition and runtime content.](/img/pack-authors/package-lineage.svg)

The letters A, B, and C in the diagram are examples. Pactrun calculates a
**content digest**, a fingerprint of the operational meaning of the definition
and its runtime content. The Package ID and this digest together identify an
exact Revision.

You do not assign a sequence number to a Revision. Changing a script's contents
or the meaning of the management definition changes the digest. Editing YAML
comments or formatting does not. Display names and descriptions also stay
outside this content identity.

Once installed, a Revision keeps its contents. Editing your source files does
not rewrite it. Installing the edited source lets Pactrun validate and identify
the resulting contents.

## How Pactrun recognizes Pack source

The **source** is the collection of files you edit to produce a Revision.
For our tutorial, it lives in the `pack/` directory prepared in Getting Ready.

Pactrun reads a source directory through a file named exactly `pactrun.yaml`
at its root. That file declares the Package ID, management definition, and any
runtime files to include. You can choose the directory's name; the manifest
name is fixed.

```text
pack/
├── pactrun.yaml
│   ├── source_format      Source format to read
│   ├── package_id         Package lineage
│   ├── revision           Management definition
│   └── runtime_content    Runtime file declarations
└── Any files referenced by runtime_content
```

The indented entries under `pactrun.yaml` above are fields inside the file.
Runtime files are added alongside it as needed.

During installation, Pactrun checks that:

- The source format is supported and the document follows its schema.
- The Package ID has the required form.
- Declarations and references are valid and consistent.
- Declared runtime files exist and meet the source path and content rules.

Valid YAML syntax is one part of these checks. Unknown fields, duplicate keys,
and references to missing runtime content are rejected. Passing the checks
means Pactrun can accept the definition; verifying what its programs actually
do remains part of the author's work.

An empty definition is allowed. That gives us a small starting point with a
Package identity and valid structure.

## Create the first definition

Run the following command from your `hello-pack-tutorial` working directory:

```sh
pactrun pack generate-id
```

It prints a new Package ID: 32 lowercase hexadecimal characters. The command
only generates the ID; you will put it in the source yourself. Generate it once
for this new Hello Pack and keep it as the Pack evolves.

Create `pack/pactrun.yaml` in your editor with:

```yaml
source_format: "1.0-alpha.3"
package_id: "<your generated Package ID>"

revision: {}

runtime_content: {}
```

Replace `<your generated Package ID>` with the command's output, keeping the
quotes. Save the file as UTF-8.

| Field | What it establishes |
| --- | --- |
| `source_format` | The rules Pactrun uses to read this source document. |
| `package_id` | The Package lineage this source belongs to. |
| `revision` | The management definition for these contents. |
| `runtime_content` | The runtime files to include with the definition. |

`1.0-alpha.3` is the source format version. Hello Pack's exact Revision identity
will be derived from its Package ID and contents.

In YAML, `{}` means an empty object. Pactrun supplies empty collections for the
omitted declarations here, so `revision: {}` provides an empty management
definition and `runtime_content: {}` declares no runtime files.

With a valid generated ID, this is a complete minimal Pack source. You now have
Hello Pack's identity and the source structure where its implementation will grow.
