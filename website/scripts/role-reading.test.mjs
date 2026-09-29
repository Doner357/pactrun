import assert from 'node:assert/strict';
import test from 'node:test';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {readFile} from 'node:fs/promises';
import {readDocuments} from '../plugins/text-docs/index.mjs';
import {makeCatalog} from '../plugins/document-catalog/index.mjs';
import {searchPages} from '../src/lib/search.mjs';
import {references, schemaCopies, expectedReferences} from './reader-references.mjs';
import {readerRole, allowReaderNavigation} from '../src/lib/reader-role.mjs';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const docs = await readDocuments(path.join(root, 'docs'));
const bodies = new Map(docs);
const catalog = makeCatalog(docs);

test('reader navigation stays in role at root and project base paths; developer navigation stays complete', () => {
  for (const base of ['/', '/pactrun/']) {
    const url = suffix => base + suffix;
    assert.equal(readerRole(url('package-authors/reference/pack-fields'), base), 'Authors');
    assert.equal(readerRole(url('guides/'), base), 'Users');
    assert.equal(readerRole(url('introduction'), base), 'Users');
    assert.equal(readerRole(url('spec/'), base), null);
    assert.equal(readerRole(url('package-authors-other/'), base), null);
    assert.equal(allowReaderNavigation(url('package-authors/'), url('spec/'), base), false);
    assert.equal(allowReaderNavigation(url('pactrun-users/reference/snapshot-limits'), url('package-authors/'), base), false);
    assert.equal(allowReaderNavigation(url('guides/'), url('pactrun-users/reference/'), base), true);
    assert.equal(allowReaderNavigation(url('spec/'), url('development/'), base), true);
    assert.equal(allowReaderNavigation(url('guides/'), base, base), true);
  }
});

