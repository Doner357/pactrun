import React from 'react';
import {Redirect, useLocation} from '@docusaurus/router';
import useBaseUrl from '@docusaurus/useBaseUrl';
import {usePluginData} from '@docusaurus/useGlobalData';

type Edition = {path: string; pages: {url: string; requirementAnchors: Record<string, string>}[]};
export default function LegacyRedirect({target}: {target: {to: string; sections?: Record<string, string>}}) {
  const location = useLocation();
  const {editions} = usePluginData('pactrun-document-catalog') as {editions: Edition[]};
  let destination = target.sections?.[location.hash.slice(1)] ?? target.to;
  let fragment = target.sections?.[location.hash.slice(1)] ? '' : location.hash;
  const requirement = location.hash.match(/^#(pr-req-\d{4})(?:-|$)/i)?.[1]?.toUpperCase();
  if (requirement && target.to.includes('/spec/')) {
    const edition = editions.find(item => target.to.startsWith('/' + item.path + '/'));
    const owner = edition?.pages.find(page => page.requirementAnchors[requirement]);
    if (owner) {
      destination = owner.url;
      fragment = '#' + owner.requirementAnchors[requirement];
    }
  }
  const [pathname, selectedAnchor] = destination.split('#');
  const to = useBaseUrl(pathname);
  return <Redirect to={to + location.search + (selectedAnchor ? '#' + selectedAnchor : fragment)} />;
}
