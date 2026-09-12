import type {SidebarsConfig} from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  docsSidebar: [
    'index',
    'introduction',
    {
      type: 'category',
      label: 'Pactrun Developers',
      items: [
        {
          type: 'category',
          label: 'Architecture',
          items: [
            'pactrun-developers/architecture/system-model',
            'pactrun-developers/architecture/identity-and-state',
            'pactrun-developers/architecture/pre-m2-installation-instance-binding-baseline',
            'pactrun-developers/architecture/m3-action-execution-approval-baseline',
            'pactrun-developers/architecture/m4-snapshot-lifecycle-approval-baseline',
            'pactrun-developers/architecture/non-identity-metadata-semantic-baseline',
            'pactrun-developers/architecture/persistence-schema-v2',
            'pactrun-developers/architecture/persistence-schema-v3',
            'pactrun-developers/architecture/persistence-schema-v4',
            'pactrun-developers/architecture/persistence-schema-v5',
            'pactrun-developers/architecture/service-storage-semantic-baseline',
            'pactrun-developers/architecture/execution-and-concurrency',
            'pactrun-developers/architecture/recovery-and-reconciliation',
            'pactrun-developers/architecture/resources-and-versioning',
            'pactrun-developers/architecture/error-taxonomy-v1',
          ],
        },
        {
          type: 'category',
          label: 'Product Behavior',
          items: [
            'pactrun-developers/product-behavior/packages-revisions-and-instances',
            'pactrun-developers/product-behavior/inputs-secrets-and-readiness',
            'pactrun-developers/product-behavior/actions-plans-and-runs',
            'pactrun-developers/product-behavior/snapshots-migrations-and-recovery',
            'pactrun-developers/product-behavior/command-and-output-reference',
            'pactrun-developers/product-behavior/m4-snapshot-command-reference',
            'pactrun-developers/product-behavior/m4-runtime-capabilities',
          ],
        },
        {
          type: 'category',
          label: 'Package Contracts',
          items: [
            'pactrun-developers/package-contracts/authoring-model',
            'pactrun-developers/package-contracts/pack-source-yaml-v1',
            'pactrun-developers/package-contracts/revision-core-format-v1',
            'pactrun-developers/package-contracts/snapshot-integrity-format-v1',
            'pactrun-developers/package-contracts/snapshot-integrity-format-v2',
            'pactrun-developers/package-contracts/snapshot-bundle-v1',
            'pactrun-developers/package-contracts/hook-protocol-v1',
            'pactrun-developers/package-contracts/actions-inputs-and-parameters',
            'pactrun-developers/package-contracts/recipes-and-runtime-content',
            'pactrun-developers/package-contracts/snapshots-and-managed-data',
            'pactrun-developers/package-contracts/migrations',
            'pactrun-developers/package-contracts/hooks-recovery-and-cleanup',
          ],
        },
        {
          type: 'category',
          label: 'Engineering',
          items: [
            'pactrun-developers/engineering/development-and-verification',
            'pactrun-developers/engineering/implementation-guidance',
            'pactrun-developers/engineering/implementation-roadmap',
            'pactrun-developers/engineering/m4-implementation-status',
            'pactrun-developers/engineering/design-references',
          ],
        },
      ],
    },
    {
      type: 'category',
      label: 'Pactrun Users (Planned)',
      items: [
        {
          type: 'category',
          label: 'Concepts (Planned)',
          items: [
            'pactrun-users/concepts/packages-revisions-and-instances',
            'pactrun-users/concepts/inputs-secrets-and-readiness',
            'pactrun-users/concepts/actions-plans-and-runs',
          ],
        },
        {
          type: 'category',
          label: 'Operations (Planned)',
          items: [
            'pactrun-users/operations/snapshots-migrations-and-recovery',
            'pactrun-users/operations/command-and-output-reference',
          ],
        },
      ],
    },
    {
      type: 'category',
      label: 'Package Authors (Planned)',
      items: [
        {
          type: 'category',
          label: 'Fundamentals (Planned)',
          items: [
            'package-authors/fundamentals/authoring-model',
            'package-authors/fundamentals/actions-inputs-and-parameters',
            'package-authors/fundamentals/recipes-and-runtime-content',
          ],
        },
        {
          type: 'category',
          label: 'Managed Capabilities (Planned)',
          items: [
            'package-authors/managed-capabilities/snapshots-and-managed-data',
            'package-authors/managed-capabilities/migrations',
            'package-authors/managed-capabilities/hooks-recovery-and-cleanup',
          ],
        },
      ],
    },
  ],
};

export default sidebars;
