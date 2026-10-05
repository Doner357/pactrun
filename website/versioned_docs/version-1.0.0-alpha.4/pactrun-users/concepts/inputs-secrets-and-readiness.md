---
title: Inputs, Secrets, and readiness
---

# Inputs, Secrets, and readiness

An Input Binding is a detached, Pactrun-managed value for one Instance. When you
supply a file, later changes to the original file do not update that binding.
Use an explicit Input operation to replace it.

## Required and optional Inputs

The Revision declares which Inputs are required and their protection. Missing
required values make configuration incomplete. Inspect `instance show` and
`input list` before invoking an operation. An Action can have additional requirements.

Readiness concerns configuration and operation eligibility. It does not certify
that an external database, daemon, or network service is healthy.

## Secrets

Secret protection affects disclosure and operation rules. Keep Secret values out
of command-line arguments, source control, terminal recordings, and diagnostics.
Use file or stdin acquisition as supported by the command. Protect those host
files and the managed store with appropriate OS permissions.

Secret handling does not provide an encrypted vault or a sandbox for trusted
Hooks. Snapshot bundles, exported Inputs, and Run Artifacts can contain sensitive
bytes; explicit authorization does not make the destination safe.

## Inputs and live service data

Use Inputs for configuration values. ServiceStorage holds service-authoritative
live state, such as database files. Workspace is execution scratch. These areas
are not implicitly synchronized.

Follow [Configure Inputs and Secrets](../../guides/configure-inputs.md).

More detail: [User reference](../reference/index.md).

<details>
<summary>Maintainer sources (optional)</summary>

Contracts: [Input behavior](../../spec/instances/inputs-secrets.md)
and [resource ownership](../../spec/lifecycle/resource-lifetimes.md).

</details>
