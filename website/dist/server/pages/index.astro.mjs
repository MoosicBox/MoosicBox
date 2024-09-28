import { c as createComponent, r as renderTemplate, d as renderComponent } from '../chunks/astro/server_BHkv7Nwt.mjs';
import { $ as $$Layout } from '../chunks/Layout_aLm9q08O.mjs';
import { ssr, ssrHydrationKey } from 'solid-js/web';
/* empty css                                 */
export { renderers } from '../renderers.mjs';

var _tmpl$ = ["<div", ' class="search-header-offset"></div>'], _tmpl$2 = ["<header", ' class="home-header"><h1>MoosicBox</h1></header>'];
function home() {
  return [ssr(_tmpl$, ssrHydrationKey()), ssr(_tmpl$2, ssrHydrationKey())];
}

const $$Index = createComponent(($$result, $$props, $$slots) => {
  return renderTemplate`${renderComponent($$result, "Layout", $$Layout, { "title": "MoosicBox" }, { "default": ($$result2) => renderTemplate` ${renderComponent($$result2, "HomePage", home, { "client:load": true, "client:component-hydration": "load", "client:component-path": "~/routes/(home).tsx", "client:component-export": "default" })} ` })}`;
}, "/home/bsteffaniak/GitHub/MoosicBoxUI/src/pages/index.astro", void 0);

const $$file = "/home/bsteffaniak/GitHub/MoosicBoxUI/src/pages/index.astro";
const $$url = "";

const _page = /*#__PURE__*/Object.freeze(/*#__PURE__*/Object.defineProperty({
    __proto__: null,
    default: $$Index,
    file: $$file,
    url: $$url
}, Symbol.toStringTag, { value: 'Module' }));

const page = () => _page;

export { page };
//# sourceMappingURL=index.astro.mjs.map
