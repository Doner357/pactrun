---
title: Configure Inputs and Secrets
---

# Configure Inputs and Secrets

Use this procedure for an existing Instance. Replace example names and paths
with identifiers declared by its Revision.
If you do not have one, [install a supplied Pack and create an Instance](./use-pack.md)
first. The empty first-Instance tutorial has no Inputs and its example is retired
at the end; do not assume it provides this page's `demo` or `config`.

## Inspect first

```text
pactrun instance show demo
pactrun input list demo
```

If the Input list is empty, there is nothing to configure here. Choose another
declared capability from the [user task index](./index.md). Otherwise use the
listed IDs; `config` below is only an example.

Record the current state-version token if you need a conditional write. Copy the
complete token; shortened object IDs do not apply to state tokens.

## Set a value

```text
pactrun input set demo config --file ./config.txt --if-version <token>
```

Pactrun acquires a detached copy. The original file remains outside its ownership.
For a command that supports stdin, use `--stdin` instead of `--file`. Supply the
intended bytes, taking care with shell encoding and newline conversion. Do not
place a Secret directly in a command-line argument.

If a conditional write reports a conflict, inspect the Instance again and decide
whether your replacement is still appropriate. Do not automatically reuse a
stale expectation.

## Verify and export

Run `input list demo` and `instance show demo` again. Inspect readiness without
printing Secret payloads. To export a binding to a new destination:

```text
pactrun input export demo config --output ./config-copy.txt
```

Secret export requires `--authorize-secret-export`. Choose a protected destination;
stdout export can disclose bytes to terminal logs or another program.

## Remove a binding

Use `input list demo` to check the binding's role. You can delete an optional
active binding or a retained binding. An active required binding cannot be
deleted once it is bound. In this example, `notes` is an optional Input declared
by the active Revision; replace it with an eligible binding from your Instance.
Copy the current state-version token again before the conditional write.

```text
pactrun input delete demo notes --if-version <token>
```

A required Input can be absent when an Instance is first created; set its value
to satisfy readiness. That initial absence does not permit deletion of a bound
active required Input. After a permitted deletion, reinspect the bindings and
readiness before running an Action. Deletion leaves the original host file intact.

More detail: [User reference](../pactrun-users/reference/index.md).

<details>
<summary>Maintainer sources (optional)</summary>

Contract: [Inputs, Secrets, and readiness](../spec/instances/inputs-secrets.md).

</details>
