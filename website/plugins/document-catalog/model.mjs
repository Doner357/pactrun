// Shared classification for HTML, search and agent text. No product rules live here.
export function classify(name, source) {
  const superseded = name.startsWith('pactrun-developers/') ||
    /Informative compatibility entry; no independent specification/.test(source) ||
    /^(?:title: |# )Retired development (?:contract|(?:persistence )?schema)\b|^# Compatibility entry/m.test(source);
  const state = superseded ? 'superseded' : 'current';
  let role = 'Guide';
  if (superseded) role = 'Compatibility';
  else if (name.startsWith('spec/') && (name.endsWith('/index.md') || ['spec/catalog.md', 'spec/core/vocabulary.md'].includes(name))) role = 'Reference';
  else if (name.startsWith('spec/')) role = 'Specification';
  else if (name.includes('/reference/')) role = 'Reference';
  else if (name.includes('/concepts/')) role = 'Explanation';
  else if (/command-and-output-reference|invoke-reference/.test(name)) role = 'Reference';
  else if (/^(introduction\.md|package-authors\/fundamentals\/authoring-model\.md)$/.test(name)) role = 'Tutorial';

  let audiences;
  if (['guides/installation.md', 'guides/data-location.md'].includes(name)) audiences = ['Users', 'Authors'];
  else if (name.startsWith('package-authors/')) audiences = ['Authors'];
  else if (name.startsWith('agents/')) audiences = ['Agents'];
  else if (/^pactrun-developers\//.test(name)) audiences = ['Developers'];
  else if (name.startsWith('spec/persistence/')) audiences = ['Developers'];
  else if (name.startsWith('spec/execution/')) audiences = ['Authors', 'Developers'];
  else if (name.startsWith('spec/')) audiences = ['Users', 'Authors', 'Developers'];
  else audiences = ['Users'];
  return {state, role, audiences};
}

export function contextNotice(state, role) {
  if (state === 'superseded') return 'Superseded compatibility page. Follow its current-owner links; it defines no independent product rules.';
  return null;
}
