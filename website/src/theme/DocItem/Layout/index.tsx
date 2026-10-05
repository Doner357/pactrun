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
      {entry.state !== 'current' && <p>This page preserves an old link. Follow its current-owner links or the <Link to={prefix + '/spec/'}>specification</Link>.</p>}
      {entry.role === 'Specification' && <p>Product contract.</p>}
      <a href={docBase + 'agent-docs/' + entry.source}>Read Markdown text</a>
      {audience && <> · <Link to={prefix + '/search?audience=' + audience}>Search {audience === 'Authors' ? 'author' : 'user'} docs</Link></>}
    </aside>}
    <OriginalLayout {...props} />
  </>;
}
