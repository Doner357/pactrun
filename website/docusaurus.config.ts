import type {Config} from '@docusaurus/types';
import type {Options, ThemeConfig} from '@docusaurus/preset-classic';
import versions from './versions.json';
import {versionLinks} from './plugins/editions.mjs';

const config: Config = {
  title: 'Pactrun',
  tagline: 'Pactrun documentation',
  url: process.env.DOCUSAURUS_URL ?? 'http://localhost:3000',
  baseUrl: process.env.DOCUSAURUS_BASE_URL ?? '/',
  onBrokenLinks: 'throw',
  onBrokenAnchors: 'throw',
  staticDirectories: ['static', '.generated-text'],
  plugins: ['./plugins/text-docs/index.mjs', './plugins/document-catalog/index.mjs'],

  presets: [
    [
      'classic',
      {
        docs: {
          path: '../docs',
          exclude: ['agents/**', 'archive/**', 'proposals/**'],
          routeBasePath: '/',
          sidebarPath: './sidebars.ts',
          lastVersion: versions[0],
          versions: {
            current: {label: 'Development (unreleased)', path: 'next', banner: 'unreleased'},
            ...Object.fromEntries(versions.map(version => [version, {label: version, path: version}])),
          },
          beforeDefaultRemarkPlugins: [versionLinks],
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
        {type: 'custom-editionLink', target: '/guides/', label: 'Users', position: 'left'},
        {type: 'custom-editionLink', target: '/package-authors/', label: 'Authors', position: 'left'},
        {type: 'custom-editionLink', target: '/spec/', label: 'Spec', position: 'left'},
        {type: 'custom-editionLink', target: '/commands', label: 'Commands', position: 'right'},
        {type: 'custom-editionLink', target: '/search', label: 'Search', position: 'right'},
        {type: 'custom-editionPicker', position: 'right'},
        {href: 'https://github.com/Doner357/pactrun', label: 'GitHub', position: 'right'},
        {type: 'custom-editionLink', target: '/', position: 'left', label: 'Browse'},
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
