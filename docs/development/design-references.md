---
title: Design References
---

# Design References

**Status: Informative.** These sources informed particular design choices but do
not define Pactrun behavior.

| Pactrun design area | Reference | Point of comparison |
| --- | --- | --- |
| Human CLI output and help | Command Line Interface Guidelines (clig.dev), Human-first design, Saying (just) enough, Output, Errors and Help | Concise task-oriented output, clear channels, discoverable help and actionable errors. |
| Human error wording | GOV.UK Design System error messages | State the problem and the corrective action directly. |
| Stable machine output | Git status porcelain | Machine contracts remain independent of human layout and display settings. |
| Complete object inspection | Docker inspect | Return useful structured object facts in a single query, including batched queries. |
| Execution event delivery | Terraform machine-readable UI and JSON Lines | Versioned, individually parseable records support incremental consumers. |
| Contributor branch workflow | nvie Git Flow | Production versus development/release repair paths; local topic prefixes communicate intent without changing that topology. |
| Formal product compatibility boundary | Conan client stability policy and Semantic Versioning | Same-Major backward compatibility; a new Major does not promise previous-Major external-format support. Pactrun's own policy controls the exact obligation. |
| Stable Instance identity separate from name | Kubernetes object IDs and owner references | Human name and object-lifetime identity are distinct. |
| Candidate Instance ID encoding | RFC 9562 UUIDv7 | A globally unique, time-ordered implementation option. |
| Durable pins and GC roots | containerd leases and Nix GC roots | Strong lifetime references protect reachable content. |
| Plan validation | Nomad plan and check index | Execution rechecks state after a dry run. |
| Materialized recovery instead of Plan replay | Temporal event history, as a contrast | Pactrun deliberately does not replay Hooks. |
| Crash-consistent recovery boundary | SQLite WAL principles | Recovery information and committed state remain consistent. |
| Manual recovery trust guard | Terraform taint and untaint | Management state can record uncertainty about external state. |
| Lifecycle obligation | Kubernetes finalizers | Completion can be guarded, without adopting controller semantics. |
| Canonical JSON | RFC 8785 JCS | Stable JSON bytes support hashing. |
| Serializer is not identity | Protocol Buffers canonicalization guidance | Deterministic serialization is not automatically canonical. |
| Digest and human reference separation | OCI Distribution, Git tags, and Git notes | Immutable identity is separate from labels and annotations. |
| Incomplete managed object | Kubernetes readiness concepts | Existence and operational readiness are different. |
| Stable Input identity | Protocol Buffers field rules | Stable identifiers are not reused and unknown state can survive. |
| Explicit continuity | Terraform `moved` | Renames do not silently imply semantic continuity. |
| Single target writer | Kubernetes Server-Side Apply | Conflicts fail rather than use hidden precedence. |
| Cleanup versus abandon | Kubernetes finalizers and Terraform state removal | Managed cleanup differs from explicit stop-managing intent. |
| Session authority versus sandbox | Flatpak and Wasmtime security models | Host permissions are guarantees only with an enforcing backend. |

## Sources

- [Docker: container identification](https://docs.docker.com/engine/containers/run/#container-identification)

Docker's full IDs, short IDs and separate names inform the CLI object-selector
usability goal. Pactrun defines its own eight-digit minimum, twelve-digit display
floor, namespace-wide collision checks and exact state-token policy in the
[selector contract](../spec/contracts/cli-id-selectors.md).

- [Command Line Interface Guidelines](https://clig.dev/)
- [GOV.UK: Error message](https://design-system.service.gov.uk/components/error-message/)
- [Git: status porcelain](https://git-scm.com/docs/git-status#_porcelain_format_version_1)
- [Docker: inspect](https://docs.docker.com/reference/cli/docker/inspect/)
- [Terraform: machine-readable UI](https://developer.hashicorp.com/terraform/internals/machine-readable-ui)
- [JSON Lines](https://jsonlines.org/)

The `pactrun.cli.events.v1` event vocabulary, 64-KiB byte chunks, Base64 encoding,
ephemeral delivery spool and explicit cancel-on-output-close option are Pactrun
decisions. They preserve arbitrary terminal bytes, bounded memory and existing
execution ownership while supporting one-shot and incremental consumers. The
CLI sources inform presentation; Pactrun's owning contracts define its behavior.

- [nvie: A successful Git branching model](https://nvie.com/posts/a-successful-git-branching-model/)

- [Conan client stability](https://docs.conan.io/2/introduction.html#stability)
- [Semantic Versioning](https://semver.org/)

- [Kubernetes: Object Names and IDs](https://kubernetes.io/docs/concepts/overview/working-with-objects/names/)
- [Kubernetes: Owners and Dependents](https://kubernetes.io/docs/concepts/overview/working-with-objects/owners-dependents/)
- [Kubernetes: Finalizers](https://kubernetes.io/docs/concepts/overview/working-with-objects/finalizers/)
- [RFC 9562: Universally Unique IDentifiers](https://www.rfc-editor.org/info/rfc9562/)
- [containerd: Garbage Collection](https://github.com/containerd/containerd/blob/main/docs/garbage-collection.md)
- [Nix: Garbage Collector Roots](https://nix.dev/manual/nix/2.35/package-management/garbage-collector-roots.html)
- [Nomad: Create and Submit a Job](https://developer.hashicorp.com/nomad/docs/job-declare/create-job)
- [Temporal: Events and Event History](https://docs.temporal.io/workflow-execution/event)
- [Temporal: Workflows](https://docs.temporal.io/workflows)
- [SQLite: Write-Ahead Logging](https://www.sqlite.org/wal.html)
- [Terraform: Provisioners](https://developer.hashicorp.com/terraform/language/provisioners)
- [Terraform: `untaint`](https://developer.hashicorp.com/terraform/cli/commands/untaint)
- [RFC 8785: JSON Canonicalization Scheme](https://www.rfc-editor.org/info/rfc8785/)
- [Protocol Buffers: Serialization Is Not Canonical](https://protobuf.dev/programming-guides/serialization-not-canonical/)
- [Git: `git-tag`](https://git-scm.com/docs/git-tag)
- [Git: `git-notes`](https://git-scm.com/docs/git-notes)
- [OCI Distribution Specification](https://github.com/opencontainers/distribution-spec/blob/main/spec.md)
- [OCI Image Annotations](https://github.com/opencontainers/image-spec/blob/main/annotations.md)
- [Protocol Buffers: Proto3 Language Guide](https://protobuf.dev/programming-guides/proto3/)
- [Terraform: `moved`](https://developer.hashicorp.com/terraform/language/block/moved)
- [Terraform: Remove Resources from State](https://developer.hashicorp.com/terraform/language/state/remove)
- [Kubernetes: Server-Side Apply](https://kubernetes.io/docs/reference/using-api/server-side-apply/)
- [Flatpak: Basic Concepts](https://docs.flatpak.org/en/latest/basic-concepts.html)
- [Wasmtime: Security](https://docs.wasmtime.dev/security.html)
