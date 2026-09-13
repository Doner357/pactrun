import type {SidebarsConfig} from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  docsSidebar: [
    'index',
    {
      type: 'category',
      label: 'Pactrun Spec',
      collapsed: false,
      items: [
        'spec/index', 'spec/glossary', 'spec/catalog',
        {type: 'category', label: 'Foundations', items: [
          'spec/foundations/index',
          'spec/foundations/identity-and-state',
          'spec/foundations/resources-and-versioning',
          'spec/foundations/system-model',
        ]},
        {type: 'category', label: 'Observable behavior', items: [
          'spec/behavior/index',
          'spec/behavior/actions-plans-and-runs',
          'spec/behavior/command-and-output-reference',
          'spec/behavior/inputs-secrets-and-readiness',
          'spec/behavior/m4-runtime-capabilities',
          'spec/behavior/m4-snapshot-command-reference',
          'spec/behavior/packages-revisions-and-instances',
          'spec/behavior/snapshots-migrations-and-recovery',
        ]},
        {type: 'category', label: 'Authoring and format contracts', items: [
          'spec/contracts/index',
          'spec/contracts/actions-inputs-and-parameters',
          'spec/contracts/authoring-model',
          'spec/contracts/error-taxonomy-v1',
          'spec/contracts/hook-protocol-v1',
          'spec/contracts/hooks-recovery-and-cleanup',
          'spec/contracts/migrations',
          'spec/contracts/pack-source-yaml-v1',
          'spec/contracts/recipes-and-runtime-content',
          'spec/contracts/revision-core-format-v1',
          'spec/contracts/snapshot-bundle-v1',
          'spec/contracts/snapshot-integrity-format-v1',
          'spec/contracts/snapshot-integrity-format-v2',
          'spec/contracts/snapshots-and-managed-data',
        ]},
        {type: 'category', label: 'Execution and recovery', items: [
          'spec/execution/index',
          'spec/execution/execution-and-concurrency',
          'spec/execution/m4-snapshot-lifecycle-approval-baseline',
          'spec/execution/recovery-and-reconciliation',
        ]},
        {type: 'category', label: 'Internal persistence', items: [
          'spec/persistence/index',
          'spec/persistence/persistence-schema-v2',
          'spec/persistence/persistence-schema-v3',
          'spec/persistence/persistence-schema-v4',
          'spec/persistence/persistence-schema-v5',
        ]},
      ],
    },
    {
      type: 'category',
      label: 'Development',
      items: [
        'development/index', 'development/product-overview',
        'development/reading-paths', 'development/next-milestone',
        'development/implementation-roadmap', 'development/development-and-verification',
        'development/implementation-guidance', 'development/m4-implementation-status',
        'development/spec-migration-review',
        {type: 'category', label: 'Design syntheses and history', items: [
          'development/design-notes/non-identity-metadata-semantic-baseline',
          'development/design-notes/pre-m2-installation-instance-binding-baseline',
          'development/design-notes/service-storage-semantic-baseline',
          'development/design-notes/m3-action-execution-approval-baseline',
          'development/design-references', 'development/history/documentation-edition-1',
        ]},
      ],
    },
    {
      type: 'category', label: 'Usage Guides (Planned)', items: [
        'guides/index', 'introduction',
        {type: 'category', label: 'Users (Planned)', items: [
          'pactrun-users/concepts/actions-plans-and-runs',
          'pactrun-users/concepts/inputs-secrets-and-readiness',
          'pactrun-users/concepts/packages-revisions-and-instances',
          'pactrun-users/operations/command-and-output-reference',
          'pactrun-users/operations/snapshots-migrations-and-recovery',
        ]},
        {type: 'category', label: 'Pack authors (Planned)', items: [
          'package-authors/fundamentals/actions-inputs-and-parameters',
          'package-authors/fundamentals/authoring-model',
          'package-authors/fundamentals/recipes-and-runtime-content',
          'package-authors/managed-capabilities/hooks-recovery-and-cleanup',
          'package-authors/managed-capabilities/migrations',
          'package-authors/managed-capabilities/snapshots-and-managed-data',
        ]},
      ],
    },
  ],
};

export default sidebars;
