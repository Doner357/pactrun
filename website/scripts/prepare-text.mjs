import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {exportCatalog} from '../plugins/document-catalog/index.mjs';
import {exportText} from '../plugins/text-docs/index.mjs';
import {editions} from '../plugins/editions.mjs';
import {readFile, writeFile, rm, copyFile} from 'node:fs/promises';

const siteDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const all = await editions(siteDir);
const aliases = JSON.parse(await readFile(path.join(siteDir, 'spec-source-aliases.json'), 'utf8'));
const baseUrl = process.env.DOCUSAURUS_BASE_URL ?? '/';
const output = path.resolve(siteDir, '.generated-text');
if (path.dirname(output) !== siteDir) throw new Error('Invalid generated output directory');
await rm(output, {recursive: true, force: true});
for (const edition of all) {
  const result = await exportText({docsDir: edition.docsDir,
    outDir: path.join(siteDir, '.generated-text', edition.path), version: edition.id, baseUrl, aliases});
  await exportCatalog(siteDir, edition);
  console.log('Prepared', edition.label, result.count, 'documents; SHA-256', result.digest);
}
// Unversioned text/API bookmarks resolve to the latest release, never development.
await exportText({docsDir: all[0].docsDir, outDir: output, version: all[0].id, baseUrl, aliases});
for (const name of ['document-catalog.json', 'command-help.json']) {
  await copyFile(path.join(output, all[0].path, name), path.join(output, name));
}
// Preserve the public text entry, but explicitly direct readers to an edition.
await writeFile(path.join(siteDir, '.generated-text', 'llms.txt'),
  '# Pactrun documentation\n\n' + all.map(edition =>
    `- [${edition.label}](./${edition.path}/llms.txt)`).join('\n') + '\n');
