import assert from 'node:assert/strict';
import {mkdir, writeFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {editions} from '../plugins/editions.mjs';
import {documentDigest, readDocuments} from '../plugins/text-docs/index.mjs';

const [base, output] = process.argv.slice(2);
if (!base || !output) throw new Error('Provide the site base URL and a new evidence directory');
const site = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
await mkdir(output, {recursive: true});
const checked = [];
for (const edition of await editions(site)) {
  const expected = documentDigest(await readDocuments(edition.docsDir));
  const url = `${base.replace(/\/+$/, '')}/${edition.path}/`;
  const metadataResponse = await fetch(url + 'agent-docs/publication.json', {signal: AbortSignal.timeout(30000), cache: 'no-store'});
  assert.equal(metadataResponse.status, 200, url);
  const metadata = await metadataResponse.json();
  assert.equal(metadata.documentation_version, edition.id);
  assert.equal(metadata.source_digest, expected, `Stale text publication for ${edition.id}`);
  const catalogResponse = await fetch(url + 'document-catalog.json', {signal: AbortSignal.timeout(30000), cache: 'no-store'});
  assert.equal(catalogResponse.status, 200, url);
  assert.equal((await catalogResponse.json()).source_digest, expected, `Stale search index for ${edition.id}`);
  checked.push({edition: edition.id, source_digest: expected});
}
await writeFile(path.join(output, 'receipt.json'), JSON.stringify({status: 'Passed', base, checked}, null, 2) + '\n');
console.log(`Verified exact source digests for ${checked.length} documentation editions`);
