import assert from 'node:assert/strict';
import {mkdtemp, mkdir, readFile, rm, writeFile} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import test from 'node:test';
import textDocs, {exportText, markdownBody, readDocuments, validateLinks, textBody} from '../plugins/text-docs/index.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

test('all current document links resolve in the text edition', async () => {
  validateLinks(await readDocuments(path.join(root, 'docs')));
});

test('front matter is omitted but exact contract bodies are preserved', () => {
  const body = '# Contract\n\n### PR-REQ-9999 - Example\n\nMUST preserve \u00e9.\n\n**Verification: Pending automated coverage.**\n';
  assert.equal(markdownBody('---\ntitle: Test\n---\n\n' + body), body);
});

test('missing or escaping relative links fail, external links remain untouched', () => {
  assert.throws(() => validateLinks([['agents/index.md', '[bad](../../outside.md)']]));
  assert.throws(() => validateLinks([['index.md', '[bad](missing.md)']]));
  validateLinks([['index.md', '[external](https://example.org) [local](#section)']]);
});

test('code examples are not parsed as document links', () => {
  validateLinks([['index.md', '`[not-a-link](missing.md)`\n\n```text\n[also-not-a-link](absent.md)\n```\n']]);
});

test('text export is deterministic, preserves relative links, and removes stale output only', async () => {
  const workspace = await mkdtemp(path.join(os.tmpdir(), 'pactrun-text-docs-'));
  try {
    const docsDir = path.join(workspace, 'docs');
    const outDir = path.join(workspace, 'build');
    await mkdir(path.join(docsDir, 'agents'), {recursive: true});
    await mkdir(path.join(docsDir, 'archive'), {recursive: true});
    await mkdir(path.join(docsDir, 'proposals'), {recursive: true});
    await mkdir(path.join(outDir, 'agent-docs'), {recursive: true});
    await writeFile(path.join(outDir, 'keep.html'), 'human');
    await writeFile(path.join(outDir, 'agent-docs/stale.md'), 'obsolete');
    await writeFile(path.join(docsDir, 'index.md'), '# Spec\n');
    await writeFile(path.join(docsDir, 'agents/index.md'), '# Agent\n[Spec](../index.md)\n');
    await writeFile(path.join(docsDir, 'archive/old.md'), '# Historical\n');
    await writeFile(path.join(docsDir, 'proposals/new.md'), '# Proposed\n');
    const schema = '  {"type":"object","description":"[not Markdown](missing.md)"}\n';
    await writeFile(path.join(docsDir, 'cli.schema.json'), schema);
    const first = await exportText({docsDir, outDir});
    const second = await exportText({docsDir, outDir});
    assert.deepEqual(first, second);
    const plugin = textDocs({siteDir: path.join(workspace, 'website'), siteConfig: {baseUrl: '/'}});
    await plugin.postBuild({outDir, routesPaths: ['/']});
    await writeFile(path.join(outDir, 'agent-docs/index.md'), '# Corrupted export\n');
    await assert.rejects(plugin.postBuild({outDir, routesPaths: ['/']}), /drift/);
    await exportText({docsDir, outDir});
    assert.equal(first.count, 3);
    assert.equal(await readFile(path.join(outDir, 'agent-docs/cli.schema.json'), 'utf8'), schema);
    assert.equal(await readFile(path.join(outDir, 'keep.html'), 'utf8'), 'human');
    await assert.rejects(readFile(path.join(outDir, 'agent-docs/stale.md')));
    await assert.rejects(readFile(path.join(outDir, 'agent-docs/archive/old.md')));
    assert.equal(await readFile(path.join(outDir, 'agent-docs/agents/index.md'), 'utf8'), textBody('agents/index.md', '# Agent\n[Spec](../index.md)\n'));
    const entry = await readFile(path.join(outDir, 'llms.txt'), 'utf8');
    assert.match(entry, /\.\/agent-docs\/agents\/index\.md/);
    await writeFile(path.join(docsDir, 'index.md'), '# Changed spec\n');
    await assert.rejects(plugin.postBuild({outDir, routesPaths: ['/']}), /stale/);
    assert.notEqual((await exportText({docsDir, outDir})).digest, first.digest);
  } finally {
    await rm(workspace, {recursive: true, force: true});
  }
});


test('agent text shares classification and preserves the entire original body after context', () => {
  const source = '---\ntitle: Old\n---\n\n# Contract\nMUST preserve this exact sentence.\n';
  for (const [name, state] of [['development/m4-implementation-status.md', 'historical'], ['spec/contracts/pack-source.md', 'current'], ['pactrun-developers/old.md', 'superseded']]) {
    const text = textBody(name, source);
    assert.ok(text.startsWith('> Document context (generated): ' + state));
    assert.equal(text.slice(text.indexOf('\n\n') + 2), markdownBody(source));
    assert.ok(text.includes('> Source: docs/' + name));
  }
  const schema = '  {"type":"object"}\n';
  assert.equal(textBody('example.schema.json', schema), schema);
});

test('discovery supports a project base path and agent HTML routes are rejected', async () => {
  const plugin = textDocs({siteDir: path.join(root, 'website'), siteConfig: {baseUrl: '/pactrun/'}});
  assert.equal(plugin.injectHtmlTags().headTags[0].attributes.href, '/pactrun/llms.txt');
  await assert.rejects(plugin.postBuild({outDir: 'unused', routesPaths: ['/pactrun/agents/index']}), /HTML route/);
});

test('agent source files are excluded from human document generation', async () => {
  const config = await readFile(path.join(root, 'website/docusaurus.config.ts'), 'utf8');
  const sidebar = await readFile(path.join(root, 'website/sidebars.ts'), 'utf8');
  assert.match(config, /exclude: \['agents\/\*\*'/);
  assert.doesNotMatch(sidebar, /['"]agents\//);
});

test('repository README links and machine-format descriptions follow current contracts', async () => {
  const readme = await readFile(path.join(root, 'README.md'), 'utf8');
  const contributing = await readFile(path.join(root, 'CONTRIBUTING.md'), 'utf8');
  const documents = await readDocuments(path.join(root, 'docs'));
  validateLinks([
    ['README.md', readme],
    ['CONTRIBUTING.md', contributing],
    ...documents.map(([name]) => ['docs/' + name, '']),
  ]);
  for (const file of ['cli-machine.schema.json', 'cli-events.schema.json']) {
    const schema = JSON.parse(await readFile(path.join(root, 'docs/spec/contracts', file), 'utf8'));
    for (const field of ['format', 'format_version']) {
      const declaration = field + ': ' + JSON.stringify(schema.properties[field].const);
      assert.ok(readme.includes('`' + declaration + '`'), file + ': README must describe ' + declaration);
    }
  }
});
