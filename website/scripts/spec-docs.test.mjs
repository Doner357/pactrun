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
  assert.match(roadmap, /### M6 - Recovery\s+\*\*State: Complete\.\*\*/);
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

test('M6 integration closes recovery without approving the scheduled ServiceStorage runtime', async () => {
  const document = name => docs.find(([file]) => file === 'development/' + name)[1];
  const roadmap = document('implementation-roadmap.md');
  const milestones = [...roadmap.matchAll(/^### (M6(?:\.5)?|M7|M8) - /gm)].map(match => match[1]);
  assert.deepEqual(milestones, ['M6', 'M6.5', 'M7', 'M8']);
  assert.match(roadmap, /### M6 - Recovery\s+\*\*State: Complete\.\*\*/);
  assert.match(roadmap, /### M6\.5 - ServiceStorage\s+\*\*State: Complete\./);
  assert.match(roadmap, /### M7 - Cleanup and deletion\s+\*\*State: Proposed\./);
  const current = roadmap.split('## Current baseline')[1].split('## Milestone states')[0].replace(/\s+/g, ' ');
  assert.match(current, /PersistenceSchemaV7.*integrated `develop` persistence baseline/);
  assert.doesNotMatch(current, /Migration and Cleanup execution remain deferred/);
  assert.doesNotMatch(roadmap.replace(/\s+/g, ' '), /Hook and operator-input execution remain pending/);

  const baseline = document('design-notes/m6-recovery-implementation-baseline.md').replace(/\s+/g, ' ');
  assert.match(baseline, /Approved bounded M6 scope/);
  assert.match(baseline, /S0 review complete/);
  assert.match(baseline, /Decision: retain exact V6/);
  assert.match(baseline, /Do not silently alter exact V6/);
  const closeout = document('m6-implementation-status.md').replace(/\s+/g, ' ');
  assert.match(closeout, /Status: Complete\. Bounded M6 S0-S4 is implemented and integrated into local develop/);
  assert.match(closeout, /explicitly without push/);
  assert.match(document('next-milestone.md'), /M6\.5 ServiceStorage is complete; M7 is next/);
  assert.doesNotMatch(document('next-milestone.md'), /M6 is `In progress`/);
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

test('M6.5 design direction preserves the separate runtime and Freeze gates', async () => {
  const proposals = [
    'contracts/revision-core-format-v2.md',
    'contracts/pack-source-yaml-v2.md',
    'contracts/hook-protocol-v2.md',
    'persistence/persistence-schema-v7.md',
    'execution/m6-5-service-storage-execution.md',
    'behavior/m6-5-service-storage-command-reference.md',
  ];
  const baseline = docs.find(([name]) => name === 'development/design-notes/m6-5-servicestorage-baseline.md')[1];
  assert.match(baseline.replace(/\s+/g, ' '), /S1-S7 may proceed/);
  const core = docs.find(([file]) => file === 'spec/contracts/revision-core-format-v2.md')[1];
  assert.match(core, /A source named by reuse, reattach or transform is consumed/);
  for (const name of proposals) {
    const body = docs.find(([file]) => file === 'spec/' + name)?.[1];
    assert.ok(body, 'Missing S0 proposal: ' + name);
    if (name === 'contracts/revision-core-format-v2.md' || name === 'contracts/hook-protocol-v2.md') {
      assert.match(body, /Status: Frozen normative Package contract specification/);
      assert.match(body, /m6-5-format-activation-review\.md/);
    } else if (name === 'contracts/pack-source-yaml-v2.md') {
      assert.match(body, /Candidate normative Package authoring contract; versioned, non-Frozen/);
    } else {
      assert.match(body, /Status: Implemented normative/);
    }
    assert.match(baseline, new RegExp(name.replaceAll('.', '\\.')));
    const rules = body.split(/^### PR-REQ-\d+/m).slice(1);
    assert.ok(rules.length > 0, 'No proposed requirements: ' + name);
    for (const rule of rules) assert.match(rule, /\*\*Verification:/);
    if (name === 'contracts/hook-protocol-v2.md') {
      assert.match(rules[0], /Codec, authority and ordinary-runtime evidence/);
      assert.match(rules[1], /Protocol and target-runtime evidence/);
      assert.match(body, /no persisted proposal flag exists/);
      assert.match(body.replace(/\s+/g, ' '), /Codec tests alone are not completed Migration target-publication evidence/);
    }
    if (name === 'execution/m6-5-service-storage-execution.md') {
      assert.match(rules[0], /\*\*Verification: PR-TEST-0354, PR-TEST-0355, PR-TEST-0366, PR-TEST-0367, PR-TEST-0368, PR-TEST-0370, PR-TEST-0371, PR-TEST-0373, PR-TEST-0374, PR-TEST-0375, PR-TEST-0376, PR-TEST-0377, PR-TEST-0378, PR-TEST-0380, PR-TEST-0381, PR-TEST-0382, PR-TEST-0383, PR-TEST-0386, PR-TEST-0387, PR-TEST-0388\.\*\*/);
      assert.match(rules[0], /Filesystem observation substrate coverage/);
      assert.match(rules[0], /final-source closeout results/);
      assert.match(rules[1], /M6\.5 association\/lifetime evidence only/);
      assert.match(rules[1].replace(/\s+/g, ' '), /do not implement M7 Cleanup/);
    }
    if (name === 'behavior/m6-5-service-storage-command-reference.md') {
      assert.match(body, /available in the integrated M6\.5 baseline/);
      assert.match(body, /CLI runtime coverage/);
      assert.match(body, /production V2 installation, cross-version retention and explicit reattachment/);
    }
  }
  const current = await readFile(path.join(root, 'src/persistence/sqlite_revision_store.rs'), 'utf8');
  assert.match(current, /pub\(crate\) const SCHEMA_VERSION: i64 = 7;/);
  const roadmap = docs.find(([name]) => name === 'development/implementation-roadmap.md')[1];
  assert.match(roadmap, /### M6\.5 - ServiceStorage\s+\*\*State: Complete\./);
  const closeout = docs.find(([name]) => name === 'development/m6-5-implementation-status.md')[1];
  assert.match(closeout.replace(/\s+/g, ' '), /Status: Complete\. M6\.5 S1-S7 is implemented and integrated into local develop/);
  assert.match(closeout, /cf7562d77201d1e0cf29f298101cc4116a51a72b/);
  assert.match(closeout, /without push/);
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
