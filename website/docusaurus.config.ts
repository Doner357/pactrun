import type {Config} from '@docusaurus/types';
import type {Options, ThemeConfig} from '@docusaurus/preset-classic';

const config: Config = {
  title: 'Pactrun',
  tagline: 'Pactrun documentation',
  url: process.env.DOCUSAURUS_URL ?? 'http://localhost:3000',
  baseUrl: process.env.DOCUSAURUS_BASE_URL ?? '/',
  onBrokenLinks: 'throw',
  staticDirectories: ['static', '.generated-text'],
  plugins: ['./plugins/text-docs/index.mjs'],

  presets: [
    [
      'classic',
      {
        docs: {
          path: '../docs',
          exclude: ['agents/**', 'archive/**', 'proposals/**'],
          routeBasePath: '/',
          sidebarPath: './sidebars.ts',
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Options,
    ],
  ],

  themeConfig: {
    navbar: {
      title: 'Pactrun',
      items: [
        {
          type: 'docSidebar',
          sidebarId: 'docsSidebar',
          position: 'left',
          label: 'Documentation',
        },
      ],
    },
    footer: {
      links: [{
        title: 'Tools',
        // Static asset, not a React Router page. Its existence is checked post-build.
        items: [{label: 'AI tool documentation (text)', href: 'pathname:///llms.txt'}],
      }],
    },
    colorMode: {
      defaultMode: 'light',
      respectPrefersColorScheme: true,
    },
  } satisfies ThemeConfig,
};

export default config;
