import { defineConfig } from 'astro/config';
import aws from 'astro-sst';
import solidJs from '@astrojs/solid-js';
import render from './render-directive/register';

// https://astro.build/config
export default defineConfig({
    integrations: [solidJs(), render()],
    adapter: aws({ deploymentStrategy: 'regional', responseMode: 'stream' }),
});
