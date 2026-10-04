import React from 'react';
import {Redirect, useLocation} from '@docusaurus/router';
import useBaseUrl from '@docusaurus/useBaseUrl';

export default function LegacyRedirect({target}: {target: {to: string}}) {
  const location = useLocation();
  const to = useBaseUrl(target.to);
  return <Redirect to={to + location.search + location.hash} />;
}
