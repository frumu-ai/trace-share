import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: process.env.SITE_URL,
  integrations: [
    starlight({
      title: 'trace-share',
      description: 'Open data infrastructure for coding-agent training traces.',
      social: {
        github: 'https://github.com/frumu-ai/trace-share'
      }
    })
  ]
});
