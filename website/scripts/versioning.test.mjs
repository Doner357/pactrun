import assert from 'node:assert/strict';
import test from 'node:test';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import {readFile, mkdtemp, rm} from 'node:fs/promises';
import os from 'node:os';
import {spawnSync} from 'node:child_process';
import {editions, versionLinks} from '../plugins/editions.mjs';
import {editionCatalog} from '../plugins/document-catalog/index.mjs';
import {exportText, verifyText, readDocuments, documentDigest, textBody, textAliases} from '../plugins/text-docs/index.mjs';
import {searchPages} from '../src/lib/search.mjs';

const site = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const versions = await editions(site);

test('every published text alias resolves against each edition source tree', async () => {
  const aliases = JSON.parse(await readFile(path.join(site, 'spec-source-aliases.json'), 'utf8'));
  for (const version of versions) {
    const result = textAliases(await readDocuments(version.docsDir), aliases, version.id, '/');
    assert.ok(result.length > 0);
    assert.ok(result.some(([name, text]) => name.endsWith('identity-and-state.md') && text.includes('PR-REQ-0025')));
  }
});

test('released and development documents have disjoint routes and scoped search', async () => {
  assert.notEqual(versions[0].id, 'current');
  assert.equal(versions.at(-1).id, 'current');
  const seen = new Set();
  for (const version of versions) {
    const catalog = await editionCatalog(site, version);
    for (const page of catalog.pages) {
      assert.ok(page.url.startsWith('/' + version.path + '/'));
      assert.ok(!seen.has(page.url), page.url);
      seen.add(page.url);
    }
    const result = searchPages(catalog.pages, {q: 'PR-REQ-0258'})[0];
    assert.ok(result.url.startsWith('/' + version.path + '/spec/'));
    assert.ok(result.url.includes('#pr-req-0258'));
  }
});

test('release CLI help comes from its snapshot, with release provenance', async () => {
  const catalog = await editionCatalog(site, versions.find(item => item.id === '1.0.0-alpha.4'));
  assert.equal(catalog.help.version, '1.0.0-alpha.4');
  assert.equal(catalog.help.release_tag, 'v1.0.0-alpha.4');
  assert.match(catalog.help.source_commit, /^[a-f0-9]{40}$/);
  assert.ok(catalog.help.text.startsWith('Pactrun 1.0.0-alpha.4\nUsage:'));
});

test('text editions preserve body/schema fidelity and identify the product version', async () => {
  const outDir = await mkdtemp(path.join(os.tmpdir(), 'pactrun-versions-'));
  try {
    for (const version of versions) {
      const target = path.join(outDir, version.path);
      await exportText({docsDir: version.docsDir, outDir: target, version: version.id});
      await verifyText({docsDir: version.docsDir, outDir: target, version: version.id});
      const meta = JSON.parse(await readFile(path.join(target, 'agent-docs/publication.json'), 'utf8'));
      assert.equal(meta.documentation_version, version.id);
      assert.equal(meta.source_digest, documentDigest(await readDocuments(version.docsDir)));
      await assert.rejects(verifyText({docsDir: version.docsDir, outDir: target, version: 'wrong'}), /edition mismatch/);
    }
  } finally { await rm(outDir, {recursive: true, force: true}); }
});

test('absolute Markdown links stay in their edition without changing external links or code', () => {
  for (const [file, prefix] of [['/repo/docs/example.md', 'next'], ['/repo/website/versioned_docs/version-1.0.0-alpha.4/example.md', '1.0.0-alpha.4']]) {
    const tree = {children: [{type: 'link', url: '/commands?q=help#usage'},
      {type: 'link', url: 'https://example.com/'}, {type: 'code', value: '[example](/commands)'}]};
    versionLinks()(tree, {history: [file]});
    assert.equal(tree.children[0].url, '/' + prefix + '/commands?q=help#usage');
    assert.equal(tree.children[1].url, 'https://example.com/');
    assert.equal(tree.children[2].value, '[example](/commands)');
  }
});

test('agent website links carry the base path and edition without rewriting code or schemas', () => {
  const source = '# Guide\n[Commands](/commands) and [source](../spec/index.md)\n`[example](/commands)`\n```md\n[example](/commands)\n```\n';
  const text = textBody('guides/example.md', source, 'current', '/pactrun/');
  assert.ok(text.includes('[Commands](/pactrun/next/commands)'));
  assert.ok(text.includes('[source](../spec/index.md)'));
  assert.ok(text.includes('`[example](/commands)`'));
  assert.ok(text.includes('```md\n[example](/commands)\n```'));
  assert.equal(textBody('example.schema.json', '{"url":"/commands"}', 'current', '/pactrun/'), '{"url":"/commands"}');
});

test('snapshot command refuses invalid or existing versions without changing the registry', async () => {
  const file = path.join(site, 'versions.json');
  const before = await readFile(file, 'utf8');
  for (const args of [[], ['not-a-product-version'], [versions[0].id]]) {
    const result = spawnSync(process.execPath, [path.join(site, 'scripts/snapshot-version.mjs'), ...args], {encoding: 'utf8'});
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /Usage:|must match Cargo.toml|already exists/);
    assert.equal(await readFile(file, 'utf8'), before);
  }
});
