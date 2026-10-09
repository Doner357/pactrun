import assert from 'node:assert/strict';
import test from 'node:test';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {readFile, mkdir, mkdtemp, writeFile, rm} from 'node:fs/promises';
import os from 'node:os';
import {readDocuments, documentDigest, textBody} from '../plugins/text-docs/index.mjs';
import {makeCatalog, classify, commandHelp, documentRoute, requirementAnchors, validateRequirementAnchors, plainText} from '../plugins/document-catalog/index.mjs';
import {searchPages} from '../src/lib/search.mjs';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const docs = await readDocuments(path.join(root, 'docs'));
const catalog = makeCatalog(docs);

test('optional maintainer citations do not outrank task content through hidden search terms', () => {
  const body = '# Task\n\nUseful reader content.\n\n<details>\n<summary>Maintainer sources (optional)</summary>\n\nHidden-owner-token\n\n</details>\n\n<details>\n<summary>Worked example</summary>\n\nVisible-task-token\n\n</details>\n';
  assert.ok(!plainText(body).includes('Hidden-owner-token'));
  assert.ok(plainText(body).includes('Visible-task-token'));
});

test('bootstrap prose agrees with the current private SQLite marker and rejects development upgrades', async () => {
  const implementation = await readFile(path.join(root, 'src/persistence/sqlite_revision_store.rs'), 'utf8');
  const marker = implementation.match(/const SCHEMA_VERSION: i64 = (\d+);/)[1];
  const identity = docs.find(([name]) => name === 'spec/storage/revision-records.md')[1].replace(/\s+/g, ' ');
  const bootstrap = identity.split('### PR-REQ-0231')[1].split('### PR-REQ-0232')[0];
  assert.ok(bootstrap.includes('private user version `' + marker + '`'));
  assert.ok(bootstrap.includes('pactrun_metadata.format_version'));
  assert.doesNotMatch(bootstrap, /exact V1|complete V1|Every V1/);
  assert.match(bootstrap, /rejected without conversion or data deletion/);
  const persistence = docs.find(([name]) => name === 'spec/persistence/persistence-baseline.md')[1];
  assert.doesNotMatch(persistence, /migration executes only the additions|only future\s+destructive lifecycle|later M7 schema must add/);
});

