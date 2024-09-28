import { renderers } from './renderers.mjs';
import { c as createExports, s as serverEntrypointModule } from './chunks/_@astrojs-ssr-adapter_CSknwEC5.mjs';
import { manifest } from './manifest_DINEaiul.mjs';
import { onRequest } from './_noop-middleware.mjs';

const _page0 = () => import('./pages/_image.astro.mjs');
const _page1 = () => import('./pages/albums.astro.mjs');
const _page2 = () => import('./pages/artists.astro.mjs');
const _page3 = () => import('./pages/auth.astro.mjs');
const _page4 = () => import('./pages/downloads.astro.mjs');
const _page5 = () => import('./pages/login.astro.mjs');
const _page6 = () => import('./pages/settings.astro.mjs');
const _page7 = () => import('./pages/setup/hello.astro.mjs');
const _page8 = () => import('./pages/setup/layout.astro.mjs');
const _page9 = () => import('./pages/setup/music.astro.mjs');
const _page10 = () => import('./pages/index.astro.mjs');

const pageMap = new Map([
    ["node_modules/.pnpm/astro@4.15.1_@types+node@22.5.0_rollup@4.21.0_typescript@5.5.4/node_modules/astro/dist/assets/endpoint/generic.js", _page0],
    ["src/pages/albums.astro", _page1],
    ["src/pages/artists.astro", _page2],
    ["src/pages/auth.astro", _page3],
    ["src/pages/downloads.astro", _page4],
    ["src/pages/login.astro", _page5],
    ["src/pages/settings.astro", _page6],
    ["src/pages/setup/hello.astro", _page7],
    ["src/pages/setup/Layout.astro", _page8],
    ["src/pages/setup/music.astro", _page9],
    ["src/pages/index.astro", _page10]
]);
const serverIslandMap = new Map();

const _manifest = Object.assign(manifest, {
    pageMap,
    serverIslandMap,
    renderers,
    middleware: onRequest
});
const _args = {
    "responseMode": "stream"
};
const _exports = createExports(_manifest, _args);
const handler = _exports['handler'];
const _start = 'start';
if (_start in serverEntrypointModule) {
	serverEntrypointModule[_start](_manifest, _args);
}

export { handler, pageMap };
//# sourceMappingURL=entry.mjs.map
