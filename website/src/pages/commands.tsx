import React from 'react';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import {useEdition} from '../lib/edition';
import CodeBlock from '@theme/CodeBlock';
import {Redirect} from '@docusaurus/router';
export default function Commands(): React.JSX.Element {
  const {edition, prefix, docBase, location} = useEdition();
  const {help} = edition;
  if (!location.pathname.startsWith(docBase)) return <Redirect to={docBase + 'commands' + location.search + location.hash} />;
  return <Layout title="Command help" description={'Command help for ' + edition.label}><main className="container search-page">
    <p className="eyebrow">GENERATED REFERENCE</p><h1>Command help</h1>
    <p>Source version: <strong>{help.version}</strong>. {edition.id === 'current' ? 'Generated from the development CLI declaration.' : 'Preserved from this release’s CLI declaration.'} Run <code>pactrun --help</code> to inspect your installed executable.</p>
    <p><Link to={prefix + '/pactrun-users/operations/command-and-output-reference'}>Output modes and command guide</Link> · <Link to={prefix + '/pactrun-users/reference/'}>Complete user reference</Link></p>
    <p><Link to={prefix + '/pactrun-users/operations/invoke-reference'}>Invoke: parameters, timeouts, defaults, and examples</Link>. The generated overview below is not a complete option reference.</p>
    <CodeBlock language="text">{help.text}</CodeBlock>
  </main></Layout>;
}
