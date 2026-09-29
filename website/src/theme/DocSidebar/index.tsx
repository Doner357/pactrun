import React, {type ReactNode} from 'react';
import OriginalSidebar from '@theme-original/DocSidebar';
import type {Props} from '@theme/DocSidebar';
import useBaseUrl from '@docusaurus/useBaseUrl';
import {readerRole} from '../../lib/reader-role.mjs';

export default function DocSidebar(props: Props): ReactNode {
  const base = useBaseUrl('/');
  const role = readerRole(props.path, base);
  const labels = {Users: 'Users', Authors: 'Pack authors'};
  const sidebar = role ? props.sidebar
    .filter(item => item.type === 'category' && item.label === labels[role])
    .map(item => item.type === 'category' ? {...item, collapsed: false} : item) : props.sidebar;
  // Reinitialize category expansion for the new document's active ancestors.
  return <OriginalSidebar key={props.path} {...props} sidebar={sidebar} />;
}
