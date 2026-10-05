import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {mkdir, readdir, writeFile} from 'node:fs/promises';
import {readDocuments, markdownBody} from '../plugins/text-docs/index.mjs';
import {requirementAnchors} from '../plugins/document-catalog/index.mjs';

// Informative reader views. Spec remains the sole owner; tests reject stale views.
export const references = [
  ['Users', 'command-details', 'Command details', 'spec/interfaces/commands.md'],
  ['Users', 'snapshot-details', 'Snapshot command details', 'spec/snapshots/commands.md'],
  ['Users', 'migration-details', 'Migration command details', 'spec/migrations/commands.md'],
  ['Users', 'resource-details', 'Resource command details', 'spec/instances/resource-commands.md'],
  ['Users', 'retirement-details', 'Retirement and recovery details', 'spec/lifecycle/retirement.md'],
  ['Users', 'object-lifecycle', 'Retained objects and garbage collection', 'spec/lifecycle/objects-gc.md'],
  ['Users', 'machine-output', 'JSON and JSONL reference', 'spec/interfaces/machine-output.md'],
  ['Users', 'snapshot-limits', 'Snapshot limits and compatibility', 'spec/snapshots/limits.md'],
  ['Authors', 'source-format', 'Complete Pack source reference', 'spec/packages/source-format.md'],
  ['Authors', 'revision-format', 'Declaration types and identity', 'spec/packages/revision-format.md'],
  ['Authors', 'hook-protocol', 'Direct Hook message reference', 'spec/interfaces/hook-protocol.md'],
  ['Authors', 'shell-loader', 'Shell helper reference', 'spec/interfaces/shell-loader.md'],
  ['Authors', 'snapshot-limits', 'Snapshot authoring limits', 'spec/snapshots/limits.md'],
].map(([audience, slug, title, source]) => ({audience, title, source,
  output: `${audience === 'Users' ? 'pactrun-users' : 'package-authors'}/reference/${slug}.md`}));
export const schemaCopies = ['cli-machine.schema.json', 'cli-events.schema.json'].map(name => ({
  source: 'spec/interfaces/' + name, output: 'pactrun-users/reference/' + name,
}));
const relative = (from, to) => {
  const value = path.posix.relative(path.posix.dirname(from), to);
  return value.startsWith('.') ? value : './' + value;
};
const slug = title => title.toLowerCase().replace(/[^\p{L}\p{N}_\-\s]/gu, '').replace(/\s/g, '-');

