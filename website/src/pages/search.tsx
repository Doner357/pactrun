import React, {useEffect, useMemo, useState} from 'react';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import useBaseUrl from '@docusaurus/useBaseUrl';
import {useHistory, useLocation} from '@docusaurus/router';
import {searchPages, type SearchPage} from '../lib/search.mjs';

export default function Search(): React.JSX.Element {
  const location = useLocation();
  const history = useHistory();
  const indexUrl = useBaseUrl('/document-catalog.json');
  const [pages, setPages] = useState<SearchPage[]>([]);
  const [status, setStatus] = useState('loading');
  const [attempt, setAttempt] = useState(0);
  const [filtersOpen, setFiltersOpen] = useState(false);
  const params = new URLSearchParams(location.search);
  const q = params.get('q') ?? '';
  const state = params.has('state') ? params.get('state')! : 'current';
  const role = params.get('role') ?? '';
  const audience = params.get('audience') ?? '';

  useEffect(() => {
    const controller = new AbortController();
    setStatus('loading');
    fetch(indexUrl, {signal: controller.signal}).then(response => {
      if (!response.ok) throw new Error('Search index unavailable');
      return response.json();
    }).then(data => {
      if (data.edition !== 2 || !Array.isArray(data.pages)) throw new Error('Invalid index');
      setPages(data.pages);
      setStatus('ready');
    }).catch(error => { if (error.name !== 'AbortError') setStatus('error'); });
    return () => controller.abort();
  }, [indexUrl, attempt]);

  const results = useMemo(() => searchPages(pages, {q, state, role, audience}), [pages, q, state, role, audience]);
  const broader = useMemo(() => searchPages(pages, {q, state: ''}), [pages, q]);
  function update(key: string, value: string) {
    const next = new URLSearchParams(location.search);
    next.set(key, value);
    next.delete('limit');
    history.replace({pathname: location.pathname, search: next.toString()});
  }
  function resetFilters(allStates = false) {
    const next = new URLSearchParams({q});
    if (allStates) next.set('state', '');
    history.replace({pathname: location.pathname, search: next.toString()});
  }
  const limit = Math.max(20, Math.min(1000, Number(params.get('limit')) || 20));
  const scope = [state ? state.charAt(0).toUpperCase() + state.slice(1) : 'All states', audience || 'All audiences', role || 'All roles'].join(' · ');

  return <Layout title="Search documentation" description="Search current documentation and historical records.">
    <main className="container search-page">
      <h1>Search documentation</h1>
      <p>Find commands, concepts, and requirement IDs. Search runs in your browser.</p>
      <label htmlFor="doc-query">Search documentation</label>
      <input id="doc-query" type="search" className="doc-query" value={q}
        placeholder="Try restore, --param-file, or PR-REQ-0258" onChange={e => update('q', e.target.value)} />
      <details className="search-filter-panel" open={filtersOpen}
        onToggle={e => setFiltersOpen(e.currentTarget.open)}>
        <summary>Filters <span className="filter-summary">{scope}</span></summary>
        <div className="search-filters">
          <label>Document state<select aria-label="Document state" value={state} onChange={e => update('state', e.target.value)}>
            <option value="current">Current documents</option><option value="">All states</option>
            <option value="historical">Historical records</option><option value="superseded">Superseded pages</option>
          </select></label>
          <label>Audience<select aria-label="Audience" value={audience} onChange={e => update('audience', e.target.value)}>
            <option value="">All audiences</option>{['Users', 'Authors', 'Developers'].map(x => <option key={x}>{x}</option>)}
          </select></label>
          <label>Document role<select aria-label="Document role" value={role} onChange={e => update('role', e.target.value)}>
            <option value="">All roles</option>{['Tutorial', 'Guide', 'Explanation', 'Reference', 'Informative', 'Specification', 'Development', 'Record', 'Compatibility'].map(x => <option key={x}>{x}</option>)}
          </select></label>
          <button className="button button--secondary" onClick={() => resetFilters()}>Reset filters</button>
        </div>
        <p className="search-scope">Current documents exclude historical and superseded pages. References can serve more than one audience.</p>
      </details>
      <div role="status" aria-live="polite">{status === 'loading' ? 'Loading document index…' : status === 'error' ? 'Search could not load. Use the documentation navigation or retry.' : results.length + ' matching documents'}</div>
      {status === 'error' && <button className="button button--primary" onClick={() => setAttempt(n => n + 1)}>Retry search</button>}
      {status === 'ready' && results.length === 0 && <p>No matches in this scope. Try fewer words or broaden the filters.</p>}
      {status === 'ready' && q.trim() && broader.length > results.length && <p>
        {broader.length - results.length} additional matching documents are outside these filters.{' '}
        <button className="button button--secondary button--sm" onClick={() => resetFilters(true)}>Search all documents</button>
      </p>}
      {status === 'ready' && <ul className="search-results">{results.slice(0, limit).map(result => <li key={result.source}>
        <div className="doc-labels"><span>{result.role}</span>{result.audiences.map(a => <span key={a}>{a}</span>)}<span>{result.state}</span></div>
        <h2><Link to={result.url}>{result.title}</Link></h2>
        <p>{result.snippet}</p><small>{result.source}</small>
      </li>)}</ul>}
      {status === 'ready' && results.length > limit && <button className="button button--secondary" onClick={() => {
        const next = new URLSearchParams(location.search);
        next.set('limit', String(limit + 20));
        history.replace({pathname: location.pathname, search: next.toString()});
      }}>Show more results</button>}
    </main>
  </Layout>;
}
