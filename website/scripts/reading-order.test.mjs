import assert from 'node:assert/strict';
import test from 'node:test';
import {readFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {readDocuments} from '../plugins/text-docs/index.mjs';
import {classify} from '../plugins/document-catalog/model.mjs';
import {readingSequences, readingNavigation} from '../src/lib/reading-order.mjs';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const documents = await readDocuments(path.join(root, 'docs'));
const docs = new Map(documents);

test('guided routes have existing prerequisites, consistent sidebar order and explicit next destinations', async () => {
  const sidebar = await readFile(path.join(root, 'website/sidebars.ts'), 'utf8');
  for (const sequence of Object.values(readingSequences)) {
    assert.equal(new Set(sequence).size, sequence.length);
    let previousPosition = -1;
    for (const [index, name] of sequence.entries()) {
      assert.ok(docs.has(name), name);
      const position = sidebar.indexOf("'" + name.replace(/\.md$/, '') + "'");
      assert.ok(position > previousPosition, name + ': sidebar order contradicts route');
      previousPosition = position;
      const navigation = readingNavigation(name);
      assert.equal(navigation.previous, sequence[index - 1]);
      assert.equal(navigation.next, sequence[index + 1]);
      if (navigation.next) {
        const relative = path.posix.relative(path.posix.dirname(name), navigation.next);
        assert.ok(docs.get(name).includes(relative), name + ': body does not point to the next step');
      }
    }
  }
  assert.ok(readingSequences.Users.indexOf('guides/installation.md') < readingSequences.Users.indexOf('introduction.md'));
});

test('independent tasks, references and records do not become an automatic execution sequence', () => {
  for (const [name, body] of documents) {
    const state = classify(name, body).state;
    const navigation = readingNavigation(name, state);
    for (const target of [navigation.previous, navigation.next, navigation.back]) if (target) assert.ok(docs.has(target), name + ': missing destination');
    if (state !== 'current') assert.equal(navigation.kind, 'record', name);
  }
  for (const name of ['guides/recovery.md', 'guides/retirement.md', 'guides/configure-inputs.md',
    'package-authors/managed-capabilities/migrations.md', 'package-authors/managed-capabilities/snapshots-and-managed-data.md',
    'spec/contracts/pack-source.md', 'pactrun-users/reference/machine-output.md']) {
    assert.equal(readingNavigation(name).kind, 'lookup', name);
    assert.equal(readingNavigation(name).next, undefined, name);
  }
});

test('installation promises an executable procedure and separates data-location work', () => {
  const install = docs.get('guides/installation.md');
  for (const text of ['Get-FileHash', 'Expand-Archive', 'sha256sum', 'tar -xzf', 'PASTE_TRUSTED_SHA256', '--version', '--help']) assert.ok(install.includes(text), text);
  assert.ok(install.includes('data-location.md'));
  assert.ok(install.includes('authoring workspace'));
  assert.ok(docs.get('guides/data-location.md').includes('XDG_DATA_HOME'));
  assert.ok(docs.get('guides/use-pack.md').includes('pactrun instance create demo --revision <reference>'));
});

test('the empty tutorial and later task prerequisites cannot be confused with a live service fixture', () => {
  assert.match(docs.get('introduction.md').replace(/\s+/g, ' '), /no longer a live Instance/);
  for (const name of ['guides/configure-inputs.md', 'guides/run-and-diagnose.md', 'guides/service-resources.md']) assert.ok(docs.get(name).includes('use-pack.md'), name);
  assert.match(docs.get('guides/index.md'), /not a sequence of commands/);
  assert.match(docs.get('package-authors/index.md').replace(/\s+/g, ' '), /Neither assumes that you have completed the other/);
});

test('repository and developer entry points describe delivered guides, not obsolete placeholders', async () => {
  for (const name of ['CONTRIBUTING.md', 'docs/development/index.md']) {
    const body = (await readFile(path.join(root, name), 'utf8')).replace(/\s+/g, ' ');
    assert.doesNotMatch(body, /guides remain (?:placeholders|reserved)|guides for users.*remain placeholders|wait for baseline consolidation/i, name);
  }
  const entry = docs.get('agents/index.md');
  assert.ok(entry.indexOf('Choose a task') < entry.indexOf('For product implementation'));
});
