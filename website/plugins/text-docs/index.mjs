import {createHash} from 'node:crypto';
import {mkdir, readFile, readdir, rm, writeFile} from 'node:fs/promises';
import path from 'node:path';
import {classify, contextNotice} from '../document-catalog/model.mjs';
import {editions} from '../editions.mjs';

const omitted = new Set(['archive', 'proposals']);

export async function readDocuments(root, relative = '') {
  const documents = [];
  for (const entry of await readdir(path.join(root, relative), {withFileTypes: true})) {
    const name = path.posix.join(relative, entry.name);
    if (entry.isSymbolicLink()) throw new Error('Documentation symlinks are not supported: ' + name);
    if (entry.isDirectory() && !omitted.has(name.split('/')[0])) {
      documents.push(...await readDocuments(root, name));
    } else if (entry.isFile() && (name.endsWith('.md') || name.endsWith('.schema.json'))) {
      documents.push([name, (await readFile(path.join(root, name), 'utf8')).replaceAll('\r\n', '\n')]);
    }
  }
  return documents.sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0);
}

export function markdownBody(source) {
  // Keep the original body, including exact contract blocks and admonitions.
  // Only Docusaurus front matter is omitted; never summarize normative text.
  return source.replace(/^---\n[\s\S]*?\n---\n/, '').trimStart();
}

export function scopeTextLinks(body, prefix) {
  let fence = null;
  return body.split('\n').map(line => {
    const marker = line.match(/^ {0,3}(`{3,}|~{3,})/);
    if (marker) {
      if (!fence) fence = marker[1];
      else if (marker[1][0] === fence[0] && marker[1].length >= fence.length) fence = null;
      return line;
    }
    if (fence) return line;
    return line.replace(/(`+).*?\1|\[[^\]\n]*\]\((\/(?!\/)[^)\s]*)\)/g,
      (match, code, href) => code ? match : match.replace('](' + href + ')', '](' + prefix + href + ')'));
  }).join('\n');
}

export function textBody(name, source, version = 'current', baseUrl = '/') {
  if (!name.endsWith('.md')) return source;
  const {state, role, audiences} = classify(name, source);
  return [
    '> Document context (generated): ' + state + ' | ' + role + ' | ' + audiences.join(', '),
    '> Documentation version: ' + (version === 'current' ? 'Development (unreleased)' : version),
    '> Source: docs/' + name,
    '> ' + contextNotice(state, role),
    '> Markdown follows with front matter removed and root-relative website links scoped to this edition; contract text and code remain unchanged.',
    '', scopeTextLinks(markdownBody(source), baseUrl + (version === 'current' ? 'next' : version)),
  ].join('\n');
}

