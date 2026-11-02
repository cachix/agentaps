import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import starlightLlmActions from 'starlight-llm-actions';
import { siteKitAstro } from '@cachix/site-kit/astro';
import { siteKitStarlight } from '@cachix/site-kit/starlight';
import { siteLlmActionsOptions } from '@cachix/site-kit/starlight/llms';

const repository = 'https://github.com/cachix/agentaps';

export default defineConfig({
  site: 'https://agentaps.dev/',
  outDir: '../dist',
  integrations: [
    siteKitAstro({ github: { repository: 'cachix/agentaps' } }),
    starlight({
      plugins: [
        siteKitStarlight(),
        starlightLlmActions(siteLlmActionsOptions('Agentaps is a desktop workspace for coding agents that speak the Agent Client Protocol (ACP). Run an agent in a local project or over SSH, keep its conversations together, and continue from a phone browser when you step away.')),
      ],
      title: 'Agentaps',
      description: 'One workspace for your coding agents. Run your harness locally or over SSH, review changes, and continue securely from your browser with Agentaps.',
      favicon: '/favicon.svg',
      head: [
        { tag: 'meta', attrs: { name: 'theme-color', content: '#10151c', media: '(prefers-color-scheme: dark)' } },
        { tag: 'meta', attrs: { name: 'theme-color', content: '#f8fafc', media: '(prefers-color-scheme: light)' } },
      ],
      components: {
        SiteTitle: './src/overrides/SiteTitle.astro',
        SocialIcons: './src/overrides/SocialIcons.astro',
      },
      social: [
        { icon: 'github', label: 'GitHub', href: repository },
        { icon: 'discord', label: 'Discord', href: 'https://discord.gg/naMgvexb6q' },
      ],
      editLink: { baseUrl: 'https://github.com/cachix/agentaps/edit/main/web/site/' },
      lastUpdated: true,
      customCss: ['./src/styles/custom.css'],
      sidebar: [
        {
          label: 'Getting Started',
          items: [
            { label: 'Get started', slug: 'getting-started' },
            { label: 'Work in Agentaps', slug: 'using-agentaps' },
            { label: 'Web Connect', slug: 'web-connect' },
            { label: 'Session terminal', slug: 'terminal' },
          ],
        },
        {
          label: 'Reference',
          items: [
            { label: 'Build from source', slug: 'build-from-source' },
            { label: 'Current limitations', slug: 'limitations' },
          ],
        },
      ],
    }),
  ],
});
