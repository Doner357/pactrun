# Delivery tools

## Candidate builds and release records

1. Select a reviewed, clean source commit with the intended Cargo product version.
2. Dispatch the Release candidate workflow with that full SHA and exact version.
   Use `main` ancestry or the exact head of `release/<version>`; the workflow checks
   this pairing. Its token builds review artifacts but cannot publish a release.
3. Qualify the exact normal/test/standalone artifacts on the supported environments.
   Source tests alone do not qualify packages. Linux builds once on pinned Bookworm
   and checks ABI, clean non-root runtime, and ordinary-user Homebrew installation.
   Tested userland is not qualification of every kernel, filesystem or service.
4. After publication authorization, upload only qualified bytes under a new release
   identity. Verify downloaded hashes and sizes. Never rebuild or replace an existing
   version merely to update documentation or metadata.
5. Prepare the aggregate release catalog, including the new record and all previous
   published records, then generate and review the project files as described below.
   Integrate reviewed metadata through the project's Git workflow.

Release JSON retains source commit, source manifest, compiler, format support and
archive identities. Keep candidate run and qualification links with the GitHub
Release/PR. Binary source can precede the catalog commit; reproduce that source
with its matching workflow recipe, not today's recipe.

`tools/release_artifacts.py` provides source capture, clean builds, checksums and
legal notices; inspect its subcommand `--help`. The `release-sources` and
`release-publish-local` xtask commands generate isolated source-selection fixtures,
not the public main-branch catalog.
Package installation and update do not provision, convert or delete service data.

## Generate the package catalog

The input is a JSON array of release records, as in `releases/catalog.json`.
Keep a copy of the previous published catalog before editing the candidate input.
From the repository root:

```text
cargo xtask release-catalog CANDIDATE_RELEASES.json target/catalog-review PREVIOUS_RELEASES.json
```

Replace the two input paths with the candidate aggregate and the previous
published aggregate. The third argument checks that published identities have
not changed or disappeared. Omitting it performs no comparison with prior
published records.

The output directory must be new. The generator writes `bucket/`, `Formula/`,
individual `releases/VERSION.json` records and the normalized aggregate catalog
beneath it. Review and apply those generated files; the command does not edit the
repository's package definitions in place, push Git refs or upload release assets.

## CI and GitHub Pages

Workflows and `tools/ci_scope.py` define verification scope. Spec and unknown
changes retain conservative runtime checks. Branch protection should require
the aggregate CI gate, not a conditionally skipped platform job.

Pages needs GitHub Actions as its source, `PACTRUN_PAGES_ENABLED=true` and an eligible
main push. Review the `github-pages` environment's access and deployment branches
when setting up a repository. Configuration and publication require authorization.

CI builds for the configured URL/base path. After its aggregate gate, deployment
uses the same artifact without rebuilding and checks main freshness. Inspect the
public site and compare the edition's publication digest with the intended sources.
After configuration changes, rerun the full eligible workflow: deployment alone
cannot create a missing artifact.

Disabling `PACTRUN_PAGES_ENABLED` prevents subsequent eligible jobs, not a running
deployment or access to the existing site. Cancel an in-progress deployment
explicitly if needed. Roll content back through a newly verified main commit,
not by replaying an obsolete deployment against the freshness guard.
