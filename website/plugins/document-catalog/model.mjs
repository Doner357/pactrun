// Shared classification for HTML, search and agent text. No product rules live here.
const currentDevelopment = new Set([
  'index', 'next-milestone', 'reading-paths', 'product-overview',
  'implementation-guidance', 'development-and-verification', 'documentation-style',
  'release-readiness', 'design-references', 'f-documentation-status',
]);

export function classify(name, source) {
  const current = name.replace(/^development\//, '').replace(/\.md$/, '');
  const superseded = name.startsWith('pactrun-developers/') ||
    /Informative compatibility entry; no independent specification/.test(source) ||
    /^(?:title: |# )Retired development (?:contract|(?:persistence )?schema)\b|^# Compatibility entry/m.test(source);
  const state = superseded ? 'superseded' :
    name.startsWith('development/') && !currentDevelopment.has(current) ? 'historical' : 'current';
  let role = 'Guide';
  if (superseded) role = 'Compatibility';
  else if (name.startsWith('spec/') && /^\*\*Status: Informative\b/m.test(source)) role = 'Informative';
  else if (name.startsWith('spec/')) role = 'Specification';
  else if (state === 'historical') role = 'Record';
  else if (name.startsWith('development/')) role = 'Development';
  else if (name.includes('/reference/')) role = 'Reference';
  else if (name.includes('/concepts/')) role = 'Explanation';
  else if (/command-and-output-reference|invoke-reference/.test(name)) role = 'Reference';
  else if (/^(introduction\.md|package-authors\/fundamentals\/authoring-model\.md)$/.test(name)) role = 'Tutorial';

  let audiences;
  if (['guides/installation.md', 'guides/data-location.md'].includes(name)) audiences = ['Users', 'Authors'];
  else if (name.startsWith('package-authors/')) audiences = ['Authors'];
  else if (name.startsWith('agents/')) audiences = ['Agents'];
  else if (/^(development|pactrun-developers|engineering)\//.test(name)) audiences = ['Developers'];
  else if (name.startsWith('spec/persistence/')) audiences = ['Developers'];
  else if (name.startsWith('spec/execution/')) audiences = ['Authors', 'Developers'];
  else if (name.startsWith('spec/')) audiences = ['Users', 'Authors', 'Developers'];
  else audiences = ['Users'];
  return {state, role, audiences};
}

export function contextNotice(state, role) {
  if (state === 'superseded') return 'Superseded compatibility page. Follow its current-owner links; it defines no independent product rules.';
  if (state === 'historical') return 'Historical record. Its approval and verification apply to its recorded scope and source, not to the current baseline.';
  if (role === 'Informative') return 'Informative reading aid. Follow the linked owning contracts for product requirements.';
  return 'Current document. Classification does not establish runtime availability, approval, or verification; consult the owning contract and current handoff.';
}
