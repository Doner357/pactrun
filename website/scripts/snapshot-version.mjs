import {execFileSync} from 'node:child_process';
import {createRequire} from 'node:module';
import {readFile, writeFile, mkdir} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {commandHelp} from '../plugins/document-catalog/index.mjs';

const siteDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const root = path.dirname(siteDir);
const [version, ...extra] = process.argv.slice(2);
if (!version || extra.length) throw new Error('Usage: pnpm docs:version <Pactrun release version>');
const help = await commandHelp(root);
if (version !== help.version) throw new Error('The documentation version must match Cargo.toml');
const versions = JSON.parse(await readFile(path.join(siteDir, 'versions.json'), 'utf8'));
if (versions.includes(version)) throw new Error('Documentation version already exists');
const changed = execFileSync('git', ['status', '--porcelain', '--', 'docs', 'website/sidebars.ts', 'src/cli.rs', 'src/cli/help.rs', 'src/cli/help_catalog.json', 'Cargo.toml'], {cwd: root, encoding: 'utf8'});
if (changed.trim()) throw new Error('Commit the reviewed documentation, sidebar and CLI inputs before capturing a release');
const source_commit = execFileSync('git', ['rev-parse', 'HEAD'], {cwd: root, encoding: 'utf8'}).trim();
const require = createRequire(import.meta.url);
const cli = path.join(path.dirname(require.resolve('@docusaurus/core/package.json')), 'bin/docusaurus.mjs');
execFileSync(process.execPath, [cli, 'docs:version', version], {cwd: siteDir, stdio: 'inherit'});
await mkdir(path.join(siteDir, 'versioned_help'), {recursive: true});
await writeFile(path.join(siteDir, 'versioned_help', version + '.json'),
  JSON.stringify({...help, source_commit, release_tag: 'v' + version}, null, 2) + '\n');
