import React from 'react';
import OriginalLayout from '@theme-original/DocItem/Layout';
import type {Props} from '@theme/DocItem/Layout';
import {useDoc} from '@docusaurus/plugin-content-docs/client';
import {useEdition} from '../../../lib/edition';
import Link from '@docusaurus/Link';
import {readerRole} from '../../../lib/reader-role.mjs';
export default function DocLayout(props: Props): React.JSX.Element {
  const {metadata} = useDoc();
  const {edition: catalog, base, docBase, prefix} = useEdition();
  const entry = catalog.pages.find(page => (base + page.url.slice(1)).replace(/\/$/, '') === metadata.permalink.replace(/\/$/, ''));
  const audience = entry ? readerRole(entry.url, prefix + '/') : null;
  return <>
    {entry && entry.source !== 'index.md' && <aside className={'doc-context ' + (entry.state !== 'current' ? 'doc-context--history' : '')} aria-label="Document context">
      <div className="doc-labels"><span>{entry.role}</span>{entry.audiences.map(audience => <span key={audience}>{audience}</span>)}<span>{entry.state}</span></div>
      {entry.state !== 'current' && <p>This {entry.state === 'superseded' ? 'superseded page preserves an old link' : 'record describes its own historical scope'}. Check <Link to={prefix + '/development/next-milestone'}>current status</Link> and <Link to={prefix + '/spec/'}>owning contracts</Link> before acting.</p>}
      {entry.role === 'Specification' && <p>Product contract. Historical milestone wording is read with the current baseline and any explicitly superseding requirements.</p>}
      {entry.role === 'Informative' && <p>Informative reading aid. Follow the linked owning contracts for product requirements.</p>}
      <a href={docBase + 'agent-docs/' + entry.source}>Read Markdown text</a>
      {audience && <> · <Link to={prefix + '/search?audience=' + audience}>Search {audience === 'Authors' ? 'author' : 'user'} docs</Link></>}
    </aside>}
    <OriginalLayout {...props} />
  </>;
}
