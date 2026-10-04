import {readFile} from 'node:fs/promises';
import path from 'node:path';

export async function editions(siteDir) {
  const versions = JSON.parse(await readFile(path.join(siteDir, 'versions.json'), 'utf8'));
  return [
    ...versions.map(version => ({id: version, label: version, path: version,
      docsDir: path.join(siteDir, 'versioned_docs', 'version-' + version)})),
    {id: 'current', label: 'Development (unreleased)', path: 'next', docsDir: path.resolve(siteDir, '../docs')},
  ];
}

// Absolute links in Markdown must stay within the rendered edition too.
export function versionLinks() {
  return (tree, file) => {
    const filename = String(file.history?.[0] ?? file.path ?? '').replaceAll('\\', '/');
    const version = filename.match(/\/versioned_docs\/version-([^/]+)\//)?.[1];
    const prefix = '/' + (version ?? 'next');
    function visit(node) {
      if (node.type === 'link' && node.url.startsWith('/') && !node.url.startsWith('//')) {
        node.url = prefix + node.url;
      }
      for (const child of node.children ?? []) visit(child);
    }
    visit(tree);
  };
}
