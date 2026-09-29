// Editorial reading routes, not product operation or authorization rules.
export const readingSequences = {
  Users: ['guides/index.md', 'guides/installation.md', 'introduction.md', 'guides/use-pack.md'],
  Authors: ['package-authors/index.md', 'package-authors/setup.md',
    'package-authors/fundamentals/authoring-model.md',
    'package-authors/fundamentals/actions-inputs-and-parameters.md',
    'package-authors/fundamentals/recipes-and-runtime-content.md',
    'package-authors/managed-capabilities/hooks-recovery-and-cleanup.md'],
  Developers: ['development/index.md', 'development/product-overview.md',
    'development/next-milestone.md', 'development/reading-paths.md',
    'development/implementation-guidance.md', 'development/development-and-verification.md'],
};

export function readingNavigation(source, state = 'current') {
  if (state !== 'current') return {kind: 'record', back: 'development/next-milestone.md', label: 'Read the current baseline'};
  for (const [audience, sequence] of Object.entries(readingSequences)) {
    const index = sequence.indexOf(source);
    if (index !== -1) return {kind: 'sequence', audience, previous: sequence[index - 1], next: sequence[index + 1],
      back: sequence[0], label: audience === 'Users' ? 'Choose an operating task' : audience === 'Authors' ? 'Choose an authoring task' : 'Choose development work'};
  }
  if (source.startsWith('package-authors/')) return {kind: 'lookup', back: 'package-authors/index.md', label: 'Back to the author guide'};
  if (source.startsWith('guides/') || source.startsWith('pactrun-users/')) return {
    kind: 'lookup', back: source.includes('/reference/') || source.includes('-reference.md') ? 'pactrun-users/reference/index.md' : 'guides/index.md',
    label: source.includes('/reference/') || source.includes('-reference.md') ? 'Back to the user reference' : 'Choose an operating task',
  };
  if (source.startsWith('spec/')) return {kind: 'lookup', back: 'spec/index.md', label: 'Back to the specification map'};
  if (source.startsWith('development/')) return {kind: 'lookup', back: 'development/index.md', label: 'Back to the development guide'};
  return {kind: 'hub'};
}
