import React, {type ReactNode} from 'react';
import OriginalPaginator from '@theme-original/DocPaginator';
import type {Props} from '@theme/DocPaginator';
import {useLocation} from '@docusaurus/router';
import useBaseUrl from '@docusaurus/useBaseUrl';
import {allowReaderNavigation} from '../../lib/reader-role.mjs';
import {readingNavigation} from '../../lib/reading-order.mjs';
import {usePluginData} from '@docusaurus/useGlobalData';
import Link from '@docusaurus/Link';

export default function DocPaginator(props: Props): ReactNode {
  const {pathname} = useLocation();
  const base = useBaseUrl('/');
  const catalog = usePluginData('pactrun-document-catalog') as {pages: {source: string; title: string; url: string; state: string}[]};
  const entry = catalog.pages.find(page => (base + page.url.slice(1)).replace(/\/$/, '') === pathname.replace(/\/$/, ''));
  if (!entry) return null;
  const route = readingNavigation(entry.source, entry.state);
  const target = (source?: string) => {
    const page = catalog.pages.find(item => item.source === source);
    if (!page) return undefined;
    const permalink = base + page.url.slice(1);
    return allowReaderNavigation(pathname, permalink, base) ? {title: page.title, permalink} : undefined;
  };
  const back = target(route.back);
  if (route.kind !== 'sequence') return back && route.back !== entry.source ?
    <nav className="pagination-nav" aria-label="Reading route"><Link to={back.permalink}>{route.label}</Link></nav> : null;
  return <>
    <OriginalPaginator {...props} previous={target(route.previous)} next={target(route.next)} />
    {!route.next && back && <nav className="pagination-nav" aria-label="Choose the next task"><Link to={back.permalink}>{route.label}</Link></nav>}
  </>;
}
