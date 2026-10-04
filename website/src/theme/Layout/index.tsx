import React from 'react';
import OriginalLayout from '@theme-original/Layout';
import type {Props} from '@theme/Layout';
import Head from '@docusaurus/Head';
import {useEdition} from '../../lib/edition';

export default function Layout(props: Props) {
  const {edition, docBase, location} = useEdition();
  const missing = new URLSearchParams(location.search).get('missing');
  const report = 'https://github.com/Doner357/pactrun/issues/new?' + new URLSearchParams({
    title: '[Docs] ' + edition.label,
    body: `Documentation version: ${edition.label}\nPage: ${location.pathname}${location.hash}\n\nWhat needs correcting?\n`,
  });
  return <OriginalLayout {...props}>
    <Head><link rel="describedby" type="text/plain" href={docBase + 'llms.txt'} /></Head>
    <aside className="edition-bar" aria-label="Documentation edition">
      <span>Pactrun {edition.label}</span>
      <a href={docBase + 'llms.txt'}>Text documentation</a>
      <a href={report}>Report a documentation issue</a>
    </aside>
    {missing && <div role="status" className="alert alert--info">“{missing}” is not available in {edition.label}. Choose a topic from this version's documentation.</div>}
    {props.children}
  </OriginalLayout>;
}
