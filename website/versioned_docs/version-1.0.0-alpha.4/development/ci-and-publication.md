---
title: CI and publication operations
---

# CI and publication operations

**Status: Operational CI/Pages procedure.** Actual activation and qualification
results belong to [Public Preview delivery](./public-preview-delivery.md). This
procedure is not a product contract or an independent certification claim.

## One project, distinct responsibilities

Source, documentation, package definitions and delivery tooling belong to the
same project. Develop changes through topic branches into `develop`; record
reviewed release results on `main` under the existing Git Flow policy. A recorded
alpha release is still an alpha, not a stable product. Git branches are not a
second package database. Pages uses an Actions artifact, not a generated HTML
branch committed into source history.

CI verifies a revision. Pages publishes its documentation. A software release
publishes explicitly qualified executable artifacts. Passing CI is not permission
to create a tag, upload binaries, change package eligibility or announce a release.
Documentation corrections do not require a new executable version.

## Verification workflow

`.github/workflows/ci.yml` runs for pull requests and pushes to `develop`/`main`.
The scope job compares the complete Git change, including deleted paths, rather
than relying on a truncated API or path-filter list. Unknown inputs, unavailable
history and initial pushes use the full scope.

- Changes confined to website files, non-Spec Markdown under docs, README and
  CONTRIBUTING run documentation tests, traceability/link checks, TypeScript and
  the strict production build. This scope does not certify runtime semantics.
- Spec changes remain conservative because Markdown can contain DDL, schemas and
  normative fixtures. Runtime, tooling, workflow and other changes retain Ubuntu
  `cargo xtask ci` and Windows `cargo xtask rust-ci`.
- Pinned, checksum-verified actionlint checks workflow syntax and expressions;
  ShellCheck integration is not enabled. Scope policy, the actual aggregate gate
  and the inline Pages freshness guard have focused executable tests.
- Runtime scope also requires the shared [Debian 12 artifact gate](./linux-compatibility-ci.md):
  one Bookworm build with launcher/payload ABI checks, a clean non-root runtime
  without Homebrew, and an independent ordinary-user Homebrew installation.
  Documentation-only scope must explicitly skip this gate; runtime failure or
  cancellation cannot satisfy the aggregate check.
- The aggregate **CI gate** requires all applicable jobs to pass. A skipped
  Windows job is accepted only when documentation-only scope was proved. Failed,
  cancelled or missing verification does not authorize deployment.

CI uses test opt-level 1 without debug symbols, with debug assertions and overflow
checks explicitly enabled; it does not shorten timeouts or remove cases. Omitting
symbols reduces the real executable fixtures copied and hashed by Hook tests.
The 2026-09-30 representative
CLI/Hook fixture took about 40 seconds unoptimized and 10 seconds at opt-level 1
on the qualified remote (excluding compilation). Release builds are unaffected.
Runtime-scope CI also regenerates checked-in package definitions, rejects drift
or non-public asset URLs, and compares previous published release identities.

Linux libtest cases are scheduled serially: unrelated fork/exec activity can
temporarily inherit another test's open-file-description lock even with CLOEXEC.
The uncontended retirement-lifetime proof also runs in its own process and checks
that exactly one proof executed. Explicit lock-contention and concurrent-operation
tests still create their own competing threads/processes and retain their original
assertions. No production lock behavior or timeout was changed to obtain a pass.

Configure branch protection/rulesets to require **CI gate**, not the conditional
Windows job. Changing repository protection requires a separately authorized
administrative action. This local workflow change does not configure it.
Formal releases still require the complete source-qualified release gate even
when their last commit only updates documentation or generated package metadata.
That release gate is not replaced by the path classifier.

## Pages artifact and trust boundary

Pages is disabled unless the repository variable `PACTRUN_PAGES_ENABLED` is the
exact string `true`. Only a push to `main` can prepare and publish its artifact.
PRs, `develop` and tags do not publish through this path. The reusable deployment
workflow has no independent push/manual trigger; CI is its configured caller.
No `pull_request_target` or privileged `workflow_run` handoff is used.

When enabled, the Ubuntu job reads the existing Pages configuration with
`enablement: false`; it does not enable Pages itself. The returned origin and base
path configure Docusaurus for either a project subpath or the configured root.
The website built during verification is uploaded without another site build.
When publication is disabled, builds use `https://example.invalid` and `/`; those
outputs are verification-only and are not uploaded as a deployment artifact.

After CI gate succeeds, the reusable `.github/workflows/pages.yml` deploys the
same run's Pages artifact. Its job has Pages-write and OIDC permissions, but it
does not check out the repository, install dependencies or execute project builds.
Build jobs have no Pages-write/OIDC permission; checkouts do not persist Git
credentials. The deploy job uses the protected `github-pages` environment.