test('persistence reference exposes current records, not retired implementation-slice exclusions', () => {
  const persistence = docs.find(([name]) => name === 'spec/persistence/persistence-baseline.md')[1];
  assert.doesNotMatch(persistence, /^## (?:Historical|Deferred)/m);
  assert.ok(persistence.includes('pr-req-0338---exact-v8-lifecycle-records'));
  for (const [role, anchor] of [['Instance', 'candidate-crate-private-repository-contract'], ['Run', 'candidate-crate-private-repository-contract-1']]) {
    assert.ok(persistence.includes(`## ${role} repository contract (crate-private) {#${anchor}}`));
  }
  for (const id of ['0257', '0270', '0276', '0300', '0312', '0325', '0339']) {
    assert.doesNotMatch(persistence, new RegExp('^### PR-REQ-' + id, 'm'));
  }
  assert.match(persistence, /^### PR-REQ-0299 /m);
});

test('service diagnostic identities match the registered catalog rather than development owners', async () => {
  const registry = JSON.parse(await readFile(path.join(root, 'tests/vectors/error_taxonomy_v1/catalog.json'), 'utf8'));
  const source = docs.find(([name]) => name === 'spec/instances/resource-commands.md')[1];
  const rows = [...source.matchAll(/^\| ([a-z_]+) \| ([a-z_]+) \|/gm)];
  const entries = registry.codes.filter(entry => entry.owner === 'service_storage');
  assert.equal(rows.length, entries.length);
  for (const [, code, category] of rows)
    assert.ok(entries.some(entry => entry.code === code && entry.category === category), code);
  assert.ok(registry.codes.some(entry => entry.owner === 'revision_core'));
  assert.ok(registry.codes.some(entry => entry.owner === 'hook_protocol'));
  assert.doesNotMatch(source, /revision_core_format_v2|semantics for Core V2 only/);
  assert.match(source.replace(/\s+/g, ' '), /Only registered owner\/code pairs are stable error references/);
});

test('main command reference composes with selectors, machine output and retired upgrade policy', () => {
  const source = docs.find(([name]) => name === 'spec/interfaces/commands.md')[1].replace(/\s+/g, ' ');
  assert.ok(source.includes('id-selectors.md'));
  assert.match(source, /local-names\.md/);
  const names = docs.find(([name]) => name === 'spec/packages/local-names.md')[1];
  assert.match(names, /ID prefixes require at least eight digits/);
  assert.ok(source.includes('machine-output.md'));
  assert.match(source.replace(/\s+/g, ' '), /Unsupported stores are refused without conversion or data deletion/);
  assert.doesNotMatch(source, /Bare tokens, digest prefixes|explicit storage-upgrade spelling/);
});

test('current runtime owners do not restate superseded milestone availability', () => {
  const obsolete = /execution remains unsupported until M7|future-version direction|accepted semantic policy for a future versioned design|How a future Plan|Future Migration target publication|future Revision Core format may make a|current pre-implementation specification stage|deferred to Versioning and Baseline Consolidation|future lifecycle support/;
  for (const [name, body] of docs) {
    if (classify(name, body).state !== 'current' || !name.endsWith('.md')) continue;
    assert.doesNotMatch(body.replace(/\s+/g, ' '), obsolete, name);
  }
  const snapshots = docs.find(([name]) => name === 'spec/snapshots/execution.md')[1];
  assert.doesNotMatch(snapshots, /Historical M4|development\//);
  assert.match(snapshots, /Confirmed|confirmed/);
});

test('reference index names each current format owner once and Snapshot verification refuses unsupported formats', () => {
  const index = docs.find(([name]) => name === 'spec/catalog.md')[1];
  const targets = [...index.matchAll(/^- \[[^\]]+\]\(([^)]+)\)/gm)].map(match => match[1]);
  assert.equal(new Set(targets).size, targets.length);
  for (const name of ['packages/source-format', 'packages/revision-format', 'interfaces/hook-protocol', 'snapshots/integrity', 'snapshots/bundles', 'packages/distribution']) {
    const owner = docs.find(([file]) => file === `spec/${name}.md`)[1];
    const title = owner.match(/^title: (.+)$/m)[1];
    assert.ok(index.includes(`[${title}](./${name}.md)`), name);
  }
  assert.ok(index.includes('operations/diagnostics.md'));
  const commands = docs.find(([name]) => name === 'spec/snapshots/commands.md')[1];
  assert.match(commands, /Unsupported formats are refused without conversion/);
  assert.match(commands, /complete payload/);
});

test('retired schema pages are replaced by version-scoped redirects to the current schema', async () => {
  const aliases = JSON.parse(await readFile(path.join(root, 'website/spec-redirects.json'), 'utf8'));
  for (let version = 2; version <= 11; version++) {
    const name = `spec/persistence/persistence-schema-v${version}.md`;
    assert.ok(!docs.some(([file]) => file === name));
    assert.ok(!catalog.pages.some(page => page.source === name));
    assert.equal(aliases['/' + name.replace(/\.md$/, '')], '/spec/storage/schema');
  }
  assert.ok(catalog.pages.some(page => page.source === 'spec/persistence/persistence-baseline.md' && page.url === '/spec/storage/schema'));
});

test('topic indexes and vocabulary are searchable references without reader-facing classification notices', () => {
  const names = ['spec/index.md', 'spec/catalog.md', 'spec/core/vocabulary.md',
    ...['core', 'packages', 'instances', 'operations', 'snapshots', 'migrations', 'lifecycle', 'interfaces', 'storage'].map(section => `spec/${section}/index.md`)];
  for (const name of names) {
    const source = docs.find(([file]) => file === name)[1];
    const entry = catalog.pages.find(page => page.source === name);
    assert.equal(entry.state, 'current', name);
    assert.equal(entry.role, 'Reference', name);
    assert.ok(searchPages(catalog.pages, {role: 'Reference'}).some(page => page.source === name));
    assert.ok(!searchPages(catalog.pages, {role: 'Specification'}).some(page => page.source === name));
    assert.doesNotMatch(textBody(name, source), /^> .*Informative/m);
  }
  assert.equal(catalog.pages.find(page => page.source === 'spec/core/objects.md').role, 'Specification');
});

test('Input deletion guide distinguishes active required, optional and retained bindings', () => {
  const text = docs.find(([name]) => name === 'guides/configure-inputs.md')[1].replace(/\s+/g, ' ');
  assert.match(text, /optional active binding or a retained binding/);
  assert.match(text, /active required binding cannot be deleted once it is bound/);
  assert.match(text, /pactrun input delete demo notes/);
  assert.doesNotMatch(text, /Deletion can leave a required Input missing/);
});

test('current ServiceStorage summaries and evidence boundaries do not defer available contracts', () => {
  for (const name of ['spec/core/vocabulary.md', 'spec/core/objects.md',
    'spec/operations/actions-plans-runs.md', 'spec/operations/action-definitions.md',
    'spec/index.md', 'spec/lifecycle/recovery.md']) {
    const text = docs.find(([file]) => file === name)[1].replace(/\s+/g, ' ');
    assert.doesNotMatch(text, /runtime remains deferred|future versioned contract|future service-resource exposure|future authority and operation-prerequisite|their deferred representation|remains pending under M7/, name);
  }
  const recovery = docs.find(([name]) => name === 'spec/lifecycle/recovery.md')[1];
  assert.ok(recovery.includes('retirement.md'));
  assert.match(recovery.replace(/\s+/g, ' '), /Successful Instance deletion removes the object/);
  const storage = docs.find(([name]) => name === 'spec/instances/service-storage.md')[1];
  assert.match(storage.replace(/\s+/g, ' '), /Abandonment cannot be represented by deleting allocation rows/);
});

test('catalog retains source identity, excludes agent routes, and covers reader guides', () => {
  assert.equal(catalog.source_digest, documentDigest(docs));
  assert.ok(catalog.pages.length > 100);
  assert.ok(catalog.pages.every(p => !p.source.startsWith('agents/')));
  for (const name of ['introduction.md', 'guides/retirement.md', 'package-authors/fundamentals/authoring-model.md'])
    assert.equal(catalog.pages.find(p => p.source === name).state, 'current');
});
test('superseded contracts cannot masquerade as current search results', () => {
  assert.equal(classify('spec/packages/source-format.md', 'title: Retired development contract').state, 'superseded');
  assert.equal(classify('spec/snapshots/limits.md', '').state, 'current');
  assert.equal(classify('pactrun-developers/architecture/system-model.md', '').state, 'superseded');
  assert.equal(classify('engineering/documentation-edition-1.md', '**Status: Informative compatibility entry; no independent specification.**').state, 'superseded');
});
test('search supports IDs, multiword queries, combined filters, superseded pages and no results', () => {
  assert.equal(searchPages(catalog.pages, {q: 'PR-REQ-0258'})[0].source, 'spec/packages/source-format.md');
  const restore = searchPages(catalog.pages, {q: 'restore', audience: 'Users'});
  assert.ok(restore.some(p => p.source.endsWith('snapshots-migrations-and-recovery.md')));
  assert.ok(restore.every(p => p.state === 'current' && p.audiences.includes('Users')));
  assert.ok(searchPages(catalog.pages, {q: 'shell loader', audience: 'Authors'}).length > 0);
  assert.ok(searchPages(catalog.pages, {state: 'superseded'}).length > 0);
  assert.ok(searchPages(catalog.pages, {state: ''}).length > searchPages(catalog.pages).length);
  assert.equal(searchPages(catalog.pages, {q: 'nonexistent-zzyy-887766'}).length, 0);
  assert.equal(searchPages(catalog.pages, {state: 'invalid'}).length, 0);
  assert.ok(searchPages(catalog.pages, {q: '  PR-REQ-0258  ', role: 'Specification'}).length > 0);
});
test('document routes preserve index and explicit slug semantics, rejecting duplicate URLs', () => {
  assert.equal(documentRoute('guides/index.md', ''), '/guides/');
  assert.equal(documentRoute('index.md', 'slug: /'), '/');
  assert.throws(() => documentRoute('a.md', 'slug: ../bad'));
  assert.throws(() => makeCatalog([['a.md', 'slug: /same'], ['b.md', 'slug: /same']]));
});
test('CLI reference derives its version and command text from executable declarations', async () => {
  const help = await commandHelp(root);
  assert.match(help.text, /pactrun pack install/);
  assert.match(help.text, /pactrun snapshot restore/);
  assert.match(help.text, /pactrun storage gc/);
  assert.ok(help.text.startsWith('Pactrun ' + help.version));
  assert.equal(help.source, 'src/cli/help_catalog.json');
  const declaration = JSON.parse(await readFile(path.join(root, help.source), 'utf8'));
  for (const command of declaration.commands) {
    for (const form of command.forms) {
      assert.ok(help.text.includes(form.usage));
      assert.ok(help.text.includes(form.description));
    }
  }
});
test('current entry points do not repeat retired storage/approval claims', () => {
  for (const name of ['index.md', 'agents/index.md', 'guides/index.md', 'introduction.md']) {
    const body = docs.find(([file]) => file === name)[1];
    assert.doesNotMatch(body, /V11|exact-V8\/V9\/V10|M5 review gates|E is the next|planned placeholders|guides come later/i, name);
  }
});


test('Windows tutorials supply complete manifests rather than requiring test-side rewrites', () => {
  const cases = [
    ['package-authors/fundamentals/authoring-model.md', 'Windows variant', 'inspect.ps1'],
    ['package-authors/managed-capabilities/snapshots-and-managed-data.md', 'Windows Snapshot variant', 'snapshot.ps1'],
    ['package-authors/managed-capabilities/migrations.md', 'Windows target variant', 'migrate.ps1'],
  ];
  for (const [name, heading, script] of cases) {
    const body = docs.find(([file]) => file === name)[1].split('### ' + heading + '\n')[1];
    const yaml = body.match(/^\x60{3}yaml\n([\s\S]*?)^\x60{3}/m)?.[1];
    assert.ok(yaml, name);
    assert.ok(yaml.includes('shell: powershell_7, command: pwsh.exe'), name);
    assert.ok(yaml.includes('source: ' + script + ', path: ' + script), name);
    assert.doesNotMatch(yaml, /shell: sh|source: .*\.sh/);
  }
});


test('introductory tutorial names the installed Revision before using its local reference', () => {
  const document = name => docs.find(([file]) => file === name)[1];
  const intro = document('introduction.md');
  assert.match(intro, /pactrun pack install \.\/pack --package-name demo --revision-name initial/);
  assert.match(intro, /pactrun instance create demo --revision demo:initial/);
  assert.doesNotMatch(intro, /### Copy a Revision reference/);
  const author = document('package-authors/fundamentals/authoring-model.md');
  assert.match(author, /pack install \.\/pack --package-name hook-example --revision-name initial/);
  assert.match(author, /instance create hook-demo --revision hook-example:initial/);
});

test('Snapshot tutorial distinguishes source restoration from fresh-store recovery', () => {
  const body = docs.find(([name]) => name === 'package-authors/managed-capabilities/snapshots-and-managed-data.md')[1];
  const headings = ['Exercise Capture and Restore', 'Export the recovery files', 'Prepare a fresh destination store', 'Import and restore in the destination', 'Prepare an incompatible Revision', 'Check the incompatible Restore refusal', 'Retire destination test objects', 'Return to the source store'];
  const offsets = headings.map(h => body.indexOf('### ' + h + '\n'));
  assert.ok(offsets.every((offset, i) => offset >= 0 && (!i || offset > offsets[i - 1])));
  assert.match(body, /pactrun revision export <snapshot-reference> --output \.\/snapshot-pack/);
  assert.match(body, /pactrun pack install \.\/snapshot-pack\.pack/);
  assert.match(body, /pactrun snapshot import \.\/backup\.snapshot/);
  assert.match(body, /pactrun snapshot restore restored-demo <snapshot-id>/);
  assert.match(body.replace(/\s+/g, ' '), /already_present.*not fresh; do not count it/);
  assert.match(body, /Test-Path -LiteralPath '\.\/recovery-store'/);
  assert.match(body, /\[ -e \.\/recovery-store \]/);
});

test('Migration tutorial provides executable byte comparisons and a separate Package fixture', () => {
  const body = docs.find(([name]) => name === 'package-authors/managed-capabilities/migrations.md')[1];
  assert.match(body, /### Compare the exported Input/);
  assert.match(body, /ReadAllBytes/);
  assert.match(body, /\$original\[\$i\] -ne \$copy\[\$i\]/);
  assert.match(body, /cmp \.\/config\.txt \.\/settings-copy\.txt/);
  assert.match(body, /pactrun pack generate-id/);
  assert.match(body, /migration-unrelated\/pactrun\.yaml/);
  assert.match(body, /--to <unrelated-reference> --plan/);
  assert.match(body, /value after.*path_id:/);
});

test('operator Snapshot guidance links to the same fresh-store recovery example', () => {
  const body = docs.find(([name]) => name === 'pactrun-users/operations/snapshots-migrations-and-recovery.md')[1];
  assert.match(body, /already_present/);
  assert.match(body, /snapshots-and-managed-data\.md#export-the-recovery-files/);
});


test('search excerpts hide custom heading syntax while anchors retain their stable identity', () => {
  const source = '### PR-REQ-9001 - Current heading {#legacy-anchor}\n\nCurrent body.\n';
  assert.equal(plainText(source), 'PR-REQ-9001 - Current heading Current body.');
  assert.equal(requirementAnchors(source)['PR-REQ-9001'], 'legacy-anchor');
});
test('new guides use complete fences, live owner links, and no invented normative definitions', () => {
  for (const [name, body] of docs.filter(([name]) => /^(guides|pactrun-users|package-authors)\//.test(name) || name === 'introduction.md')) {
    assert.equal((body.match(/^\x60{3}/gm) ?? []).length % 2, 0, name);
    assert.doesNotMatch(body, /\x00|^### PR-REQ-\d+/m, name);
    assert.doesNotMatch(body, /Planned usage-guide placeholder|guidance is planned/i, name);
  }
});
test('search and text discovery are integrated without authorizing Pages', async () => {
  const config = await readFile(path.join(root, 'website/docusaurus.config.ts'), 'utf8');
  assert.match(config, /document-catalog\/index.mjs/);
  assert.match(config, /type: 'custom-editionLink', target: '\/search'/);
});


test('reader filters retain shared references and exact requirement links', () => {
  const author = searchPages(catalog.pages, {q: 'PR-REQ-0258', audience: 'Authors'})[0];
  assert.equal(author.source, 'spec/packages/source-format.md');
  assert.ok(author.url.endsWith('#pr-req-0258---packsourceyamlv1-schema-numbers-and-package-lineage'));
  for (const q of ['--param-file', '--action-timeout-ms']) {
    assert.ok(searchPages(catalog.pages, {q, audience: 'Users'}).some(p => p.source.endsWith('/invoke-reference.md')));
  }
  assert.ok(classify('spec/packages/source-format.md', '').audiences.includes('Authors'));
  assert.deepEqual(classify('spec/persistence/persistence-baseline.md', '').audiences, ['Developers']);
  assert.deepEqual(searchPages(catalog.pages, {q: 'PR-REQ-0258', role: 'Tutorial'}), []);
  assert.ok(searchPages(catalog.pages, {q: 'PR-REQ-0258', state: ''}).length > 0);
});

test('requirement anchors must exist in rendered HTML, not merely resemble slugs', async () => {
  const dir = await mkdtemp(path.join(os.tmpdir(), 'pactrun-anchors-'));
  try {
    const anchors = requirementAnchors('### PR-REQ-9000 - Typed "state"\n### PR-REQ-9001 - Explicit {#stable-rule}\n');
    assert.deepEqual(anchors, {'PR-REQ-9000': 'pr-req-9000---typed-state', 'PR-REQ-9001': 'stable-rule'});
    const pages = [{source: 'spec/test.md', url: '/spec/test', requirementAnchors: anchors}];
    await mkdir(path.join(dir, 'spec/test'), {recursive: true});
    await writeFile(path.join(dir, 'spec/test/index.html'), '<h3 id="pr-req-9000---typed-state"></h3><h3 id="stable-rule"></h3>');
    await validateRequirementAnchors(dir, pages);
    await writeFile(path.join(dir, 'spec/test/index.html'), '<h3 id="renamed"></h3>');
    await assert.rejects(validateRequirementAnchors(dir, pages), /Unverified requirement anchor/);
  } finally { await rm(dir, {recursive: true, force: true}); }
});

test('effective baseline statements do not contradict canonical versions or diagnostics', () => {
  const doc = name => docs.find(([file]) => file === name)[1].replace(/\s+/g, ' ');
  const source = doc('spec/packages/source-format.md');
  const canonical = doc('spec/packages/revision-format.md');
  const marker = canonical.match(/format_version: "([^"]+)"/)[1];
  assert.ok(source.includes('Revision Core format version ' + String.fromCharCode(96) + '"' + marker + '"'));
  assert.doesNotMatch(source, /including Hook protocol versions/);
  assert.match(source, /numeric markers are rejected/);
  const loader = doc('spec/interfaces/shell-loader.md');
  assert.match(loader, /retained by default for Run inspection/);
  assert.doesNotMatch(loader, /Core discards its text|not displayed live or retained/);
  assert.match(loader, /execution diagnostics policy/);
  assert.match(doc('spec/snapshots/commands.md'), /prefixes of at least eight digits/);
  assert.doesNotMatch(doc('spec/instances/resource-commands.md'), /storage upgrade.*retains its existing/);
  assert.doesNotMatch(doc('spec/index.md'), /Reserved for later usage documentation/);
});

test('Invoke reference enumerates parser flags and warns about byte-preserving acquisition', async () => {
  const source = await readFile(path.join(root, 'src/cli.rs'), 'utf8');
  const parser = source.split('fn parse_execution_option(')[1].split('\nfn parse_run(')[0];
  const flags = new Set([...parser.matchAll(/^        "([a-z-]+)"/gm)].map(m => m[1]));
  flags.add('action-timeout-ms');
  const guide = docs.find(([name]) => name.endsWith('/invoke-reference.md'))[1];
  for (const flag of flags) assert.ok(guide.includes('--' + flag), flag);
  assert.match(guide, /5000 milliseconds/);
  assert.match(guide, /does not trim whitespace/);
  assert.match(guide, /pactrun invoke --help/);
  assert.match(guide, /syntax\/option errors are 2/);
});


test('current document prose rejects the obsolete availability claims found by full-site review', () => {
  const obsolete = [
    /current integrated baseline is V11/i,
    /persistence baseline is V[0-9]+/i,
    /\[V[0-9]+\]\([^)]*\) is current in/i,
    /prerequisites remain future design work rather than current V1/i,
    /persistence ownership remain future design gates/i,
    /Current integrated persistence:.*\[V11\]/i,
    /Existing Snapshot compatibility:.*\[V1\].*supported/i,
    /YAML V1\/V2\/V3 are the current built-in frontends/i,
    /usage guides remain planned for a later documentation phase/i,
    /Usage guides are reserved for later/i,
    /Formal ServiceStorage authoring and runtime support requires future/i,
    /The next entry is M6\.5 design/i,
    /short hash-prefix lookup is not supported/i,
    /No generic force flag, stable JSON/i,
    /discard remain future persistence and runtime design gates/i,
    /The full multi-slice requirement is not yet end-to-end complete/i,
  ];
  for (const [name, body] of docs.filter(([name]) => name.endsWith('.md'))) {
    if (classify(name, body).state !== 'current') continue;
    const prose = body.replace(/\s+/g, ' ');
    for (const pattern of obsolete) assert.doesNotMatch(prose, pattern, name);
  }
});


test('ServiceStorage overview and authoring reference the effective declarations without erasing deferred taxonomy', () => {
  const get = name => docs.find(([file]) => file === name)[1];
  const author = get('spec/packages/authoring.md');
  assert.match(author, /ServiceStorageDeclarations/);
  assert.match(author, /ServiceResourceDeclarations/);
  for (const name of ['spec/packages/authoring.md', 'spec/interfaces/hooks.md', 'spec/instances/inputs-secrets.md']) {
    const text = get(name);
    assert.ok(text.includes('service-storage.md'), name);
    assert.ok(text.includes('retirement.md'), name);
  }
  const scope = get('spec/storage/format-domains.md').replace(/\s+/g, ' ');
  assert.match(scope, /MUST NOT add them retroactively to an older closed schema/);
  assert.match(scope, /Revision Core and Hook Protocol remain independent version domains/);
  assert.match(scope, /cover ServiceStorage-backed resources only/);
});

test('lifecycle clarification retains non-replay and non-destruction obligations with concrete owners', () => {
  const text = docs.find(([name]) => name === 'spec/lifecycle/deletion.md')[1].replace(/\s+/g, ' ');
  assert.match(text, /MUST NOT be treated as proof that replay is safe/);
  assert.match(text, /MUST NOT replay Cleanup/);
  assert.match(text, /MUST NOT require a public persistent/);
  assert.match(text, /MUST NOT destructively remove the abandoned service-owned state/);
  assert.ok(text.includes('retirement.md'));
  assert.ok(text.includes('persistence-baseline.md'));
});

test('Migration and retirement spellings compose with current selectors and machine presentation', () => {
  const migration = docs.find(([name]) => name === 'spec/migrations/commands.md')[1];
  const retirement = docs.find(([name]) => name === 'spec/lifecycle/retirement.md')[1];
  assert.ok(migration.includes('mp1-<8..63 hex digits>'));
  assert.ok(migration.includes('id-selectors.md#pr-req-0369'));
  for (const text of [migration, retirement]) assert.ok(text.includes('machine-output.md'));
  assert.match(retirement.replace(/\s+/g, ' '), /No generic force flag, replayable Plan or path-based discard exists/);
});

test('Snapshot contracts describe complete current execution without staged-delivery claims', () => {
  const execution = docs.find(([name]) => name === 'spec/snapshots/execution.md')[1];
  const commands = docs.find(([name]) => name === 'spec/snapshots/commands.md')[1];
  for (const text of [execution, commands]) assert.doesNotMatch(text, /Historical M4|S[3-7] coverage|later-slice|development\//);
  assert.match(execution, /RecoveryConsequenceVersion/);
  assert.match(execution, /MUST NOT.*clear|clear no guard/);
  assert.match(commands, /incomplete verification/);
  assert.match(commands, /not_recorded/);
});
