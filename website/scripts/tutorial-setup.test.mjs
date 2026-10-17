import assert from 'node:assert/strict';
import test from 'node:test';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {editions} from '../plugins/editions.mjs';
import {readDocuments} from '../plugins/text-docs/index.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
for (const directory of ['docs']) {
  test(`${directory}: reader sections retain references and Hello Pack lessons`, async () => {
    const documents = new Map(await readDocuments(path.join(root, directory)));
    assert.ok(!documents.has('introduction.md'));
    const exceptions = new Set(['guides/index.md', 'package-authors/index.md',
      'package-authors/tutorial/first-hello-pack/intro.md',
      'package-authors/tutorial/first-hello-pack/understanding-a-pack.md',
      'package-authors/tutorial/first-hello-pack/installing-the-first-revision.md',
      'pactrun-users/operations/command-and-output-reference.md',
      'pactrun-users/operations/invoke-reference.md']);
    for (const name of documents.keys()) {
      if (/^(guides|package-authors|pactrun-users)\//.test(name))
        assert.ok(name.includes('/reference/') || exceptions.has(name), name);
    }
    for (const name of ['package-authors/reference/pack-fields.md',
      'package-authors/reference/service-fields.md', 'package-authors/reference/shell-loader.md',
      'pactrun-users/reference/index.md', ...exceptions]) assert.ok(documents.has(name), name);
    const intro = documents.get('package-authors/tutorial/first-hello-pack/intro.md');
    assert.match(intro, /Service Storage/);
    assert.match(intro, /Gitea/);
  });
}

test('released alpha.9 keeps its original tutorial routes', async () => {
  const documents = new Map(await readDocuments(path.join(root, 'website/versioned_docs/version-1.0.0-alpha.9')));
  assert.ok(documents.has('introduction.md'));
  assert.ok(documents.has('package-authors/fundamentals/authoring-model.md'));
  assert.ok(!documents.has('package-authors/tutorial/first-hello-pack/intro.md'));
});

test('tutorial Pack source examples use a format supported by their edition', async () => {
  const site = path.join(root, 'website');
  for (const edition of await editions(site)) {
    const documents = new Map(await readDocuments(edition.docsDir));
    const tutorial = [...documents].filter(([name]) => name.startsWith('package-authors/tutorial/'));
    if (tutorial.length === 0) continue;

    const formats = documents.get('spec/storage/format-domains.md')
      ?.match(/Pack source writes `([^`]+)` and also reads `([^`]+)`/);
    assert.ok(formats, `${edition.id}: Pack source support must be stated in format-domains.md`);
    const supported = new Set(formats.slice(1));
    let examples = 0;
    for (const [name, body] of tutorial) {
      for (const block of body.matchAll(/^```(?:yaml|yml)[^\n]*\n([\s\S]*?)^```/gm)) {
        for (const marker of block[1].matchAll(/^source_format:\s*"([^"\n]+)"\s*$/gm)) {
          examples++;
          assert.ok(supported.has(marker[1]),
            `${edition.id}/${name}: source_format ${marker[1]} is unsupported; expected ${[...supported].join(' or ')}`);
        }
      }
    }
    assert.ok(examples > 0, `${edition.id}: tutorial must include a Pack source format example`);
  }
});
