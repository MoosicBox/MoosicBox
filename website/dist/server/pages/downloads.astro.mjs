import { c as createComponent, r as renderTemplate, d as renderComponent, b as createAstro, m as maybeRenderHead } from '../chunks/astro/server_BHkv7Nwt.mjs';
/* empty css                                     */
import { $ as $$Layout } from '../chunks/Layout_aLm9q08O.mjs';
export { renderers } from '../renderers.mjs';

const $$Astro = createAstro();
const $$Downloads = createComponent(($$result, $$props, $$slots) => {
  const Astro2 = $$result.createAstro($$Astro, $$props, $$slots);
  Astro2.self = $$Downloads;
  const search = Object.fromEntries(new URLSearchParams(Astro2.url.searchParams));
  return renderTemplate`${renderComponent($$result, "Layout", $$Layout, { "title": "MoosicBox", "data-astro-cid-i6ote7bk": true }, { "default": ($$result2) => renderTemplate` ${maybeRenderHead()}<div class="downloads-page" data-astro-cid-i6ote7bk> <div class="downloads-header-text-container" data-astro-cid-i6ote7bk> <h1 class="downloads-header-text" data-astro-cid-i6ote7bk>Downloads</h1> </div> ${renderComponent($$result2, "download-tabs", "download-tabs", { "class": "downloads-page-tabs", "query-param": "tab", "data-astro-cid-i6ote7bk": true }, { "default": () => renderTemplate` ${renderComponent($$result2, "download-tab", "download-tab", { "default": true, "for": "QUEUED", "class": `downloads-page-tabs-queued-tab${search.tab === "QUEUED" ? " active" : ""}`, "data-astro-cid-i6ote7bk": true }, { "default": () => renderTemplate`
Queued
` })} ${renderComponent($$result2, "download-tab", "download-tab", { "for": "HISTORY", "class": `downloads-page-tabs-history-tab${search.tab === "HISTORY" ? " active" : ""}`, "data-astro-cid-i6ote7bk": true }, { "default": () => renderTemplate`
History
` })} ${renderComponent($$result2, "tab-content", "tab-content", { "value": "QUEUED", "class": `downloads-page-queued-downloads${search.tab === "QUEUED" ? " active" : ""}`, "data-astro-cid-i6ote7bk": true }, { "default": () => renderTemplate` ${renderComponent($$result2, "DownloadQueue", null, { "state": "QUEUED", "client:only": true, "client:component-hydration": "only", "data-astro-cid-i6ote7bk": true, "client:component-path": "~/components/DownloadQueue/DownloadQueue", "client:component-export": "default" })} ` })} ${renderComponent($$result2, "tab-content", "tab-content", { "value": "HISTORY", "class": `downloads-page-history-downloads${search.tab === "HISTORY" ? " active" : ""}`, "data-astro-cid-i6ote7bk": true }, { "default": () => renderTemplate` ${renderComponent($$result2, "DownloadQueue", null, { "state": "HISTORY", "client:only": true, "client:component-hydration": "only", "data-astro-cid-i6ote7bk": true, "client:component-path": "~/components/DownloadQueue/DownloadQueue", "client:component-export": "default" })} ` })} ` })} </div> ` })}  `;
}, "/home/bsteffaniak/GitHub/MoosicBoxUI/src/pages/downloads.astro", void 0);

const $$file = "/home/bsteffaniak/GitHub/MoosicBoxUI/src/pages/downloads.astro";
const $$url = "/downloads";

const _page = /*#__PURE__*/Object.freeze(/*#__PURE__*/Object.defineProperty({
    __proto__: null,
    default: $$Downloads,
    file: $$file,
    url: $$url
}, Symbol.toStringTag, { value: 'Module' }));

const page = () => _page;

export { page };
//# sourceMappingURL=downloads.astro.mjs.map
