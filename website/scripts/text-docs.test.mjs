import assert from 'node:assert/strict';
import {mkdtemp, mkdir, readFile, rm, writeFile} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import test from 'node:test';
import textDocs, {exportText, markdownBody, readDocuments, validateLinks, textBody, verifyText, textAliases} from '../plugins/text-docs/index.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

test('legacy text aliases combine split owners without breaking links or altering code and JSON', () => {
  const code = '```md\n[example](./literal.md)\n```\n';
  const sources = [['spec/packages/identity.md', '# Identity\n[State](../instances/state.md)\n' + code],
    ['spec/instances/state.md', '# State\n[Identity](../packages/identity.md)\n'],
    ['spec/interfaces/example.schema.json', '{"description":"[literal](./literal.md)"}\n']];
  const aliases = {'spec/foundations/identity-and-state.md': ['spec/packages/identity.md', 'spec/instances/state.md'],
    'spec/contracts/example.schema.json': ['spec/interfaces/example.schema.json']};
  const result = new Map(textAliases(sources, aliases, '1.0.0-alpha.4', '/pactrun/'));
  const body = result.get('spec/foundations/identity-and-state.md');
  assert.ok(body.includes('Source: docs/spec/packages/identity.md'));
  assert.ok(body.includes('Source: docs/spec/instances/state.md'));
  assert.ok(body.includes('[State](../instances/state.md)'));
  assert.ok(body.includes('[Identity](../packages/identity.md)'));
  assert.ok(body.includes(code));
  assert.equal(result.get('spec/contracts/example.schema.json'), sources[2][1]);
  assert.throws(() => textAliases(sources, {'spec/../../outside.md': [sources[0][0]]}, 'current', '/'), /Invalid text alias/);
  assert.throws(() => textAliases(sources, {'spec/old.md': ['spec/missing.md']}, 'current', '/'), /Missing text alias target/);
});

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
    const plugin = {postBuild: () => verifyText({docsDir, outDir})};
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
  for (const [name, state] of [['spec/packages/source-format.md', 'current'], ['pactrun-developers/old.md', 'superseded']]) {
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
  await assert.rejects(plugin.postBuild({outDir: 'unused', routesPaths: ['/pactrun/agents/index']}), /HTML route/);
  await assert.rejects(plugin.postBuild({outDir: 'unused', routesPaths: ['/pactrun/next/agents/index']}), /HTML route/);
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
  const artwork = await Promise.all(['.github/assets/readme-responsibilities.svg', '.github/assets/readme-tooling.svg']
    .map(async name => [name, await readFile(path.join(root, name), 'utf8')]));
  for (const [name, svg] of artwork) {
    assert.match(svg, /<svg\s/, name);
    assert.match(svg, /<title\b/, name + ': accessible title');
    assert.match(svg, /<desc\b/, name + ': text alternative');
    assert.doesNotMatch(svg, /<script\b|\son\w+\s*=/i, name + ': no active content');
  }
  const documents = await readDocuments(path.join(root, 'docs'));
  const maintenance = await Promise.all(['website/README.md', 'tests/README.md', 'tools/README.md'].map(async name => [name, await readFile(path.join(root, name), 'utf8')]));
  validateLinks([
    ['README.md', readme],
    ...artwork,
    ['CONTRIBUTING.md', contributing],
    ...maintenance,
    ['LICENSE', await readFile(path.join(root, 'LICENSE'), 'utf8')],
    ...documents.map(([name]) => ['docs/' + name, '']),
  ]);
  for (const file of ['cli-machine.schema.json', 'cli-events.schema.json']) {
    const schema = JSON.parse(await readFile(path.join(root, 'docs/spec/interfaces', file), 'utf8'));
    for (const field of ['format', 'format_version']) {
      const declaration = field + ': ' + JSON.stringify(schema.properties[field].const);
      assert.ok(readme.includes('`' + declaration + '`'), file + ': README must describe ' + declaration);
    }
  }
});

test('Pactrun home follows the README structure with explanatory diagrams, not a tutorial', async () => {
  const current = await readFile(path.join(root, 'docs/index.md'), 'utf8');
  const released = await readFile(path.join(root, 'website/versioned_docs/version-1.0.0-alpha.9/index.md'), 'utf8');
  assert.equal(current, released, 'Both alpha.9-based home pages must show the reviewed content');
  assert.match(current, /^slug: \/$/m);
  const readme = await readFile(path.join(root, 'README.md'), 'utf8');
  const headings = text => [...text.matchAll(/^## (.+)$/gm)].map(match => match[1]);
  assert.deepEqual(headings(current), headings(readme));
  const comparison = current.split('## Where it fits\n')[1].split('## Install\n')[0];
  assert.deepEqual([...comparison.matchAll(/^### (.+)$/gm)].map(match => match[1]),
    ['Command interfaces and task runners', 'Service managers and container tools', 'Application management platforms']);
  for (const product of ['Taskfile.yml', 'YunoHost', 'Cloudron', 'Coolify']) assert.ok(comparison.includes(product));
  assert.doesNotMatch(current, /className="pactrun-home/);
  const install = current.split('## Install\n')[1].split('## Use a Pack\n')[0];
  assert.doesNotMatch(current.replace(install, ''), /^```/m, 'Only Install contains executable instructions');
  assert.equal([...install.matchAll(/^```(?:powershell|sh)$/gm)].length, 2);
  for (const text of ['Windows x86-64', 'glibc 2.36 or newer', 'Git',
    'scoop install pactrun/pactrun-preview', 'brew install pipapoen/pactrun/pactrun-preview',
    ':::warning Preview only', 'there is no Stable release yet']) assert.ok(install.includes(text), text);
  const images = [...current.matchAll(/!\[([^\]]+)\]\((\/img\/intro-alpha9\/[^)]+)\)/g)];
  assert.equal(images.length, 2);
  for (const [, alt, url] of images) {
    assert.ok(alt.length > 25, 'The explanation must also be available without the picture');
    const svg = await readFile(path.join(root, 'website/static', url.slice(1)), 'utf8');
    assert.match(svg, /<title\b/);
    assert.match(svg, /<desc\b/);
    assert.doesNotMatch(svg, /<script\b|\son\w+\s*=/i);
  }
  for (const destination of ['./guides/installation.md', './guides/index.md', './package-authors/index.md']) {
    assert.ok(current.includes('](' + destination + ')'), destination);
  }
  const sidebar = await readFile(path.join(root, 'website/sidebars.ts'), 'utf8');
  assert.ok(sidebar.includes("'guides/installation', 'introduction', 'guides/use-pack'"), 'Keep the original first-Instance tutorial route');
});
