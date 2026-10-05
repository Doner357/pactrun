// Editorial reading routes, not product operation or authorization rules.
export const readingSequences = {
  Users: ['guides/index.md', 'guides/installation.md', 'introduction.md', 'guides/use-pack.md'],
  Authors: ['package-authors/index.md', 'package-authors/setup.md',
    'package-authors/fundamentals/authoring-model.md',
    'package-authors/fundamentals/actions-inputs-and-parameters.md',
    'package-authors/fundamentals/recipes-and-runtime-content.md',
    'package-authors/managed-capabilities/hooks-recovery-and-cleanup.md'],
};

export function readingNavigation(source, state = 'current') {
  if (state !== 'current') return {kind: 'record', back: 'spec/index.md', label: 'Read the specification'};
  for (const [audience, sequence] of Object.entries(readingSequences)) {
    const index = sequence.indexOf(source);
    if (index !== -1) return {kind: 'sequence', audience, previous: sequence[index - 1], next: sequence[index + 1],
      back: sequence[0], label: audience === 'Users' ? 'Choose an operating task' : 'Choose an authoring task'};
  }
  if (source.startsWith('package-authors/')) return {kind: 'lookup', back: 'package-authors/index.md', label: 'Back to the author guide'};
  if (source.startsWith('guides/') || source.startsWith('pactrun-users/')) return {
    kind: 'lookup', back: source.includes('/reference/') || source.includes('-reference.md') ? 'pactrun-users/reference/index.md' : 'guides/index.md',
    label: source.includes('/reference/') || source.includes('-reference.md') ? 'Back to the user reference' : 'Choose an operating task',
  };
  if (source.startsWith('spec/')) {
    const topics = {
      core: 'System and Core Concepts', packages: 'Packages and Revisions',
      instances: 'Instances and Data', operations: 'Operations and Execution',
      snapshots: 'Snapshots and Restore', migrations: 'Migration',
      lifecycle: 'Lifecycle and Recovery', interfaces: 'Hooks and Integration Interfaces',
      storage: 'Compatibility and Storage',
    };
    const directory = source.split('/')[1];
    const topic = directory === 'persistence' ? 'storage' : directory;
    if (topics[topic] && !source.endsWith('/index.md')) return {
      kind: 'lookup', back: `spec/${topic}/index.md`, label: 'Back to ' + topics[topic],
    };
    return {kind: 'lookup', back: 'spec/index.md', label: 'Back to the specification'};
  }
  return {kind: 'hub'};
}