test('role reference views and user schemas exactly match their owning sources', () => {
  for (const [name, expected] of expectedReferences(docs)) assert.equal(bodies.get(name), expected, name + ': regenerate reader references');
  for (const entry of references) {
    const body = bodies.get(entry.output);
    assert.doesNotMatch(body, /^#{1,6} PR-REQ-\d+/m, entry.output);
    assert.doesNotMatch(body, /\*\*Verification:/, entry.output);
    const record = catalog.pages.find(page => page.source === entry.output);
    assert.equal(record.role, 'Reference');
    assert.equal(record.readerView, true);
    assert.deepEqual(record.audiences, [entry.audience]);
  }
  for (const entry of schemaCopies) assert.deepEqual(JSON.parse(bodies.get(entry.output)), JSON.parse(bodies.get(entry.source)));
});

test('non-developer primary reading flow has no Spec or Development dependency', () => {
  for (const [name, body] of docs) {
    if (!/^(guides\/|pactrun-users\/|package-authors\/|introduction\.md)/.test(name) || !name.endsWith('.md')) continue;
    for (const panel of body.matchAll(/<details>\s*<summary>Maintainer sources \(optional\)<\/summary>([\s\S]*?)<\/details>/g))
      assert.doesNotMatch(panel[1], /^#{1,6} |^```/m, name + ': task content hidden inside source panel');
    const primary = body.replace(/<details>[\s\S]*?<\/details>/g, '');
    for (const match of primary.matchAll(/\]\(([^)]+)\)/g)) {
      const target = match[1].split('#')[0];
      if (!target.startsWith('.')) continue;
      const resolved = path.posix.normalize(path.posix.join(path.posix.dirname(name), target));
      assert.ok(!/^(spec|development)\//.test(resolved), `${name}: primary link ${target}`);
    }
  }
});

test('author setup uses the same safe executable snippets as the user tutorial', () => {
  for (const language of ['powershell', 'sh']) {
    const code = text => text.match(new RegExp('^```' + language + '\\n([\\s\\S]*?)^```', 'm'))[1];
    assert.equal(code(bodies.get('package-authors/setup.md')), code(bodies.get('introduction.md')));
  }
});

test('reader views preserve every real source code block independently of projection equality', () => {
  const blocks = body => [...body.matchAll(/^```[^\n]*\n[\s\S]*?^```[ \t]*$/gm)].map(match => match[0]);
  for (const entry of references) assert.deepEqual(blocks(bodies.get(entry.output)), blocks(bodies.get(entry.source)), entry.output);
});

test('both reader roles explain exact invocation text and user maintenance states its store-wide scope', () => {
  const owner = bodies.get('spec/contracts/actions-inputs-and-parameters.md');
  const patterns = [String.raw`-?(0|[1-9][0-9]*)`, String.raw`-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?`];
  for (const pattern of patterns) {
    assert.ok(owner.includes(pattern));
    for (const name of ['pactrun-users/operations/invoke-reference.md', 'package-authors/reference/pack-fields.md'])
      assert.ok(bodies.get(name).includes(pattern), name);
  }
  for (const name of ['guides/recovery.md', 'guides/retirement.md']) {
    const text = bodies.get(name).replace(/\s+/g, ' ');
    assert.match(text, /store-wide/);
    assert.match(text, /selected store/);
  }
});

test('author field guide exposes choices, omissions and meanings for common declarations', () => {
  const guide = bodies.get('package-authors/reference/pack-fields.md');
  const service = bodies.get('package-authors/reference/service-fields.md');
  for (const word of ['required', 'protection', 'parameters', 'sensitive', 'default', 'protocol_version', 'launch', 'io.terminal', 'portable_metadata']) assert.ok(guide.includes('`' + word) || guide.includes('.' + word), word);
  for (const value of ['none', 'output', 'interactive', 'integer', 'float', 'boolean', 'string', 'normal', 'secret', 'direct', 'interpreter', 'shell_loader', 'powershell_7', 'windows_powershell_5_1']) assert.ok(guide.includes('`' + value + '`'), value);
  for (const value of ['read_exposure', 'user_mutation', 'service_access', 'service_requires', 'source_revision_digest', 'storage_transitions', 'resource_transitions', 'reuse', 'reattach', 'transform', 'declassify']) assert.ok(service.includes(value), value);
  assert.match(guide, /String `"1\.0-alpha\.1"`/);
  const sourceMarker = bodies.get('spec/contracts/pack-source.md').match(/source_format: "([^"]+)"/)[1];
  const hookMarker = bodies.get('spec/contracts/hook-protocol.md').match(/the supported value is `([^`]+)`/)[1];
  assert.ok(guide.split('| `source_format` |')[1].split('\n')[0].includes('"' + sourceMarker + '"'));
  assert.ok(guide.split('| `protocol_version` |')[1].split('\n')[0].includes('"' + hookMarker + '"'));
  assert.match(guide, /There is no default terminal mode/);
  assert.match(guide, /Present: operator may omit the parameter\. Absent: operator must supply it/);
});

test('task queries prefer reader guidance while exact requirement queries retain the Spec owner', () => {
  assert.equal(searchPages(catalog.pages, {q: 'terminal none', audience: 'Authors'})[0].source, 'package-authors/reference/pack-fields.md');
  assert.ok(searchPages(catalog.pages, {q: 'jsonl', audience: 'Users'})[0].source.startsWith('pactrun-users/'));
  assert.equal(searchPages(catalog.pages, {q: 'PR-REQ-0258', audience: 'Authors'})[0].source, 'spec/contracts/pack-source.md');
  assert.equal(searchPages(catalog.pages, {q: 'Product Versioning and Compatibility'})[0].source, 'spec/foundations/product-versioning-and-compatibility.md');
  for (const [q, audience] of [['hook protocol', 'Authors'], ['snapshot integrity', 'Users']]) {
    const results = searchPages(catalog.pages, {q, audience});
    const readerIndex = results.findIndex(page => ['Guide', 'Tutorial', 'Reference', 'Explanation'].includes(page.role));
    const specIndex = results.findIndex(page => page.role === 'Specification');
    assert.ok(readerIndex >= 0 && specIndex >= 0, q + ': fixture must cover both audiences');
    assert.ok(readerIndex < specIndex, q + ': title-only scoring must not bypass reader guidance');
  }
});

test('reader enum choices follow the owning declaration vocabulary', () => {
  const owner = bodies.get('spec/contracts/revision-canonical.md');
  for (const [name, keys] of [
    ['package-authors/reference/pack-fields.md', ['type', 'protection', 'access', 'terminal', 'shell']],
    ['package-authors/reference/service-fields.md', ['read_exposure', 'mode', 'view', 'role']],
  ]) {
    const guide = bodies.get(name);
    for (const key of keys) {
      const values = owner.match(new RegExp('\\b' + key + ': ([a-z0-9_]+(?: \\| [a-z0-9_]+)+)'))?.[1];
      assert.ok(values, 'Owner declaration changed: ' + key);
      for (const value of values.split(' | ')) assert.match(guide, new RegExp('\\b' + value + '\\b'), `${name}: missing ${key}=${value}`);
    }
  }
});

test('all reader references are reachable through the role sidebar and landing maps', async () => {
  const sidebar = await readFile(path.join(root, 'website/sidebars.ts'), 'utf8');
  assert.ok(sidebar.indexOf("'package-authors/fundamentals/authoring-model'") < sidebar.indexOf("'package-authors/fundamentals/actions-inputs-and-parameters'"));
  for (const entry of references) assert.ok(sidebar.includes("'" + entry.output.replace(/\.md$/, '') + "'"), entry.output);
  const authors = bodies.get('package-authors/index.md');
  const users = bodies.get('pactrun-users/reference/index.md');
  for (const entry of references) assert.ok((entry.audience === 'Users' ? users : authors).includes(path.posix.basename(entry.output)), entry.output);
});
