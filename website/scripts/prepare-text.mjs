import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {exportText} from '../plugins/text-docs/index.mjs';

const siteDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const result = await exportText({
  docsDir: path.resolve(siteDir, '../docs'),
  outDir: path.join(siteDir, '.generated-text'),
});
console.log('Prepared text edition:', result.count, 'documents; source SHA-256', result.digest);
