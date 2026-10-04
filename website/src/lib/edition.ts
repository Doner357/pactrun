import {usePluginData} from '@docusaurus/useGlobalData';
import {useLocation} from '@docusaurus/router';
import useBaseUrl from '@docusaurus/useBaseUrl';
import type {SearchPage} from './search.mjs';

export type Edition = {id: string; label: string; path: string;
  help: {version: string; text: string}; pages: SearchPage[]};

export function useEdition() {
  const {editions} = usePluginData('pactrun-document-catalog') as {editions: Edition[]};
  const location = useLocation();
  const base = useBaseUrl('/');
  const local = '/' + location.pathname.slice(base.length);
  const edition = editions.find(item => local === '/' + item.path || local.startsWith('/' + item.path + '/')) ?? editions[0];
  return {edition, editions, base, location, prefix: '/' + edition.path,
    docBase: base + edition.path + '/'};
}
