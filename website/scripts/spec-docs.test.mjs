import assert from 'node:assert/strict';
import {readFile, readdir} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import test from 'node:test';
import {readDocuments} from '../plugins/text-docs/index.mjs';
import {classify} from '../plugins/document-catalog/model.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const docs = await readDocuments(path.join(root, 'docs'));

test('Spec pages explain the product without stage labels or development-reading prerequisites', () => {
  for (const [name, source] of docs.filter(([name]) => name.startsWith('spec/') && name.endsWith('.md'))) {
    const prose = source.replace(/\]\([^)]*\)/g, ']').replace(/\{#[^}]+\}/g, '').replace(/<!--[^]*?-->/g, '').replace(/<a id="[^"]+"\s*\/>/g, '');
    assert.doesNotMatch(prose, /\bM\d+(?:\.\d+)?(?:-[A-Z])?\b|\bmilestone\b|\bpre-E\b/i, name);
    assert.doesNotMatch(prose, /\b(?:normative|informative)\b/i, name);
    assert.doesNotMatch(source, /\]\([^)]*development\//, name);
  }
});

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

test('requirement links in compatibility indexes use current owner titles', () => {
  const titles = new Map(docs.filter(([name]) => name.startsWith('spec/')).flatMap(([, body]) =>
    [...body.matchAll(/^### (PR-REQ-\d+) - (.+)$/gm)].map(([, id, title]) =>
      [id, title.replace(/\s*\{#[^}]+\}\s*$/, '')])));
  for (const [name, body] of docs.filter(([name]) => name.startsWith('pactrun-developers/'))) {
    if (/^## Section links/m.test(body)) assert.match(body, /^## Section links \{#previous-section-links\}$/m, name);
    if (/^## Requirement links/m.test(body)) assert.match(body, /^## Requirement links \{#previous-requirement-links\}$/m, name);
    for (const [, label, targetId] of body.matchAll(/\[(PR-REQ-\d+ - [^\]]+)\]\([^)]*#pr-req-(\d+)[^)]*\)/g)) {
      const id = 'PR-REQ-' + targetId;
      assert.equal(label, id + ' - ' + titles.get(id), name);
    }
  }
});

test('source documentation keeps Hook and source format domains independent', () => {
  const get = name => docs.find(([file]) => file === name)[1];
  const hook = get('spec/interfaces/hook-protocol.md').match(/supported value is `([^`]+)`/)[1];
  const source = get('spec/packages/source-format.md');
  const selected = source.match(/Hook `protocol_version`[^]*?current supported value is `"([^"]+)"`/)[1];
  assert.equal(selected, hook);
  assert.doesNotMatch(source, /A reference-label semantic key|metadata plan contains reference-label/);
  assert.doesNotMatch(get('spec/packages/authoring.md'), /ReferenceLabelMetadata|Reference-label projection/);
  assert.doesNotMatch(get('spec/packages/distribution.md'), /label-source|portable\s+labels|Label\/provenance/);
});

test('current reference and Store-opening prose does not retain replaced rules', () => {
  for (const [name, body] of docs) {
    if (name.startsWith('pactrun-developers/')) continue;
    const prose = body.replace(/```[^]*?```/g, '').replace(/\{#[^}]+\}/g, '');
    assert.doesNotMatch(prose, /references retain their exact\/label\/alias grammar/i, name);
    assert.doesNotMatch(prose, /Read-only opening cannot initialize or upgrade storage/i, name);
    if (name.startsWith('pactrun-users/concepts/')) {
      assert.doesNotMatch(prose, /Labels and aliases are useful\s+lookup names/i, name);
    }
  }
});

test('outside and duplicate definitions fail; reference links are allowed', () => {
  assert.throws(() => assertSpecOnlyDefinitions([['development/a.md', '### PR-REQ-9999 - Example']]));
  assert.throws(() => assertSpecOnlyDefinitions([['spec/a.md', '### PR-REQ-9999 - Example'], ['spec/b.md', '### PR-REQ-9999 - Duplicate']]));
  assertSpecOnlyDefinitions([['development/a.md', '[PR-REQ-9999](../spec/a.md)']]);
});

export function assertBidirectionalVerification(documents, sources) {
  const requirements = new Map();
  for (const [name, body] of documents) {
    if (!name.startsWith('spec/')) continue;
    const sections = [...body.matchAll(/^### (PR-REQ-\d+)\b/gm)];
    for (const [index, section] of sections.entries()) {
      const id = section[1];
      assert.ok(!requirements.has(id), 'Duplicate requirement: ' + id);
      const text = body.slice(section.index, sections[index + 1]?.index ?? body.length);
      const ids = new Set();
      for (const match of text.matchAll(/^\*\*Verification: ([\s\S]*?)\*\*/gm)) {
        for (const test of match[1].matchAll(/PR-TEST-\d+/g)) ids.add(test[0]);
      }
      // Some existing requirements use a bold label followed by a prose paragraph.
      for (const match of text.matchAll(/^\*\*Verification:\*\*([^\n]*(?:\n(?!\n|#)[^\n]*)*)/gm)) {
        for (const test of match[1].matchAll(/PR-TEST-\d+/g)) ids.add(test[0]);
      }
      requirements.set(id, ids);
    }
  }
  const tests = new Map();
  for (const [name, body] of sources) {
    for (const match of body.matchAll(/^[ \t]*\/\/ Test-ID: (PR-TEST-\d+)\r?\n([^\n]*)/gm)) {
      const id = match[1];
      assert.ok(!tests.has(id), 'Duplicate test: ' + id);
      assert.match(match[2], /^[ \t]*\/\/ Verifies: /, 'Missing Verifies metadata in ' + name);
      const owners = new Set([...match[2].matchAll(/PR-REQ-\d+/g)].map(value => value[0]));
      assert.ok(owners.size, 'No owner for ' + id);
      tests.set(id, owners);
    }
  }
  for (const [requirement, ids] of requirements) {
    for (const id of ids) assert.ok(tests.get(id)?.has(requirement), requirement + ' has no reverse evidence link from ' + id);
  }
  for (const [id, owners] of tests) {
    for (const requirement of owners) assert.ok(requirements.get(requirement)?.has(id), id + ' has no owning verification citation in ' + requirement);
  }
  return {requirements: requirements.size, tests: tests.size};
}

async function verificationSources(directory) {
  const files = [];
  for (const entry of await readdir(path.join(root, directory), {withFileTypes: true})) {
    const name = path.posix.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...await verificationSources(name));
    else if (/\.(rs|mjs)$/.test(name)) files.push([name, await readFile(path.join(root, name), 'utf8')]);
  }
  return files;
}

test('verification citations and actual Rust/Node test declarations are bidirectional', async () => {
  const sources = (await Promise.all(['src', 'tests', 'xtask/src'].map(verificationSources))).flat();
  const result = assertBidirectionalVerification(docs, sources);
  assert.ok(result.requirements > 0 && result.tests > 0);
});

test('traceability rejects missing links and duplicate IDs without requiring contiguous numbering', () => {
  const document = '### PR-REQ-0001 - One\n\n**Verification: PR-TEST-0003.**\n\n### PR-REQ-0007 - Seven\n\n**Verification: Pending automated coverage.**\n';
  const source = '// Test-ID: PR-TEST-0003\n// Verifies: PR-REQ-0001\n';
  const documents = [['spec/example.md', document]];
  const sources = [['src/example.rs', source]];
  assert.deepEqual(assertBidirectionalVerification(documents, sources), {requirements: 2, tests: 1});
  assert.throws(() => assertBidirectionalVerification(documents, []));
  assert.throws(() => assertBidirectionalVerification([['spec/example.md', document.replace('**Verification: PR-TEST-0003.**', 'Historical mention: PR-TEST-0003.')]], sources));
  assert.throws(() => assertBidirectionalVerification(documents, [['src/example.rs', source.replace('PR-REQ-0001', 'PR-REQ-0007')]]));
  assert.throws(() => assertBidirectionalVerification(documents, [...sources, ['tests/duplicate.rs', source]]));
  assert.throws(() => assertBidirectionalVerification([...documents, ['spec/duplicate.md', document]], sources));
  const prose = document.replace('**Verification: PR-TEST-0003.**', '**Verification:** PR-TEST-0003 proves this case.\nNo new assertion is implied.\n');
  assertBidirectionalVerification([['spec/example.md', prose]], sources);
});

test('reference index covers every requirement owner without status boilerplate', () => {
  const catalog = docs.find(([name]) => name === 'spec/catalog.md')[1];
  for (const [name, body] of docs) {
    if (!/^### PR-REQ-\d+/m.test(body)) continue;
    assert.equal(classify(name, body).role, 'Specification', 'Requirements must belong to a specification: ' + name);
    const target = name.slice('spec/'.length);
    const line = catalog.split('\n').find(value => value.includes('](./' + target + ')'));
    assert.ok(line, 'Missing catalog entry: ' + name);
    assert.match(body, /^# .+/m, 'Missing document title: ' + name);
    assert.doesNotMatch(body, /^\*\*Status:/m, 'Reader-facing classification: ' + name);
  }
});

test('Complete baseline SQL remains identical to the owning Persistence specification', async () => {
  const spec = docs.find(([name]) => name === 'spec/persistence/persistence-baseline.md')[1];
  const sql = await readFile(path.join(root, 'src/persistence/persistence_baseline.sql'), 'utf8');
  assert.equal(spec.split('```sql\n')[1].split('```')[0].trim(), sql.trim());
});

test('usage guides are substantive, linked, and non-normative', () => {
  const names = ['agents/use-pactrun.md', 'agents/author-packs.md', 'agents/integrate-hooks.md'];
  for (const [name, body] of docs) {
    if (!names.includes(name) && !name.startsWith('pactrun-users/') && !name.startsWith('package-authors/')) continue;
    if (!name.endsWith('.md')) continue; // JSON schemas have separate exact-source checks.
    assert.doesNotMatch(body, /Planned usage-guide placeholder|guidance is planned|guided example are planned/i, name);
    assert.ok(body.length > 300, name);
    assert.match(body, /\[[^\]]+\]\([^)]+\.md(?:#[^)]*)?\)/, name);
    assert.doesNotMatch(body, /^### PR-REQ-\d+/m, name);
  }
});

test('main sidebar exposes product topics instead of compatibility locations', async () => {
  const sidebar = await readFile(path.join(root, 'website/sidebars.ts'), 'utf8');
  assert.doesNotMatch(sidebar, /['"]pactrun-developers\//);
  assert.doesNotMatch(sidebar, /['"]agents\//);
  assert.match(sidebar, /spec\/index/);
});





test('service contracts preserve source consumption, authority and persistence boundaries', async () => {
  const get = name => docs.find(([file]) => file === 'spec/' + name)?.[1];
  for (const name of ['packages/revision-format.md', 'packages/source-format.md', 'interfaces/hook-protocol.md', 'persistence/persistence-baseline.md', 'instances/service-storage.md', 'instances/resource-commands.md']) {
    const body = get(name);
    assert.ok(body, name);
    const rules = body.split(/^### PR-REQ-\d+/m).slice(1);
    assert.ok(rules.length > 0, name);
    for (const rule of rules) assert.match(rule, /\*\*Verification:/);
    assert.doesNotMatch(body, /^\*\*Status:/m);
  }
  assert.match(get('packages/revision-format.md'), /A source named by reuse, reattach or transform is consumed/);
  assert.match(get('instances/service-storage.md'), /without deleting bytes/);
  assert.match(get('instances/service-storage.md'), /Ambiguous Cleanup completion authorizes neither replay/);
  assert.match(get('interfaces/hook-protocol.md'), /service_authorities: ServiceAuthorityV2\[\]/);
  assert.match(get('interfaces/hook-protocol.md'), /target_ready_received/);
  assert.match(get('instances/resource-commands.md'), /Registered|registered/);
  const current = await readFile(path.join(root, 'src/persistence/sqlite_revision_store.rs'), 'utf8');
  assert.doesNotMatch(current, /SCHEMA_LADDER|SCHEMA_V9_VERSION|legacy_v4/);
  const marker = current.match(/pub\(crate\) const SCHEMA_VERSION: i64 = (\d+);/)?.[1];
  assert.ok(marker, 'The writer must declare its private schema marker');
  const persistence = get('persistence/persistence-baseline.md').replace(/\s+/g, ' ');
  assert.ok(persistence.includes('private bootstrap marker ' + marker), 'Spec and writer markers must agree');
  assert.ok(persistence.includes('CHECK(admitted_schema_version = ' + marker + ')'), 'Writer admission must use the same marker');
  assert.match(persistence, /Unsupported stores are not upgraded/);
  assert.match(persistence, /reopening MUST preserve exact Snapshot identities/);
  assert.match(persistence, /schemas are unsupported and no storage upgrade command remains/);
});

test('compatibility preserves supported meaning, data and independent release eligibility', () => {
  const policy = docs.find(([file]) => file === 'spec/storage/compatibility.md')[1];
  const text = policy.replace(/\s+/g, ' ');
  assert.deepEqual([...policy.matchAll(/^### (PR-REQ-\d+)/gm)].map(match => match[1]), [
    'PR-REQ-0329', 'PR-REQ-0330', 'PR-REQ-0331', 'PR-REQ-0332', 'PR-REQ-0333']);
  assert.doesNotMatch(policy, /Verification: Pending automated coverage/);
  const tests = new Set(policy.split('### PR-REQ-0333')[1].match(/PR-TEST-\d+/g));
  for (const id of ['0620','0626','0627','0628','0629','0630','0631','0635','0636','0637','0640','0641']) assert.ok(tests.has('PR-TEST-'+id));
  assert.match(text, /earlier published external contracts of that Major/);
  assert.match(text, /does not promise compatibility with a previous Major's external formats/);
  assert.match(text, /Data acceptance MUST depend on full conformance to supported contracts/);
  assert.match(text, /Fully conforming data MUST NOT be rejected solely because it was produced during development/);
  assert.match(text, /MUST NOT require an additional generation marker/);
  assert.match(text, /Conformance MUST include identity, encoding, reference, invariant, and semantic checks/);
  assert.match(text, /MUST NOT retain readers, migration chains, dispatch branches, or special cases solely for unsupported development contracts/);
  assert.match(text, /MUST NOT automatically publish or relabel a candidate as formal `1.0.0`/);
  assert.match(text, /MUST NOT reset formats or rewrite existing object identities/);
  assert.match(text, /No formal candidate MUST NOT fall back to prerelease/);
});

test('current authoring keeps reproducibility and internal Candidate rules without withdrawn proposals', () => {
  const get = name => docs.find(([file]) => file === name)[1];
  const runtime = get('spec/packages/runtime-content.md');
  const authoring = get('spec/packages/authoring.md');
  for (const id of ['0137','0139','0140','0257','0270','0276','0300','0312','0325','0339']) {
    for (const [name,body] of docs.filter(([name]) => name.startsWith('spec/'))) assert.doesNotMatch(body, new RegExp('^### PR-REQ-'+id, 'm'), name);
  }
  assert.match(runtime.replace(/\s+/g, ' '), /same canonical Revision Core and the same owned runtime content produce the same `RevisionContentDigest`/);
  assert.match(runtime, /Pactrun MUST materialize runtime content/);
  assert.match(authoring.replace(/\s+/g, ' '), /Every authoring frontend MUST ultimately produce a `RevisionCandidate`/);
  assert.match(authoring, /not a public frontend API/);
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
      assert.match(contract, /^### PR-REQ-\d+/m);
      assert.doesNotMatch(contract, /# (?:Compatibility entry|Retired development)/);
    }
  }
});

test('format owners retain established anchors without displaying editorial classifications', async () => {
  const config = await readFile(path.join(root, 'website/docusaurus.config.ts'), 'utf8');
  assert.match(config, /onBrokenAnchors:\s*'throw'/);
  for (const [name, patterns] of [
    ['interfaces/hook-protocol.md', ['{#reading-map-informative}', '## Protocol lifecycle']],
    ['packages/revision-format.md', ['## Format lifecycle']],
    ['snapshots/integrity.md', ['## Format lifecycle']],
  ]) {
    const body = docs.find(([file]) => file === 'spec/' + name)?.[1];
    assert.ok(body, name);
    for (const pattern of patterns) assert.ok(body.includes(pattern), name + ': ' + pattern);
    assert.doesNotMatch(body, /^## Reading map \(informative\)/m);
  }
});
