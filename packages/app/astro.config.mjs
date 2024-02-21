import { defineConfig } from 'astro/config';
import { searchForWorkspaceRoot } from 'vite';
import solidJs from '@astrojs/solid-js';
import render from './render-directive/register';

// https://astro.build/config
export default defineConfig({
    integrations: [solidJs(), render()],

    // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
    //
    // 1. prevent vite from obscuring rust errors
    clearScreen: false,
    // 2. tauri expects a fixed port, fail if that port is not available
    server: {
        port: 1420,
        strictPort: true,
        fs: {
            allow: [
                searchForWorkspaceRoot(process.cwd()),
                //"node_modules/@moosicbox/moosicbox-ui/src",
                //"../MoosicBoxUI/src",
            ],
        },
    },
    // 3. to make use of `TAURI_DEBUG` and other env variables
    // https://tauri.studio/v1/api/config#buildconfig.beforedevcommand
    envPrefix: ['VITE_', 'TAURI_'],
    build: {
        // Tauri supports es2021
        target: ['es2021', 'chrome100', 'safari13'],
        // don't minify for debug builds
        minify: !process.env.TAURI_DEBUG ? 'esbuild' : false,
        // produce sourcemaps for debug builds
        sourcemap: !!process.env.TAURI_DEBUG,
    },
});
