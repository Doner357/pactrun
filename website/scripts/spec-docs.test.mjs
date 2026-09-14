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

test('M5 integration remains distinct from separately approved M6 work', async () => {
  const roadmap = docs.find(([name]) => name === 'development/implementation-roadmap.md')[1];
  const baseline = docs.find(([name]) => name === 'development/design-notes/m5-migration-implementation-baseline.md')[1];
  const status = docs.find(([name]) => name === 'development/m5-implementation-status.md')[1];
  assert.match(roadmap, /### M5 - Migration\s+\*\*State: Implemented and integrated into develop\.\*\*/);
  assert.match(roadmap, /### M6 - Recovery\s+\*\*State: In progress\.\*\*/);
  assert.match(baseline, /Approved scope and implementation decisions, 2026-09-13/);
  assert.match(status.replace(/\s+/g, ' '), /approved M5 scope is implemented, verified and integrated into local develop/);
  assert.match(status, /Current production storage is V6/);
  assert.match(status.replace(/\s+/g, ' '), /Hook-backed chains and operator file inputs are integrated into the existing Migration Run path/);
  assert.match(status.replace(/\s+/g, ' '), /No ServiceStorage representation\/runtime, generalized M6 recovery/);
  const readme = await readFile(path.join(root, 'README.md'), 'utf8');
  assert.match(readme.replace(/\s+/g, ' '), /target-qualified operator file inputs, declarative and Hook-backed chains/);
  assert.doesNotMatch(readme, /in-progress M5|not full M5 completion|remain pending; this is/);
});

test('roadmap distinguishes historical V4 introduction from the current schema', () => {
  const roadmap = docs.find(([name]) => name === 'development/implementation-roadmap.md')[1];
  assert.doesNotMatch(roadmap.replace(/\s+/g, ' '), /PersistenceSchemaV4 as the current canonical implemented internal schema/);
});

test('bounded M6 approval does not approve the scheduled ServiceStorage runtime', async () => {
  const document = name => docs.find(([file]) => file === 'development/' + name)[1];
  const roadmap = document('implementation-roadmap.md');
  const milestones = [...roadmap.matchAll(/^### (M6(?:\.5)?|M7|M8) - /gm)].map(match => match[1]);
  assert.deepEqual(milestones, ['M6', 'M6.5', 'M7', 'M8']);
  assert.match(roadmap, /### M6 - Recovery\s+\*\*State: In progress\.\*\*/);
  assert.match(roadmap, /### M6\.5 - ServiceStorage\s+\*\*State: Proposed\./);
  const current = roadmap.split('## Current baseline')[1].split('## Milestone states')[0].replace(/\s+/g, ' ');
  assert.match(current, /PersistenceSchemaV6.*integrated `develop` persistence baseline/);
  assert.doesNotMatch(current, /Migration and Cleanup execution remain deferred/);
  assert.doesNotMatch(roadmap.replace(/\s+/g, ' '), /Hook and operator-input execution remain pending/);

  const baseline = document('design-notes/m6-recovery-implementation-baseline.md').replace(/\s+/g, ' ');
  assert.match(baseline, /Approved bounded M6 scope/);
  assert.match(baseline, /S0 review complete/);
  assert.match(baseline, /Decision: retain exact V6/);
  assert.match(baseline, /Do not silently alter exact V6/);
  const alignment = document('design-notes/service-storage-staged-design-alignment.md').replace(/\s+/g, ' ');
  assert.match(alignment, /before approving the storage schema/);
  assert.match(alignment, /broader taxonomy.*remain deferred and do not block/);

  for (const entry of ['index.md', 'reading-paths.md', 'next-milestone.md', 'implementation-guidance.md', 'design-notes/service-storage-semantic-baseline.md']) {
    assert.match(document(entry), /service-storage-staged-design-alignment\.md/, entry);
    assert.match(document(entry), /m6-recovery-implementation-baseline\.md/, entry);
  }
  const sidebar = await readFile(path.join(root, 'website/sidebars.ts'), 'utf8');
  assert.match(sidebar, /development\/design-notes\/service-storage-staged-design-alignment/);
  assert.match(sidebar, /development\/design-notes\/m6-recovery-implementation-baseline/);
  assert.match(sidebar, /development\/m6-implementation-status/);
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
