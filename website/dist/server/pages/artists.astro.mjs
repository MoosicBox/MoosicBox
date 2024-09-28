import { c as createComponent, r as renderTemplate, d as renderComponent } from '../chunks/astro/server_BHkv7Nwt.mjs';
import { $ as $$Layout } from '../chunks/Layout_aLm9q08O.mjs';
export { renderers } from '../renderers.mjs';

const $$Artists = createComponent(($$result, $$props, $$slots) => {
  return renderTemplate`${renderComponent($$result, "Layout", $$Layout, { "title": "MoosicBox" }, { "default": ($$result2) => renderTemplate` ${renderComponent($$result2, "ArtistsRoute", null, { "client:only": true, "client:component-hydration": "only", "client:component-path": "~/routes/artists", "client:component-export": "default" })} ` })}`;
}, "/home/bsteffaniak/GitHub/MoosicBoxUI/src/pages/artists.astro", void 0);

const $$file = "/home/bsteffaniak/GitHub/MoosicBoxUI/src/pages/artists.astro";
const $$url = "/artists";

const _page = /*#__PURE__*/Object.freeze(/*#__PURE__*/Object.defineProperty({
    __proto__: null,
    default: $$Artists,
    file: $$file,
    url: $$url
}, Symbol.toStringTag, { value: 'Module' }));

const page = () => _page;

export { page };
//# sourceMappingURL=artists.astro.mjs.map