function linkProse(body) {
  let fence = null;
  return body.split('\n').filter(line => {
    const marker = line.match(/^ {0,3}(`{3,}|~{3,})/);
    if (marker && !fence) { fence = marker[1]; return false; }
    if (marker && fence && marker[1][0] === fence[0] && marker[1].length >= fence.length) {
      fence = null;
      return false;
    }
    return !fence;
  }).join('\n').replace(/(`+)[\s\S]*?\1/g, '');
}

export function validateLinks(documents) {
  const files = new Set(documents.map(([name]) => name));
  files.add('publication.json');
  for (const [name, body] of documents) {
    if (!name.endsWith('.md')) continue;
    for (const match of linkProse(body).matchAll(/\[[^\]\n]*\]\(([^)\s]+)\)/g)) {
      const href = match[1];
      if (/^(?:[a-z][a-z0-9+.-]*:|#|\/)/i.test(href)) continue;
      const target = decodeURIComponent(href.split(/[?#]/)[0]);
      const resolved = path.posix.normalize(path.posix.join(path.posix.dirname(name), target));
      if (!files.has(resolved)) throw new Error('Unresolved text link: ' + name + ' -> ' + href);
    }
  }
}

export function documentDigest(documents) {
  return createHash('sha256').update(JSON.stringify(documents)).digest('hex');
}

export async function exportText({docsDir, outDir, version = 'current', baseUrl = '/'}) {
  const documents = await readDocuments(docsDir);
  validateLinks(documents);
  const digest = documentDigest(documents);
  const target = path.resolve(outDir, 'agent-docs');
  if (path.dirname(target) !== path.resolve(outDir)) throw new Error('Invalid text output directory');
  // Only this plugin-owned output subtree is replaced. Never touch sources.
  await rm(target, {recursive: true, force: true});
  for (const [name, source] of documents) {
    const destination = path.join(target, name);
    await mkdir(path.dirname(destination), {recursive: true});
    await writeFile(destination, textBody(name, source, version, baseUrl));
  }
  await mkdir(target, {recursive: true});
  await writeFile(path.join(target, 'publication.json'), JSON.stringify({
    edition: 3,
    source_digest_algorithm: 'sha256',
    source_digest: digest,
    source_digest_input: 'JSON array of sorted [docs-relative path, LF-normalized source] pairs',
    documentation_version: version,
    status: version === 'current' ? 'Development documentation (unreleased)' : 'Documentation for Pactrun ' + version,
    context_policy: 'Generated context precedes Markdown with edition-scoped root-relative website links. Contract text, code and JSON schemas are preserved.',
    authority_map: 'spec/index.md',
    implementation_status: 'development/next-milestone.md',
    documents: documents.map(([name]) => name),
  }, null, 2) + '\n');
  await writeFile(path.join(outDir, 'llms.txt'), [
    '# Pactrun — ' + (version === 'current' ? 'Development (unreleased)' : version), '',
    '> Local-first Pack installation, managed Instances, and Pack-defined operations.', '',
    'Product guides, specification, and development records from one English source.',
    'Task guides are informative; the linked specification defines behavior.', '',
    '## Start here', '',
    '- [User guides](./agent-docs/guides/index.md): operating procedures and tutorials.',
      '- [Pack author guide](./agent-docs/package-authors/index.md): prepare a workspace, then build and validate a Pack.',
    '- [Document catalog](./document-catalog.json): generated source paths, roles, states, and source digest.',
      '- [Agent task entry](./agent-docs/agents/index.md): choose operating, authoring, Hook integration, or development work.',
    '- [Development paths](./agent-docs/development/reading-paths.md): read the owning contracts by task.',
    '- [Next milestone](./agent-docs/development/next-milestone.md): current baseline, implementation availability, and work status.',
    '- [Contract catalog](./agent-docs/spec/catalog.md): exact specification status.',
    '- [Develop Pactrun](./agent-docs/agents/develop-pactrun.md): project rules and verification.',
    '- [Specification](./agent-docs/spec/index.md): the sole normative product-rule tree.',
    '- [Publication metadata](./agent-docs/publication.json): source snapshot identity.', '',
  ].join('\n'));
  return {digest, count: documents.length};
}

export default function textDocs(context) {
  return {
    name: 'pactrun-text-docs',
    async postBuild({outDir, routesPaths}) {
      const base = context.siteConfig.baseUrl;
      for (const route of routesPaths) {
        if (/^(?:[^/]+\/)?agents(?:\/|$)/.test(route.slice(base.length))) {
          throw new Error('Agent source unexpectedly became an HTML route: ' + route);
        }
      }
      for (const edition of await editions(context.siteDir)) {
        await verifyText({docsDir: edition.docsDir, outDir: path.join(outDir, edition.path), version: edition.id, baseUrl: base});
      }
    },
  };
}

export async function verifyText({docsDir, outDir, version = 'current', baseUrl = '/'}) {
  const documents = await readDocuments(docsDir);
  const metadata = JSON.parse(await readFile(path.join(outDir, 'agent-docs/publication.json'), 'utf8'));
  if (metadata.documentation_version !== version) throw new Error('Text edition mismatch');
  const textEntry = await readFile(path.join(outDir, 'llms.txt'), 'utf8');
  if (!textEntry.includes('./agent-docs/agents/index.md')) {
    throw new Error('Text discovery entry is missing or invalid');
  }
  if (metadata.source_digest !== documentDigest(documents)) {
    throw new Error('Text publication is stale; rebuild from unchanged sources');
  }
  for (const [name, source] of documents) {
    const published = await readFile(path.join(outDir, 'agent-docs', name), 'utf8');
    if (published !== textBody(name, source, version, baseUrl)) throw new Error('Text publication drift: ' + name);
  }
}