// Prose rewriting must not reinterpret literal Markdown inside payload examples.
function isolateCode(source) {
  const prefix = '\u0000READER_CODE_';
  if (source.includes(prefix)) throw new Error('Reserved reader projection marker in source');
  const code = [];
  const save = text => `${prefix}${code.push(text) - 1}\u0000`;
  const lines = source.match(/[^\n]*(?:\n|$)/g).filter(Boolean);
  const parts = [];
  for (let i = 0; i < lines.length; i++) {
    const open = lines[i].replace(/\r?\n$/, '').match(/^ {0,3}(`{3,}|~{3,})(.*)$/);
    if (!open || (open[1][0] === '`' && open[2].includes('`'))) {parts.push(lines[i]); continue;}
    const close = new RegExp(`^ {0,3}${open[1][0]}{${open[1].length},}[ \\t]*$`);
    let end = i + 1;
    while (end < lines.length && !close.test(lines[end].replace(/\r?\n$/, ''))) end++;
    if (end < lines.length) end++;
    const block = lines.slice(i, end).join('');
    const newline = block.match(/\r?\n$/)?.[0] ?? '';
    parts.push(save(newline ? block.slice(0, -newline.length) : block) + newline);
    i = end - 1;
  }
  const fenced = parts.join('');
  const runs = [...fenced.matchAll(/`+/g)];
  let cursor = 0;
  const prose = [];
  for (let i = 0; i < runs.length; i++) {
    for (let j = i + 1; j < runs.length; j++) {
      const between = fenced.slice(runs[i].index, runs[j].index);
      if (/\n[ \t\r]*\n/.test(between) || between.includes(prefix)) break;
      if (runs[j][0].length !== runs[i][0].length) continue;
      const end = runs[j].index + runs[j][0].length;
      prose.push(fenced.slice(cursor, runs[i].index), save(fenced.slice(runs[i].index, end)));
      cursor = end;
      i = j;
      break;
    }
  }
  prose.push(fenced.slice(cursor));
  return {fenced, prose: prose.join(''), restore: text => text.replace(/\u0000READER_CODE_(\d+)\u0000/g, (_, index) => code[Number(index)])};
}

export function renderReference(entry, source) {
  const literal = isolateCode(markdownBody(source));
  const anchors = requirementAnchors(literal.fenced);
  const sources = new Map([[entry.source, entry.title]]);
  const targets = new Map(references.filter(r => r.audience === entry.audience).map(r => [r.source, r.output]));
  if (entry.audience === 'Users') for (const r of schemaCopies) targets.set(r.source, r.output);
  let body = literal.prose
    .replace(/<!-- spec-navigation:start -->[\s\S]*?<!-- spec-navigation:end -->/g, '')
    .replace(/^\*\*Status:[\s\S]*?\*\*\s*/m, '')
    .replace(/^\*\*Verification:[\s\S]*?\*\*\s*/gm, '')
    .replace(/^## Reading map \(informative\)\s*/m, '')
    .replace(/^# (.+)\n/, (_, title) => `<a id="${slug(literal.restore(title))}" />\n`)
    .replace(/^(#{2,6}) (PR-REQ-\d+)\s*-\s*([^\n]+)$/gm,
      (_, level, id, title) => `${level} ${title.replace(/\s*\{#[^}]+\}\s*$/, '')} {#${anchors[id]}}`)
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, (original, label, href) => {
      if (/^(?:[a-z][a-z0-9+.-]*:|\/|#)/i.test(href) || !/\.(?:md|json)(?:#|$)/.test(href)) return original;
      const [file, fragment] = href.split('#');
      const resolved = path.posix.normalize(path.posix.join(path.posix.dirname(entry.source), file));
      if (targets.has(resolved)) return `[${label}](${relative(entry.output, targets.get(resolved))}${fragment ? '#' + fragment : ''})`;
      // Cross-layer implementation/history links remain available for maintainers,
      // but do not interrupt a role-specific reading path.
      sources.set(resolved + (fragment ? '#' + fragment : ''), label);
      return label;
    }).trim();
  const start = entry.audience === 'Users' ? './index.md' : './pack-fields.md';
  return literal.restore(`---\ntitle: ${entry.title}\n---\n\n# ${entry.title}\n\n` +
    `<!-- Generated by reader-references.mjs from ${entry.source}; edit the owner, not this view. -->\n\n` +
    `Start with the [${entry.audience === 'Users' ? 'user reference guide' : 'Pack field guide'}](${start}) for task-oriented explanations.\n` +
    '\n' + body +
    '\n\n<details>\n<summary>Maintainer sources (optional)</summary>\n\n' +
    [...sources].map(([file, label]) => `- [${label}](${relative(entry.output, file)})`).join('\n') +
    '\n\n</details>\n');
}

export function expectedReferences(documents) {
  const source = new Map(documents);
  return [...references.map(entry => [entry.output, renderReference(entry, source.get(entry.source))]),
    ...schemaCopies.map(entry => [entry.output, source.get(entry.source)])];
}

// Emit bounded apply_patch inputs, never edit repository documents implicitly.
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv[2] !== '--patch-dir' || !process.argv[3]) throw new Error('Usage: node reader-references.mjs --patch-dir target/...');
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
  const out = path.resolve(root, process.argv[3]);
  if (!out.startsWith(path.join(root, 'target') + path.sep)) throw new Error('Patch output must be under target/');
  await mkdir(out, {recursive: true});
  if ((await readdir(out)).length) throw new Error('Use an empty patch directory; stale patches must not be reapplied');
  const docs = await readDocuments(path.join(root, 'docs'));
  const existing = new Map(docs);
  let count = 0;
  const emit = async patch => writeFile(path.join(out, String(count++).padStart(4, '0') + '.patch'), '*** Begin Patch\n' + patch + '*** End Patch\n');
  for (const [name, body] of expectedReferences(docs)) {
    if (existing.get(name) === body) continue;
    const file = 'docs/' + name;
    if (existing.has(name)) await emit(`*** Delete File: ${file}\n`);
    const marker = '<!-- reader-reference:building -->';
    await emit(`*** Add File: ${file}\n+${marker}\n`);
    let chunk = [];
    const flush = async () => { if (!chunk.length) return; await emit(`*** Update File: ${file}\n@@\n-${marker}\n${chunk.map(line => '+' + line).join('\n')}\n+${marker}\n`); chunk = []; };
    for (const line of body.replace(/\n$/, '').split('\n')) {
      if (chunk.join('\n').length + line.length > 6000) await flush();
      chunk.push(line);
    }
    await flush();
    await emit(`*** Update File: ${file}\n@@\n-${marker}\n`);
  }
  console.log(`Prepared ${count} bounded patches in ${out}`);
}
