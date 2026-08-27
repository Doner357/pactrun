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
            'pactrun-developers/architecture/execution-and-concurrency',
            'pactrun-developers/architecture/recovery-and-reconciliation',
            'pactrun-developers/architecture/resources-and-versioning',
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
          ],
        },
        {
          type: 'category',
          label: 'Package Contracts',
          items: [
            'pactrun-developers/package-contracts/authoring-model',
            'pactrun-developers/package-contracts/revision-core-format-v1',
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
