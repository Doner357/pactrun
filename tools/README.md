# Delivery tools

## Candidate builds and release records

1. Select a reviewed, clean source commit with the intended Cargo product version.
2. Dispatch the Release candidate workflow with that full SHA, exact version and
   explicit published baseline version.
   Use `main` ancestry or the exact head of `release/<version>`; the workflow checks
   this pairing. Its token builds review artifacts but cannot publish a release.
3. The workflow qualifies the exact normal/test/standalone artifacts on the supported environments.
   Source tests alone do not qualify packages. Linux builds once on pinned Bookworm
   and checks ABI, clean non-root runtime, and ordinary-user Homebrew installation.
   Tested userland is not qualification of every kernel, filesystem or service.
   It also runs the existing native continuity scenarios on disposable Windows
   and Ubuntu runners, choosing unchanged-format or supported Store-upgrade
   qualification from the two records. Scoop's temporary PATH entry is confined
   to that runner and its restoration is checked.
4. Download the successful run's `qualified-catalog` artifact. Review and apply
   its generated files on the release branch, preserving older records. Wait for
   the candidate-source CI before pushing metadata so an in-flight qualification
   is not cancelled unnecessarily. Merge through the required `CI gate`.
5. After publication authorization, dispatch Publish qualified release from main
   with the version, successful candidate run, baseline and product-only notes.
   It checks current main, source equivalence, main CI and candidate qualification;
   uploads only the qualified bytes; refuses changed tags/assets; and verifies all
   public downloads anonymously. Native public acquisition checks follow.
6. Pages waits until the corresponding product release is public. Publication
   imports the exact successful main-CI site artifact by producer identity and
   SHA-256 rather than rebuilding it, then checks the actual public editions and
   desktop/mobile reader paths.
7. With existing local owner `gh` authentication, inspect and complete closeout:

   ```text
   python -B -m tools.finish_release --version VERSION --expected-main MAIN_SHA --publication-run RUN_ID
   python -B -m tools.finish_release --version VERSION --expected-main MAIN_SHA --publication-run RUN_ID --merge
   ```

   This back-merges main into develop through protected PR checks, and refuses
   divergent or advanced inputs. No Actions PR-approval permission or additional
   long-lived token is required. Main/develop remain protected; merged temporary
   branches are removed, not unmerged work.

Never rebuild or replace a published version to repair delivery metadata. A
failed upload leaves an unpublished draft; the same publication request can
verify existing matching assets and upload only missing ones. A failure after
publication is a partial delivery requiring verification/retry, not permission
to replace public bytes. Source changes require fresh qualification.

Release JSON retains source commit, source manifest, compiler, format support and
archive identities. Keep candidate run and qualification links with the GitHub
Release/PR. Binary source can precede the catalog commit; reproduce that source
with its matching workflow recipe, not today's recipe.

`tools/release_artifacts.py` provides source capture, clean builds, checksums and
legal notices; inspect its subcommand `--help`. The `release-sources` and
`release-publish-local` xtask commands generate isolated source-selection fixtures,
not the public main-branch catalog.
Package installation and update do not provision, convert or delete service data.

Git dependency notices are read from the exact locked commit, including relative
license symlinks within that repository. The packager rejects links escaping the
Git tree, cycles, oversized files and mismatched checkout identities. This also
avoids packaging Windows symlink-placeholder text as if it were a license.
Registry dependency notices retain their package-directory safety boundary.

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

`tools/ci_reuse.py` accepts runtime receipts only from successful same-repository
CI, with matching tracked inputs, compiler, Node, runner, profile and hosted
image. Generated delivery files are excluded only because their consumers run
afresh on both platforms. Missing, foreign, stale or unverifiable evidence means
fresh tests. Manual CI dispatch never reuses results, allowing controlled runner
comparisons. The reports distinguish reuse from fresh execution.

Pages needs GitHub Actions as its source, `PACTRUN_PAGES_ENABLED=true` and an eligible
main push. Review the `github-pages` environment's access and deployment branches
when setting up a repository. Configuration and publication require authorization.

CI builds for the configured URL/base path. After its aggregate gate, deployment
uses the verified artifact without rebuilding and checks main freshness and
public release availability. Inspect the
public site and compare the edition's publication digest with the intended sources.
After configuration changes, rerun the eligible workflow: deployment alone
cannot create a missing artifact. A publication import additionally requires the
exact successful main producer and matching artifact digest.

Disabling `PACTRUN_PAGES_ENABLED` prevents subsequent eligible jobs, not a running
deployment or access to the existing site. Cancel an in-progress deployment
explicitly if needed. Roll content back through a newly verified main commit,
not by replaying an obsolete deployment against the freshness guard.