The caller explicitly checks the successful aggregate gate and overrides implicit
ancestor-success handling. Otherwise an intentionally skipped Windows job in a
documentation-only run can suppress deployment despite a green gate. Cancellation,
failed gates, non-main events and a disabled publication flag remain refusals.
After deployment, compare the public `agent-docs/publication.json` source digest
with the intended documentation sources; a green workflow alone does not prove
that the latest document edition reached the public URL.

Production deployments are serialized. Immediately before deploying, the job
checks that its SHA is still the current remote `main` SHA; stale runs are skipped
and an API failure stops deployment. Newer code may still reach main while a
deployment is in progress: it will be verified/deployed separately, not injected
into the older artifact. Old runs cannot later roll back a newer completed
deployment through this path. Main CI runs are not automatically interrupted by
new pushes; non-main runs may be superseded.

## First activation checklist

Do not run this checklist merely because the files exist. Obtain explicit approval
for Git integration/push and the first public website activation.

1. Review and integrate the workflow changes through the project Git process.
2. Confirm the intended repository and public audience. Review the generated site
   for material unsuitable for public distribution.
3. Set repository Pages source to **GitHub Actions**. Configure `github-pages`
   environment deployment branch rules to allow only `main`; use a reviewer if
   publication requires manual approval. Do not weaken protection to make a run pass.
4. Set repository Actions variable `PACTRUN_PAGES_ENABLED=true` only when ready.
5. Run CI through a new approved main push. If rerunning an existing main push,
   rerun the whole workflow so the site is rebuilt with the configured destination;
   rerunning only the deploy job does not create a missing artifact.
6. Verify CI gate, upload and deployment success on GitHub, then inspect the actual
   public URL: entry pages, CSS/JS, search, command reference, agent text links and
   project-base-path navigation. Record SHA, workflow run, deployment URL and results.

Setting the variable false prevents subsequent eligible publication jobs but is
not an emergency cancellation of an already running deployment, and does not take
an existing site offline. For an incident, explicitly cancel the running deployment
and use GitHub's Pages controls as authorized. For content rollback, revert through
the normal project process and publish a newly verified main commit; do not rerun
an obsolete deployment against the freshness guard. The prior successful site is
not intentionally removed when verification fails; inspect actual deployment
status before claiming the public site is healthy.

## Software delivery is a separate gate

The delivery uses a Stable `pactrun` entry and a Preview `pactrun-preview` entry in
the project's ordinary version history. Stable is absent until a formal release
is eligible. Native installation/update does not require a Git branch switch;
the old source-ref helper is not shipped. The legacy publisher remains only an
engineering fixture. See the [public delivery evidence](./public-preview-delivery.md),
[historical native delivery evidence](./native-package-delivery.md) and
[release readiness gates](./release-readiness.md).

Before a later executable delivery, requalify affected package definitions,
Major/exact selection, command conflicts, failed-switch recovery and data-preservation
cases. Keep binary source SHA, tested archive hashes and the
subsequent metadata commit linked in a delivery receipt: inserting archive hashes
into package definitions does not justify silently rebuilding qualified binaries.
Real releases require full candidate/artifact qualification, minimum-environment
and trust/license review, and explicit publication authorization. Neither workflow
automatically uploads executable releases or updates package sources. The manually
dispatched **Release candidate** workflow builds source-qualified artifacts on
Windows 2025 and a pinned Debian 12 Bookworm container with Rust 1.98.1; the Linux
leg uses the same build-and-runtime workflow as normal CI. Its read-only token
cannot publish artifacts. Choose the binary source SHA from the delivery receipt, not a later metadata
commit, when reproducing that source. Candidate artifacts are review inputs, not
permission to overwrite an already published version with a different build.
Archived alpha.3 used its original Ubuntu 24.04 build recipe; reproduce it with
the historical workflow revision, not by relabeling a new Bookworm build.

For pre-main qualification, manually dispatch the workflow from
`release/<version>` and supply that exact dispatch-head SHA and Cargo version.
The source guard rejects a mismatched version branch, a different source commit,
and other dispatch branches. Main dispatch retains the existing main-ancestry
check. This permits artifact acceptance before main integration without granting
the build token publication permissions or bypassing protected merge checks.

## Validation limits

Local unit tests, workflow lint and remote documentation builds are preflight
evidence. They cannot prove GitHub repository settings, environment protection,
OIDC, artifact handoff or production Pages reachability. Mark hosted execution
**Not run** until an authorized actual run provides those results. Do not report
this workflow-only change as a fresh full Rust CI pass or a public deployment.
