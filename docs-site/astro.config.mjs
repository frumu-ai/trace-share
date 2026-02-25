import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

const site = process.env.SITE_URL || 'http://localhost:4321';
const base = process.env.BASE_PATH || '/';

export default defineConfig({
  site,
  base,
  integrations: [
    starlight({
      title: 'trace-share',
      description: 'Open data infrastructure for coding-agent training traces.',
      customCss: ['./src/styles/oss-theme.css'],
      social: {
        github: 'https://github.com/frumu-ai/trace-share'
      }
    })
  ]
});
