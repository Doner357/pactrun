import React, {type ReactNode} from 'react';
import OriginalPaginator from '@theme-original/DocPaginator';
import type {Props} from '@theme/DocPaginator';
import {useLocation} from '@docusaurus/router';
import {allowReaderNavigation} from '../../lib/reader-role.mjs';
import {readingNavigation} from '../../lib/reading-order.mjs';
import {useEdition} from '../../lib/edition';
import Link from '@docusaurus/Link';

export default function DocPaginator(props: Props): ReactNode {
  const {pathname} = useLocation();
  const {base, docBase, edition: catalog} = useEdition();
  const pageAt = (permalink: string) => catalog.pages.find(page =>
    (base + page.url.slice(1)).replace(/\/$/, '') === permalink.replace(/\/$/, ''));
  const entry = pageAt(pathname);
  if (!entry) return null;
  const inTutorial = (source: string) => source === 'package-authors/index.md' ||
    source.startsWith('package-authors/tutorial/');
  if (entry.state === 'current' && inTutorial(entry.source)) {
    const tutorialTarget = (item: Props['previous']) => {
      const page = item && pageAt(item.permalink);
      return page && inTutorial(page.source) ? item : undefined;
    };
    return <OriginalPaginator {...props} previous={tutorialTarget(props.previous)} next={tutorialTarget(props.next)} />;
  }
  const route = readingNavigation(entry.source, entry.state);
  const target = (source?: string) => {
    const page = catalog.pages.find(item => item.source === source);
    if (!page) return undefined;
    const permalink = base + page.url.slice(1);
    return allowReaderNavigation(pathname, permalink, docBase) ? {title: page.title, permalink} : undefined;
  };
  const back = target(route.back);
  if (route.kind !== 'sequence') return back && route.back !== entry.source ?
    <nav className="pagination-nav" aria-label="Reading route"><Link to={back.permalink}>{route.label}</Link></nav> : null;
  return <>
    <OriginalPaginator {...props} previous={target(route.previous)} next={target(route.next)} />
    {!route.next && back && <nav className="pagination-nav" aria-label="Choose the next task"><Link to={back.permalink}>{route.label}</Link></nav>}
  </>;
}
