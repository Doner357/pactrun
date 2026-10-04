import React from 'react';
import OriginalLogo from '@theme-original/Logo';
import type {Props} from '@theme/Logo';
import {useEdition} from '../../lib/edition';

export default function Logo(props: Props) {
  const {docBase} = useEdition();
  return <OriginalLogo {...props} to={docBase} />;
}
