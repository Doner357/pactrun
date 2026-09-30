import React from 'react';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import {usePluginData} from '@docusaurus/useGlobalData';
import CodeBlock from '@theme/CodeBlock';
export default function Commands(): React.JSX.Element {
  const {help} = usePluginData('pactrun-document-catalog') as {help: {text: string; version: string}};
  return <Layout title="Command help" description="Command help generated from the current source tree."><main className="container search-page">
    <p className="eyebrow">GENERATED REFERENCE</p><h1>Command help</h1>
    <p>Source version: <strong>{help.version}</strong>. Generated from the CLI declaration at build time. Run <code>pactrun --help</code> to inspect your installed executable.</p>
    <p><Link to="/pactrun-users/operations/command-and-output-reference">Output modes and command guide</Link> · <Link to="/pactrun-users/reference/">Complete user reference</Link></p>
    <p><Link to="/pactrun-users/operations/invoke-reference">Invoke: parameters, timeouts, defaults, and examples</Link>. The generated overview below is not a complete option reference.</p>
    <CodeBlock language="text">{help.text}</CodeBlock>
  </main></Layout>;
}
