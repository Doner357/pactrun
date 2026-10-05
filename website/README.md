# Website maintenance

Canonical Markdown lives in `docs/`; `website/` renders it as HTML and agent
text. Commands below run from the repository root. Use the Node and pnpm versions
in `website/package.json`.

```text
pnpm --dir website install --frozen-lockfile
pnpm --dir website test:docs
pnpm --dir website typecheck
cargo xtask docs-build
```

The build includes document/link/traceability checks and text generation.
Use the build and preview environment permitted by the workspace instructions.

## Generated reader references

After changing a selected Spec owner, emit patches into a new directory:

```text
node website/scripts/reader-references.mjs --patch-dir target/reader-reference-patches
```

Review and apply the patches rather than editing generated views independently.
The source-to-view mappings are in `website/scripts/reader-references.mjs`.
The generator preserves code, schema bytes and anchors while adapting links to
the reader's section. Tests reject stale views. An existing nonempty patch
directory is refused; choose a new output directory for the next run.

## Documentation versions

`website/versions.json` lists saved editions. The current `docs/` tree appears
at `/next/`. To capture a new release edition:

```text
pnpm --dir website docs:version VERSION
```

Replace VERSION with the released product version matching Cargo.toml. The script
requires clean, committed documentation, sidebar and CLI inputs and refuses an
existing version. It creates versioned Markdown and a sidebar, updates the
edition registry, and captures CLI help with its source commit. It does not
publish the website.

For a correction to an existing edition, edit its files under
`website/versioned_docs/version-VERSION/` and the affected versioned sidebar;
do not run the snapshot command again. Keep the correction applicable to that
executable. Update generated reader views in each affected edition as well.

## Navigation and browser checks

Update `website/sidebars.ts` for the current tree. Reader sequences live in
`website/src/lib/reading-order.mjs`. Bookmarks with a real new owner use
`website/spec-redirects.json` and `website/spec-section-redirects.json`;
raw-text aliases use `website/spec-source-aliases.json`.

Build and inspect both root and project deployment paths with
`DOCUSAURUS_URL` and `DOCUSAURUS_BASE_URL`. Browser checks need an installed
Playwright package and Chromium runtime. Each script requires the preview base
URL, an output directory, and `PLAYWRIGHT_PACKAGE` pointing to that package's
absolute `index.mjs` path. For example, on the POSIX test host:

```sh
PLAYWRIGHT_PACKAGE=/absolute/path/to/playwright/index.mjs \
  node website/scripts/spec-browser.mjs http://127.0.0.1:3000/ target/spec-browser-review
```

Use `website/scripts/version-browser.mjs` with the same arguments to check
version switching, search and unavailable pages. Browser output includes receipts
and screenshots; inspect the rendered pages as well as the exit status.

Each edition's `agent-docs/publication.json` and `document-catalog.json` carry
a source digest. Compare it with the intended sources after publication; a green
build alone does not establish that the public site was updated.
