import assert from 'node:assert/strict';
import {readFile, readdir} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import test from 'node:test';
import {readDocuments} from '../plugins/text-docs/index.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const docs = await readDocuments(path.join(root, 'docs'));

export function assertSpecOnlyDefinitions(documents) {
  const seen = new Set();
  for (const [name, body] of documents) {
    for (const match of body.matchAll(/^#{1,6} (PR-REQ-\d+)\b/gm)) {
      assert.ok(name.startsWith('spec/'), 'Definition outside Spec: ' + name);
      assert.ok(!seen.has(match[1]), 'Duplicate definition: ' + match[1]);
      seen.add(match[1]);
    }
  }
  return seen;
}

test('product requirements have unique definitions only under Spec', () => {
  assert.ok(assertSpecOnlyDefinitions(docs).size > 0);
});

test('outside and duplicate definitions fail; reference links are allowed', () => {
  assert.throws(() => assertSpecOnlyDefinitions([['development/a.md', '### PR-REQ-9999 - Example']]));
  assert.throws(() => assertSpecOnlyDefinitions([['spec/a.md', '### PR-REQ-9999 - Example'], ['spec/b.md', '### PR-REQ-9999 - Duplicate']]));
  assertSpecOnlyDefinitions([['development/a.md', '[PR-REQ-9999](../spec/a.md)']]);
});

test('catalog matches every requirement-bearing document and its original status', () => {
  const catalog = docs.find(([name]) => name === 'spec/catalog.md')[1];
  for (const [name, body] of docs) {
    if (!/^### PR-REQ-\d+/m.test(body)) continue;
    const status = body.match(/\*\*Status: ([\s\S]*?)\*\*/)?.[1].replace(/\s+/g, ' ').trim();
    assert.ok(status, 'Missing status: ' + name);
    const target = name.slice('spec/'.length);
    const line = catalog.split('\n').find(value => value.includes('](' + target + ')'));
    assert.ok(line, 'Missing catalog entry: ' + name);
    assert.ok(line.includes('| ' + status + ' |'), 'Stale catalog status: ' + name);
    assert.ok(body.includes('<!-- spec-navigation:start -->'), 'Missing reading map: ' + name);
  }
});

test('usage guides are placeholders, not completed tutorials', () => {
  const names = ['agents/use-pactrun.md', 'agents/author-packs.md', 'agents/integrate-hooks.md'];
  for (const [name, body] of docs) {
    if (!names.includes(name) && !name.startsWith('pactrun-users/') && !name.startsWith('package-authors/')) continue;
    assert.match(body, /placeholder|reserves the\s+documentation structure/i, name);
    assert.doesNotMatch(body, /^### PR-REQ-\d+/m, name);
  }
});

test('main sidebar uses Spec and Development instead of compatibility locations', async () => {
  const sidebar = await readFile(path.join(root, 'website/sidebars.ts'), 'utf8');
  assert.doesNotMatch(sidebar, /['"]pactrun-developers\//);
  assert.doesNotMatch(sidebar, /['"]agents\//);
  assert.match(sidebar, /spec\/index/);
  assert.match(sidebar, /development\/index/);
});

test('roadmap approval is not advanced by the documentation migration', () => {
  const roadmap = docs.find(([name]) => name === 'development/implementation-roadmap.md')[1];
  assert.match(roadmap, /### M5 - Migration\s+\*\*State: Proposed\.\*\*/);
});

test('roadmap distinguishes historical V4 introduction from the current schema', () => {
  const roadmap = docs.find(([name]) => name === 'development/implementation-roadmap.md')[1];
  assert.doesNotMatch(roadmap.replace(/\s+/g, ' '), /PersistenceSchemaV4 as the current canonical implemented internal schema/);
});

async function rustFiles(directory) {
  const files = [];
  for (const entry of await readdir(directory, {withFileTypes: true})) {
    const name = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...await rustFiles(name));
    else if (entry.isFile() && name.endsWith('.rs')) files.push(name);
  }
  return files;
}

test('embedded Markdown contracts use canonical Spec paths, not forwarding pages', async () => {
  for (const name of await rustFiles(path.join(root, 'src'))) {
    const source = await readFile(name, 'utf8');
    for (const match of source.matchAll(/include_str!\(\s*"([^"]*docs\/[^"]+\.md)"/g)) {
      assert.ok(match[1].includes('/docs/spec/'), 'Noncanonical embedded contract: ' + name);
      const contract = await readFile(path.resolve(path.dirname(name), match[1]), 'utf8');
      assert.match(contract, /\*\*Status:/);
    }
  }
});
